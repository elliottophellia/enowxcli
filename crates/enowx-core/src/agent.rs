//! The core agent loop: stream a model call, persist it, execute every tool call,
//! append paired results, then continue until the model yields or a cap stops it.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context as _, Result};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    config::Config,
    event::Event,
    message::{Attachment, Message, Role as MessageRole},
    provider::{Chunk, Provider},
    role::Role,
    session::{Session, SessionStore},
    tools::{mcp_proxy::McpProxyTool, skill::SkillReadTool, ToolCtx, ToolOutput, ToolRegistry},
};

use crate::{discovery::Discovery, mcp::McpClient};

/// The report a delegated sub-agent owes its caller. Carried in the sub-agent's
/// system prompt — not appended to the task — so it holds a fixed position at
/// the front of the context instead of trailing whatever the task happened to
/// say, where a long brief buried it and the sub-agent stopped without one.
const REPORT_CONTRACT: &str = "\nYou are working on behalf of another agent, which cannot see anything \
     you do — only your final message. It cannot answer questions: decide, and say what you decided.\n\
     Do the task you were given and stop; the caller decides what comes next.\n\
     End every turn with this report, one line per field, and nothing after it:\n\
     DONE: what you achieved, or what you could not\n\
     CHANGED: every file you created or edited, or `none`\n\
     VERIFIED: how you checked it (the project's build or tests, or reading the result back) and what you found, or `not verified`\n\
     NEXT: what the caller must know to carry on, or `nothing`\n\
     Other agents may be working at the same time. A file one of them is editing is refused to you \
     until it finishes: do your other files first, and if you cannot finish without it, say so in NEXT.\n";

/// The workspace's top level, for the system prompt.
///
/// Nearly every run opened with a call to see what the workspace holds: the
/// router's `glob **/*`, a specialist's `ls -la`, each a model call that
/// re-sends the whole context. Forty names cost a few dozen tokens once.
fn workspace_overview(workspace: &std::path::Path) -> String {
    const SHOWN: usize = 40;
    const SKIPPED: [&str; 6] = [
        ".git",
        ".enx-sessions",
        "node_modules",
        "target",
        ".DS_Store",
        ".next",
    ];
    let mut names: Vec<String> = std::fs::read_dir(workspace)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if SKIPPED.contains(&name.as_str()) {
                        return None;
                    }
                    Some(if entry.path().is_dir() {
                        format!("{name}/")
                    } else {
                        name
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    if names.is_empty() {
        return "Its top level is empty: there is nothing to look at yet.".to_owned();
    }
    names.sort();
    let total = names.len();
    names.truncate(SHOWN);
    let more = if total > SHOWN {
        format!(", and {} more", total - SHOWN)
    } else {
        String::new()
    };
    format!(
        "Its top level, so you need not list it: {}{more}",
        names.join("  ")
    )
}

/// How much work a task gets, and with which tools. Written from what the
/// agents actually did: `bash ls`/`cat`/`find` where glob and read fit, a
/// checklist for a two-file page with every item ticked in its own call, six
/// skills loaded before any work, throwaway Python to check a stylesheet.
/// Each of those is a model call that re-sends the whole context.
const EFFORT_RULES: &str = "\
Effort and tools:\n\
- Match effort to the task. A small task (a page, a fix in one or two files) is: look, write, check once, report. Most tasks need a handful of tool calls.\n\
- Use the dedicated tools: `glob` to list or find files, `grep` to search contents, `read` to read (offset and limit for a range). `bash` is for building, running, installing and testing, never for ls, find, cat, head, sed or grep.\n\
- Calls that do not depend on each other go in one step, as several tool calls in the same reply: the files to read, the searches, the screenshots to look at, a build beside them. They run at the same time; one per step makes the user wait a model call for each. Only a call that needs another's result waits for the next step.\n\
- A picture you have read stays in view: look back at it instead of reading it again.\n\
- Create a file whole with one `write`. Change an existing file with `edit`, and several changes to one file with one `multi_edit`, not one call each; do not rewrite a file to change a detail of it.\n\
- Verify with what the project already has (its build, tests, linter) or by reading the result. Do not write throwaway scripts (a python heredoc, an ad-hoc validator) to check your own output.\n\
- Leave nothing running: no servers or background processes (`&`, nohup) that outlive the command that started them.\n\
- `todo` is for work of four or more steps. Set the list once and mark finished steps together; skip it for small tasks.\n\
- Read a skill only when the task needs its instructions: the one your instructions name for the work, or the one that applies. Never every skill listed.\n\
- When a detail is open and a sensible default exists, choose it and say what you chose. Ask the user (the `ask` tool, when you have it) before something that cannot be undone, or when a choice changes what you build and neither the request nor the project settles it: every question you have in one `ask`, each with options, the one you recommend first.\n\
- Stop when the request is met. Do not add files, features or polish nobody asked for.\n\
- Files: new ones stay under about 300 lines (components about 200) unless splitting would scatter one idea; never split or restructure an existing file on your own, however long: ask first with `ask`, or propose it in your report.\n\
- When you finish and answer the user (a delegated report has its own form), keep it short: at most six bullets of one line each, saying what you did or found, how you checked it, and what is left to the user (placeholders, decisions, anything unverified). No paragraphs, no retelling of the design or the conversation, no list of every file: the rows above hold the detail.\n";

/// Build the tool registry with skill discovery. MCP servers are spawned lazily
/// via `Agent::start_mcp` so a synchronous `Agent::new` cannot deadlock the
/// tokio runtime with a nested `block_on`.
fn build_registry(discovery: Arc<Discovery>, disabled_skills: Vec<String>) -> ToolRegistry {
    let mut registry = ToolRegistry::default();
    let active = discovery
        .skills
        .iter()
        .filter(|s| !disabled_skills.iter().any(|d| d == &s.name))
        .count();
    if active > 0 {
        registry.register(SkillReadTool::with_disabled(
            discovery.clone(),
            disabled_skills,
        ));
    }
    registry.register(crate::tools::skill::SkillBindTool::new(discovery));
    registry
}
/// How long a turn waits for MCP servers still starting before it calls the
/// model without their tools. They keep starting and join a later step.
const MCP_GRACE: std::time::Duration = std::time::Duration::from_secs(2);

/// A server that has not finished its handshake and listed its tools in this
/// long is left out, and the reason recorded.
const MCP_START_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// The registry for writing, taken only when no one holds it.
///
/// Never queues. Tokio's lock is write-preferring, so a queued writer stops
/// every new reader, and a turn keeps a read guard while it delegates and
/// the sub-agent's turn takes another: a writer queued between the two would
/// wait on the first guard while the second waited on it. Tools started in
/// the background join between turns instead.
async fn write_when_free(
    tools: &tokio::sync::RwLock<ToolRegistry>,
) -> tokio::sync::RwLockWriteGuard<'_, ToolRegistry> {
    loop {
        if let Ok(guard) = tools.try_write() {
            return guard;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

/// Start every enabled MCP server at once, registering each one's tools the
/// moment it is ready. Servers used to start one after another before the
/// first model call: one that never answered its handshake (a `dokploy`
/// server whose backend was gone) held every first message for 30 seconds.
async fn start_mcp_servers(
    tools: Arc<tokio::sync::RwLock<ToolRegistry>>,
    discovery: Arc<Discovery>,
    clients: Arc<tokio::sync::Mutex<Vec<Arc<McpClient>>>>,
    failures: Arc<std::sync::Mutex<std::collections::BTreeMap<String, String>>>,
) {
    use futures::stream::{FuturesUnordered, StreamExt as _};
    let mut starting: FuturesUnordered<_> = discovery
        .mcp_servers
        .iter()
        .filter(|server| server.enabled)
        .map(|server| async move {
            let outcome = tokio::time::timeout(MCP_START_TIMEOUT, async {
                let client = Arc::new(McpClient::spawn(server).await?);
                let listed = client.list_tools().await?;
                anyhow::Ok((client, listed))
            })
            .await;
            (server, outcome)
        })
        .collect();
    while let Some((server, outcome)) = starting.next().await {
        let name = server.name.clone();
        match outcome {
            Ok(Ok((client, listed))) => {
                {
                    let mut registry = write_when_free(&tools).await;
                    for tool in listed {
                        registry.register(McpProxyTool::new(client.clone(), &tool));
                    }
                }
                clients.lock().await.push(client);
            }
            Ok(Err(error)) => {
                // The cause, not the context around it: "spawning MCP server
                // `x`" says nothing a row with the server's name does not.
                let missing = error
                    .chain()
                    .filter_map(|cause| cause.downcast_ref::<std::io::Error>())
                    .any(|io| io.kind() == std::io::ErrorKind::NotFound);
                let reason = if missing {
                    format!("command not found: {}", server.command_or_url)
                } else {
                    error.root_cause().to_string()
                };
                if let Ok(mut failed) = failures.lock() {
                    failed.insert(name, reason);
                }
            }
            Err(_) => {
                if let Ok(mut failed) = failures.lock() {
                    failed.insert(
                        name,
                        format!("no answer in {}s", MCP_START_TIMEOUT.as_secs()),
                    );
                }
            }
        }
    }
}

pub struct RunRequest {
    pub session_id: Option<String>,
    pub prompt: String,
    pub role: Role,
    /// Images the user attached to this message.
    pub attachments: Vec<Attachment>,
    /// The agent a new session starts with, when the user picked one before
    /// the first message. Ignored when `session_id` names a session, which
    /// keeps its own.
    pub agent: Option<String>,
}

/// Summary row shown by the TUI for one proxied MCP tool.
#[derive(Debug, Clone)]
pub struct McpToolSummary {
    pub name: String,
    pub description: String,
}

fn sanitize(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// Rough char-based token estimate for the whole session content. Not an
/// exact tokenizer, but stable enough for an auto-compact threshold: ~4
/// chars per token averages out across natural prose and code.
fn estimate_session_tokens(session: &crate::session::Session) -> f32 {
    let mut chars: usize = 0;
    for t in &session.turns {
        chars += t.message.content.len();
        if let Some(r) = &t.message.reasoning {
            chars += r.len();
        }
        for tc in &t.message.tool_calls {
            chars += tc.arguments.len();
        }
    }
    (chars as f32) / 4.0
}

pub struct Agent {
    config: Config,
    tools: Arc<tokio::sync::RwLock<ToolRegistry>>,
    store: SessionStore,
    discovery: Arc<Discovery>,
    /// Kept alive so child processes survive for the agent's lifetime;
    /// filled by `Agent::start_mcp` as servers come up.
    #[allow(dead_code)]
    mcp_clients: Arc<tokio::sync::Mutex<Vec<Arc<McpClient>>>>,
    mcp_warmed: Arc<std::sync::atomic::AtomicBool>,
    /// Set once every server has started or failed; `mcp_ready` wakes
    /// anyone waiting on that.
    mcp_done: Arc<std::sync::atomic::AtomicBool>,
    mcp_ready: Arc<tokio::sync::Notify>,
    /// When the servers began starting. The grace a turn waits for them is
    /// counted from here, so only a message sent in the first moments after
    /// launch waits at all.
    mcp_started_at: Arc<std::sync::OnceLock<std::time::Instant>>,
    /// Why each server that did not start failed, by name.
    mcp_failures: Arc<std::sync::Mutex<std::collections::BTreeMap<String, String>>>,
    /// `None` unless a TypeSafe key is configured, in which case small typed
    /// judgements are available to the harness.
    system_one: Option<crate::systemone::SystemOne>,
    /// Whether the host can put a question to the user and send the answer
    /// back. Off unless the host says so, so an agent run where no one can
    /// answer (the HTTP server, a script) is never left waiting.
    asks_user: bool,
    /// Questions waiting on the user, by the id of the call that asked.
    questions: std::sync::Mutex<
        std::collections::HashMap<String, tokio::sync::oneshot::Sender<crate::ask::Answer>>,
    >,
    /// Which files the agents of the current run are editing, so two of
    /// them working at once never edit the same file.
    board: crate::contract::Board,
    /// Language servers started for this workspace, shared by every agent
    /// of the run.
    lsp: Arc<crate::lsp::Lsp>,
    /// This agent, when the host holds it in an `Arc` (`into_shared`), so a
    /// turn can leave delegations running after it ends.
    me: std::sync::Weak<Agent>,
    /// Where delegations that outlive their turn report, when the host
    /// listens (`background_events`).
    background: std::sync::Mutex<Option<mpsc::Sender<Event>>>,
    /// Batches of delegations still running in the background.
    waiting: Arc<std::sync::atomic::AtomicUsize>,
}

impl Agent {
    pub fn new(config: Config) -> Self {
        let discovery = Arc::new(Discovery::run(&config.workspace()));
        Self::assemble(config, SessionStore::default(), discovery)
    }

    pub fn with_store(config: Config, store: SessionStore) -> Self {
        let discovery = Arc::new(Discovery::run(&config.workspace()));
        Self::assemble(config, store, discovery)
    }

    /// Build an agent with a discovery result supplied rather than read from
    /// disk. Tests use it with `Discovery::default()` so a run never spawns the
    /// developer's real MCP servers from `~/.mcp.json` — warming those took
    /// tens of seconds per turn and hung or failed on a machine without them,
    /// which read as a cancellation bug rather than the environment leak it was.
    pub fn with_discovery(config: Config, store: SessionStore, discovery: Discovery) -> Self {
        Self::assemble(config, store, Arc::new(discovery))
    }

    fn assemble(config: Config, store: SessionStore, discovery: Arc<Discovery>) -> Self {
        let disabled = config.ui.disabled_skills.clone();
        let registry = build_registry(discovery.clone(), disabled);
        let system_one = crate::systemone::SystemOne::new(&config.typesafe);
        let config_workspace = config.workspace();
        Self {
            config,
            tools: Arc::new(tokio::sync::RwLock::new(registry)),
            store,
            discovery,
            mcp_clients: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            mcp_warmed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            mcp_done: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            mcp_ready: Arc::new(tokio::sync::Notify::new()),
            mcp_started_at: Arc::new(std::sync::OnceLock::new()),
            mcp_failures: Arc::new(std::sync::Mutex::new(std::collections::BTreeMap::new())),
            system_one,
            asks_user: false,
            questions: std::sync::Mutex::new(std::collections::HashMap::new()),
            board: crate::contract::Board::default(),
            lsp: Arc::new(crate::lsp::Lsp::new(config_workspace)),
            me: std::sync::Weak::new(),
            background: std::sync::Mutex::new(None),
            waiting: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// Hold the agent in an `Arc` that it knows about, so the orchestrator's
    /// delegations can run on after its turn ends.
    pub fn into_shared(mut self) -> Arc<Self> {
        Arc::new_cyclic(|me| {
            self.me = me.clone();
            self
        })
    }

    /// Listen for what delegations running in the background send: each
    /// one's `DelegationFinished`, then `DelegationsReported` when a batch is
    /// done. Until a host listens (and the agent is shared), delegations run
    /// inside the turn that started them.
    pub fn background_events(&self) -> mpsc::Receiver<Event> {
        let (tx, rx) = mpsc::channel(256);
        if let Ok(mut slot) = self.background.lock() {
            *slot = Some(tx);
        }
        rx
    }

    /// How many batches of delegations are still running in the background.
    pub fn delegations_running(&self) -> usize {
        self.waiting.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Let the agent holding the conversation ask the user questions. The
    /// host shows each `Event::Question` and answers it with `answer`.
    pub fn asking_user(mut self) -> Self {
        self.asks_user = true;
        self
    }

    /// Answer the question asked with `id`. False when nothing is waiting on
    /// it: already answered, or the turn was stopped.
    pub fn answer(&self, id: &str, answer: crate::ask::Answer) -> bool {
        let waiting = self.questions.lock().expect("questions").remove(id);
        waiting.is_some_and(|reply| reply.send(answer).is_ok())
    }

    /// Put the questions to the user and wait for the answers, or for the
    /// turn to be stopped.
    async fn ask_user(
        &self,
        id: &str,
        agent: &str,
        args: &serde_json::Value,
        holds_conversation: bool,
        events: &mpsc::Sender<Event>,
        cancel: &CancellationToken,
    ) -> ToolOutput {
        if !self.asks_user || !holds_conversation {
            return ToolOutput::error(
                "There is no user to ask here. Decide, and say what you decided and why.",
            );
        }
        let questions = match crate::ask::parse(args) {
            Ok(questions) => questions,
            Err(why) => return ToolOutput::error(why),
        };
        let (reply, answer) = tokio::sync::oneshot::channel();
        self.questions
            .lock()
            .expect("questions")
            .insert(id.to_owned(), reply);
        let _ = events
            .send(Event::Question {
                id: id.to_owned(),
                agent: agent.to_owned(),
                questions: questions.clone(),
            })
            .await;
        let answer = tokio::select! {
            answer = answer => answer.ok(),
            _ = cancel.cancelled() => None,
        };
        self.questions.lock().expect("questions").remove(id);
        match answer {
            Some(answer) => ToolOutput::ok(crate::ask::answer_message(&questions, &answer)),
            None => ToolOutput::error("The user stopped the turn instead of answering."),
        }
    }

    /// The judge used to rescue still-live turns from a compaction, or `None`
    /// when the feature is off or unconfigured — in which case compaction is
    /// the positional fold it has always been.
    fn ranking_judge(&self) -> Option<&crate::systemone::SystemOne> {
        self.config
            .typesafe
            .rank_compaction
            .then_some(self.system_one.as_ref())
            .flatten()
    }

    /// Spawn every discovered MCP server on the current tokio runtime. Safe to
    /// call more than once: the second call is a no-op. Called on the first
    /// turn so `Agent::new` stays sync-friendly.
    /// Start the MCP servers in the background, once. The interface calls
    /// this as soon as the agent exists, so the servers are usually up
    /// before the first message; a turn calls it too, in case nothing has.
    /// Does nothing outside a Tokio runtime.
    pub fn start_mcp(&self) {
        use std::sync::atomic::Ordering;
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        if self.mcp_warmed.swap(true, Ordering::AcqRel) {
            return;
        }
        let _ = self.mcp_started_at.set(std::time::Instant::now());
        let tools = self.tools.clone();
        let discovery = self.discovery.clone();
        let clients = self.mcp_clients.clone();
        let failures = self.mcp_failures.clone();
        let done = self.mcp_done.clone();
        let ready = self.mcp_ready.clone();
        runtime.spawn(async move {
            start_mcp_servers(tools, discovery, clients, failures).await;
            done.store(true, Ordering::Release);
            ready.notify_waiters();
        });
    }

    /// Wait for the MCP servers to finish starting, until `grace` after
    /// they began. A server that hangs is waited on once, briefly, not on
    /// every turn until it gives up.
    async fn wait_for_mcp(&self, grace: std::time::Duration) {
        use std::sync::atomic::Ordering;
        let notified = self.mcp_ready.notified();
        if self.mcp_done.load(Ordering::Acquire) {
            return;
        }
        let Some(started) = self.mcp_started_at.get() else {
            return;
        };
        let Some(left) = grace.checked_sub(started.elapsed()) else {
            return;
        };
        let _ = tokio::time::timeout(left, notified).await;
    }

    /// Whether the MCP servers are still starting.
    pub fn mcp_starting(&self) -> bool {
        use std::sync::atomic::Ordering;
        self.mcp_warmed.load(Ordering::Acquire) && !self.mcp_done.load(Ordering::Acquire)
    }

    /// Why a server failed to start, when it did.
    pub fn mcp_failure(&self, server: &str) -> Option<String> {
        self.mcp_failures
            .lock()
            .ok()
            .and_then(|failed| failed.get(server).cloned())
    }

    /// The agent holding `session`, falling back to the orchestrator.
    ///
    /// A session naming an agent that has since been deleted from disk would
    /// otherwise have no prompt at all; routing from the orchestrator is a
    /// better failure than running with an empty system message.
    fn active_agent(&self, session: &Session) -> crate::agent_def::AgentDef {
        let wanted = session.agent_or_default();
        let roster = &self.discovery.agents;
        roster
            .iter()
            .find(|a| a.name == wanted)
            .or_else(|| {
                roster
                    .iter()
                    .find(|a| a.name == crate::agent_def::ORCHESTRATOR)
            })
            .cloned()
            .unwrap_or_else(|| {
                crate::agent_def::builtin_agents()
                    .into_iter()
                    .find(|a| a.name == crate::agent_def::ORCHESTRATOR)
                    .expect("the orchestrator always ships")
            })
    }

    /// Assemble an agent's system prompt: shared rules, the agent's own
    /// instructions, and — for anyone who may delegate — the roster it routes
    /// to.
    ///
    /// The roster is only included for agents that can act on it. A specialist
    /// reading a list of colleagues it may not call would just be noise in its
    /// window.
    fn agent_prompt(&self, agent: &crate::agent_def::AgentDef, workspace: &str) -> String {
        let overview = workspace_overview(std::path::Path::new(workspace));
        let mut prompt = format!(
            "You are a tool-using engineering agent. Workspace root: {workspace}\n\
             {overview}\n\
             \n\
             Rules that hold for every agent:\n\
             - Use tools to establish facts. Never claim you read a file, ran a command, or fetched a page unless the tool result is in this conversation.\n\
             - Paths are relative to the workspace root. Reads and writes outside it are refused.\n\
             - Prefer one precise tool call over several speculative ones. Stop calling tools once you can answer.\n\
             - When a tool fails, read the error and change approach instead of repeating the same call.\n\
             - Verify with the project's own commands (its build, typecheck, tests, linter) and your tools. Do not write probe scripts, isolated builds or experiments to investigate the toolchain; when something fails outside your files or your task, report it and stop.\n\
             - Answer in the user's language. Be concrete: exact paths, symbols, and commands.\n\
             \n\
             {EFFORT_RULES}\n\
             You are `{}`.\n{}\n",
            agent.name, agent.prompt
        );
        if agent.delegation != crate::agent_def::Delegation::None {
            prompt.push_str(&self.roster_block(agent));
        }
        if agent.tools.iter().any(|t| t == "edit_lines") {
            prompt.push_str(
                "\nEditing code: `read` shows each line as `12#a3f:text`. To change lines of a file \
                 you have read, use `edit_lines` with those anchors: no old text to repeat, and a \
                 stale anchor is caught. Use `edit` for a short replacement of unique text, and \
                 `write` for a new file. For a symbol, ask `lsp` (definition, references, hover, \
                 rename) before searching text. Every change is checked against the project's \
                 rules and its syntax: a refused change says why; fix it and send it again.\n",
            );
        }
        if !self.config.agent.preview && agent.tools.iter().any(|t| t == "preview") {
            prompt.push_str(
                "\nThe user turned `preview` off: do not look at pages in a browser. Check \
                 interface work with `ui_check` and by reading the code, and say in your report \
                 that it was not looked at.\n",
            );
        }
        if self.config.agent.lsp && agent.tools.iter().any(|t| t == "diagnostics") {
            prompt.push_str(
                "\nWhen `write` or `edit` reports errors from the language server, fix them \
                 before you move on; warnings too when they are lint on code you wrote. When the \
                 server was still checking, call `diagnostics` on the file before you report the \
                 work done.\n",
            );
        }
        let carried = self.discovery.carried_by(agent);
        if let Some(extra) = self
            .discovery
            .system_prompt_for(&self.config.ui.disabled_skills, Some(&carried))
        {
            prompt.push('\n');
            prompt.push_str(&extra);
        }
        // Only the orchestrator decides who gets a skill found on disk.
        if agent.delegation == crate::agent_def::Delegation::Orchestrator {
            if let Some(block) = self
                .discovery
                .local_skills_block(&self.config.ui.disabled_skills)
            {
                prompt.push_str(&block);
            }
        }
        prompt
    }

    /// The one-line-per-agent listing a delegating agent chooses from.
    ///
    /// Descriptions are written to be matched against a request rather than
    /// read, which is what keeps the whole roster to a couple of hundred
    /// tokens — about two file reads, against a session that will spend tens
    /// of thousands.
    fn roster_block(&self, asking: &crate::agent_def::AgentDef) -> String {
        let mut out = String::from("\nAgents you may delegate to:\n");
        for agent in &self.discovery.agents {
            // Never offer an agent the asker cannot actually reach, or itself.
            if agent.name == asking.name
                || !agent.is_routable()
                || agent.name == crate::agent_def::ORCHESTRATOR
                || !asking.delegation.may_call(&agent.name)
            {
                continue;
            }
            out.push_str(&format!("- {}: {}\n", agent.name, agent.description));
        }
        out
    }

    pub fn discovery(&self) -> Arc<Discovery> {
        self.discovery.clone()
    }

    /// Snapshot of every proxy tool the registry currently exposes for one
    /// MCP server. Returns `(name, description)` pairs, stripped of the
    /// `mcp__<server>__` prefix.
    pub fn mcp_tools(&self, server: &str) -> Vec<crate::agent::McpToolSummary> {
        let prefix = format!("mcp__{}__", sanitize(server));
        let Ok(registry) = self.tools.try_read() else {
            return Vec::new();
        };
        registry
            .keys()
            .filter_map(|(name, desc)| {
                name.strip_prefix(&prefix).map(|short| McpToolSummary {
                    name: short.to_string(),
                    description: desc.clone(),
                })
            })
            .collect()
    }

    pub fn config(&self) -> &Config {
        &self.config
    }
    pub fn store(&self) -> &SessionStore {
        &self.store
    }

    /// Manually compact a session: fold older turns into a summary. Returns
    /// the summary text when work was done; `None` when the session was too
    /// short to compact or the summarizer produced nothing.
    pub async fn compact(&self, session_id: &str) -> Result<Option<String>> {
        let mut session = self.store.load(session_id)?;
        let provider = Provider::from_config(&self.config)?;
        let keep = self.config.agent.compact_keep_last.max(1);
        let summary =
            crate::compact::compact_with(&mut session, &provider, keep, self.ranking_judge())
                .await?;
        if summary.is_some() {
            self.store.save(&session)?;
        }
        Ok(summary)
    }

    pub async fn run(
        &self,
        request: RunRequest,
        events: mpsc::Sender<Event>,
        cancel: CancellationToken,
    ) -> Result<()> {
        let outcome = self.run_inner(request, &events, cancel).await;
        // The run is over and every agent in it has finished, unless some
        // still work in the background and hold files on the board.
        if self.delegations_running() == 0 {
            self.board.clear();
        }
        match outcome {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = events
                    .send(Event::Error {
                        message: format!("{error:#}"),
                    })
                    .await;
                Err(error)
            }
        }
    }

    /// Run one delegation in a branch session and return its summary.
    ///
    /// The branch keeps its own transcript so the work can be inspected, while
    /// only the summary reaches the caller's context — that is the whole point
    /// of delegating rather than handing over. Its token spend rolls up into
    /// the parent, because a branch whose cost is not counted makes the readout
    /// wrong rather than merely incomplete.
    /// Move to the next model on the ladder, or None at the bottom.
    ///
    /// A tier drop is announced rather than made quietly: a cheaper model may
    /// not be up to the task, and work that silently got worse is harder to
    /// diagnose than work that failed.
    async fn step_down(
        &self,
        ladder: &mut crate::provider::ModelLadder,
        agent_config: &mut Config,
        error: &anyhow::Error,
        events: &mpsc::Sender<Event>,
    ) -> Option<Provider> {
        if !crate::provider::classify(error).another_model_helps() {
            return None;
        }
        let previous = ladder.tier();
        // A rung on a provider with no key is passed over: it would only
        // fail again, on the key rather than the model.
        let step = loop {
            let step = ladder.next(&self.config)?;
            if agent_model_unusable(agent_config, &step.model).is_none() {
                break step;
            }
        };
        let drop = crate::provider::tier_drop(previous, &step, error);
        let _ = events
            .send(Event::Notice {
                message: drop.message(),
            })
            .await;
        agent_config.use_model(&step.model);
        Provider::from_config(agent_config).ok()
    }

    /// Fold older steps of this turn into a summary when the session is near
    /// the window, keeping a delegated agent's brief word for word.
    async fn compact_mid_turn(
        &self,
        session: &mut Session,
        agent_config: &Config,
        events: &mpsc::Sender<Event>,
    ) {
        if !self.config.agent.auto_compact || self.config.agent.auto_compact_at <= 0.0 {
            return;
        }
        let window = agent_config
            .model
            .context_window
            .max(self.config.model.context_window)
            .max(1) as f32;
        if estimate_session_tokens(session) / window < self.config.agent.auto_compact_at {
            return;
        }
        let brief = session
            .turns
            .first()
            .filter(|turn| turn.message.role == MessageRole::User)
            .cloned();
        let Ok(provider) = Provider::from_config(agent_config) else {
            return;
        };
        let keep = self.config.agent.compact_keep_last.max(6);
        let _ = events
            .send(Event::Notice {
                message: "compacting mid-turn to stay inside the context window…".into(),
            })
            .await;
        if let Ok(Some(_)) =
            crate::compact::compact_with(session, &provider, keep, self.ranking_judge()).await
        {
            if let Some(brief) = brief {
                if !session.turns.iter().any(|turn| turn.id == brief.id) {
                    session.turns.insert(0, brief);
                }
            }
            let _ = self.store.save(session);
        }
    }

    /// Run the delegations asked for in one step, all at the same time, each in
    /// its own branch session, and return their summaries in the same order.
    ///
    /// Several may go to the same agent (three `fe` tasks for three pages).
    /// Agents at work together keep to the contract in `crate::contract`: a
    /// file one of them is editing is closed to the others until it finishes.
    async fn run_delegations(
        &self,
        parent: &mut Session,
        delegations: &[crate::routing::Delegation],
        events: &mpsc::Sender<Event>,
    ) -> Vec<crate::event::DelegationReport> {
        let started = self.start_branches(parent, delegations, events).await;
        let reports = self.finish_branches(delegations, started, events).await;
        self.absorb_reports(parent, &reports);
        reports
    }

    /// Start a branch for each delegation: records, events and the lineage the
    /// contract uses, so the runs themselves share nothing mutable. A branch
    /// that could not start is its failure summary.
    async fn start_branches(
        &self,
        parent: &mut Session,
        delegations: &[crate::routing::Delegation],
        events: &mpsc::Sender<Event>,
    ) -> Vec<Result<Session, crate::event::DelegationReport>> {
        let mut started = Vec::new();
        for delegation in delegations {
            // Continuing an earlier delegation: its own session, as it left it.
            if let Some(id) = &delegation.resume {
                let resumed = self.store.load(id).ok().filter(|branch| {
                    branch.parent.as_deref() == Some(parent.id.as_str())
                        && crate::agent_def::canonical_name(&branch.agent)
                            == crate::agent_def::canonical_name(&delegation.to)
                });
                let Some(branch) = resumed else {
                    let summary = format!(
                        "failed before changing anything: could not resume `{id}`: no delegation \
                         of this conversation to `{}` has that session",
                        delegation.to
                    );
                    let _ = events
                        .send(Event::DelegationFinished {
                            agent: delegation.to.clone(),
                            summary: summary.clone(),
                            session_id: id.clone(),
                            failed: true,
                        })
                        .await;
                    started.push(Err(crate::event::DelegationReport {
                        agent: delegation.to.clone(),
                        session_id: id.clone(),
                        summary,
                        failed: true,
                    }));
                    continue;
                };
                self.board.branch(&branch.id, &parent.id);
                if let Some(record) = parent
                    .delegations
                    .iter_mut()
                    .rev()
                    .find(|record| record.session_id == branch.id)
                {
                    record.failed = false;
                }
                let _ = events
                    .send(Event::DelegationStarted {
                        agent: delegation.to.clone(),
                        task: delegation.task.clone(),
                        session_id: branch.id.clone(),
                    })
                    .await;
                started.push(Ok(branch));
                continue;
            }
            let branch = parent.branch(&delegation.to);
            if let Err(error) = self.store.save(&branch) {
                // Said on screen as well as to the caller: the delegate call
                // itself went through, so without this the failure shows nowhere.
                let summary =
                    format!("failed before changing anything: could not start: {error:#}");
                let _ = events
                    .send(Event::DelegationFinished {
                        agent: delegation.to.clone(),
                        summary: summary.clone(),
                        session_id: branch.id.clone(),
                        failed: true,
                    })
                    .await;
                started.push(Err(crate::event::DelegationReport {
                    agent: delegation.to.clone(),
                    session_id: branch.id.clone(),
                    summary,
                    failed: true,
                }));
                continue;
            }
            self.board.branch(&branch.id, &parent.id);
            parent.delegations.push(crate::session::DelegationRecord {
                agent: delegation.to.clone(),
                session_id: branch.id.clone(),
                failed: false,
            });
            let _ = events
                .send(Event::DelegationStarted {
                    agent: delegation.to.clone(),
                    task: delegation.task.clone(),
                    session_id: branch.id.clone(),
                })
                .await;
            started.push(Ok(branch));
        }
        started
    }

    /// Run the started branches together and say how each ended.
    async fn finish_branches(
        &self,
        delegations: &[crate::routing::Delegation],
        started: Vec<Result<Session, crate::event::DelegationReport>>,
        events: &mpsc::Sender<Event>,
    ) -> Vec<crate::event::DelegationReport> {
        let runs = delegations
            .iter()
            .zip(&started)
            .map(|(delegation, branch)| async move {
                let Ok(branch) = branch else {
                    return None;
                };
                // The task alone. The report the sub-agent owes (DONE/CHANGED/…)
                // rides in its system prompt via REPORT_CONTRACT, keyed off the
                // branch's `parent`.
                let request = RunRequest {
                    prompt: delegation.task.clone(),
                    session_id: Some(branch.id.clone()),
                    role: branch.role,
                    attachments: Vec::new(),
                    agent: None,
                };
                // The sub-agent's own events are not forwarded: the caller sees a
                // summary, and interleaving several agents' tool calls in one
                // transcript is unreadable. The branch session holds the detail.
                let (sink, mut drain) = mpsc::channel::<Event>(64);
                tokio::spawn(async move { while drain.recv().await.is_some() {} });
                // Its own token, not a child of the caller's: stopping the turn
                // stops the agent the user talks to, and a sub-agent cut off
                // mid-task would leave its caller a `NO REPORT` for work that was
                // interrupted rather than failed.
                let own = CancellationToken::new();
                let outcome = Box::pin(self.run_inner(request, &sink, own)).await;
                // Finished: what it was editing is open to the others again.
                self.board.release(&branch.id);
                Some(outcome)
            });
        let outcomes = futures::future::join_all(runs).await;

        let mut reports = Vec::new();
        for ((delegation, branch), outcome) in delegations.iter().zip(started).zip(outcomes) {
            let (branch, outcome) = match (branch, outcome) {
                (Err(report), _) => {
                    reports.push(report);
                    continue;
                }
                (Ok(branch), Some(outcome)) => (branch, outcome),
                (Ok(_), None) => unreachable!("a started branch always runs"),
            };
            // Reload: the nested run owns the branch on disk from here.
            let branch = self.store.load(&branch.id).unwrap_or(branch);
            let summary = match outcome {
                Ok(()) => branch_summary(&branch),
                Err(error) => {
                    // Whether files were already changed decides what the
                    // caller may safely do next, so the report says which.
                    let touched = files_touched(&branch);
                    if touched.is_empty() {
                        format!("failed before changing anything: {error:#}")
                    } else {
                        format!(
                            "PARTIAL FAILURE: {error:#}\nAlready changed: {}\nDo not redo this work.",
                            touched.join(", ")
                        )
                    }
                }
            };
            let failed = summary.starts_with("failed") || summary.starts_with("PARTIAL FAILURE");
            let _ = events
                .send(Event::DelegationFinished {
                    agent: delegation.to.clone(),
                    summary: summary.clone(),
                    session_id: branch.id.clone(),
                    failed,
                })
                .await;
            reports.push(crate::event::DelegationReport {
                agent: delegation.to.clone(),
                session_id: branch.id.clone(),
                summary,
                failed,
            });
        }
        reports
    }

    /// Roll finished branches into the session that delegated: their token
    /// spend, and which of them failed.
    fn absorb_reports(&self, parent: &mut Session, reports: &[crate::event::DelegationReport]) {
        for report in reports {
            if let Ok(branch) = self.store.load(&report.session_id) {
                parent.absorb_usage(&branch.usage);
            }
            if let Some(record) = parent
                .delegations
                .iter_mut()
                .rev()
                .find(|record| record.session_id == report.session_id)
            {
                record.failed = report.failed;
            }
        }
    }

    /// Whether this turn's delegations can run on after it: the host holds the
    /// agent in an `Arc` and listens on the background channel.
    fn can_delegate_in_background(&self) -> bool {
        self.me.upgrade().is_some()
            && self
                .background
                .lock()
                .map(|slot| slot.is_some())
                .unwrap_or(false)
    }

    /// Leave started branches running after this turn ends. Their reports come
    /// back on the background channel together, as `DelegationsReported`, for
    /// the host to wake the orchestrator with.
    ///
    /// A plain function, not `async`: the task it spawns runs the branches,
    /// whose turns may come back here, and a future that contained itself
    /// could not be proven safe to send between threads.
    fn continue_in_background(
        &self,
        parent_id: String,
        delegations: Vec<crate::routing::Delegation>,
        started: Vec<Result<Session, crate::event::DelegationReport>>,
    ) {
        let (Some(me), Some(background)) = (
            self.me.upgrade(),
            self.background.lock().ok().and_then(|slot| slot.clone()),
        ) else {
            return;
        };
        self.waiting
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let run: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> =
            Box::pin(async move {
                let reports = me.finish_branches(&delegations, started, &background).await;
                me.waiting.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                let _ = background
                    .send(Event::DelegationsReported {
                        session_id: parent_id,
                        reports,
                    })
                    .await;
            });
        tokio::spawn(run);
    }

    /// Wake the orchestrator with the reports of delegations that ran in the
    /// background: they go into its session as the reports of a delegation
    /// that ran inside the turn do, and its turn carries on from them.
    pub async fn resume_with_reports(
        &self,
        session_id: &str,
        role: Role,
        reports: Vec<crate::event::DelegationReport>,
        events: mpsc::Sender<Event>,
        cancel: CancellationToken,
    ) -> Result<()> {
        let mut session = self.store.load(session_id)?;
        self.absorb_reports(&mut session, &reports);
        for report in &reports {
            session.push(Message {
                role: MessageRole::User,
                content: crate::routing::report_message_for(
                    &report.agent,
                    &report.session_id,
                    &report.summary,
                ),
                reasoning: None,
                tool_calls: Vec::new(),
                attachments: Vec::new(),
                tool_call_id: None,
                interrupted: false,
                error: None,
                model: None,
                message_id: None,
            });
        }
        self.store.save(&session)?;
        self.run(
            RunRequest {
                prompt: String::new(),
                session_id: Some(session_id.to_owned()),
                role,
                attachments: Vec::new(),
                agent: None,
            },
            events,
            cancel,
        )
        .await
    }

    async fn run_inner(
        &self,
        request: RunRequest,
        events: &mpsc::Sender<Event>,
        cancel: CancellationToken,
    ) -> Result<()> {
        // A handoff ends the turn so the incoming agent can be assembled with
        // its own prompt, model and tools. Before this it also ended the
        // *reply*: the new agent waited for another message from the user,
        // who had no way to know one was wanted and saw the session stop
        // dead. Now it carries on.
        const MAX_HANDOFFS: usize = 4;
        let role = request.role;
        let mut next = self.run_turn(request, events, cancel.clone()).await?;
        for _ in 0..MAX_HANDOFFS {
            let Some(session_id) = next else {
                return Ok(());
            };
            // A turn that ends anywhere but its own end still gives the
            // conversation back: stopped before the incoming agent began,
            // or failed partway through its work.
            if cancel.is_cancelled() {
                self.give_back_saved(&session_id, events).await;
                return Ok(());
            }
            let turn = self
                .run_turn(
                    RunRequest {
                        prompt: String::new(),
                        session_id: Some(session_id.clone()),
                        role,
                        attachments: Vec::new(),
                        agent: None,
                    },
                    events,
                    cancel.clone(),
                )
                .await;
            next = match turn {
                Ok(next) => next,
                Err(error) => {
                    self.give_back_saved(&session_id, events).await;
                    return Err(error);
                }
            };
        }
        // Still handing over after four turns is a loop, not progress.
        if let Some(session_id) = next {
            self.give_back_saved(&session_id, events).await;
            let _ = events
                .send(Event::Notice {
                    message: "Agents kept handing over to each other; stopping.".into(),
                })
                .await;
            let _ = events
                .send(Event::Done {
                    stop_reason: "handoff_loop".into(),
                })
                .await;
        }
        Ok(())
    }

    /// One turn. Returns the id of the session it ran in when the turn ended
    /// in a handoff, so the caller can run the incoming agent straight away.
    ///
    /// `request.prompt` may be empty for a handoff continuation: the incoming
    /// agent picks up the conversation as it stands rather than being sent a
    /// new message.
    async fn run_turn(
        &self,
        request: RunRequest,
        events: &mpsc::Sender<Event>,
        cancel: CancellationToken,
    ) -> Result<Option<String>> {
        let prompt = request.prompt.trim();
        let continuing = prompt.is_empty() && request.session_id.is_some();
        if prompt.is_empty() && !continuing {
            anyhow::bail!("the message is empty");
        }
        if !self.config.is_ready() {
            anyhow::bail!(if self.config.has_connected_provider() {
                "Choose a model with /model."
            } else {
                "Connect a provider with /provider, or set ENX_BASE_URL, ENX_MODEL and ENX_API_KEY."
            });
        }
        let workspace = std::fs::canonicalize(self.config.workspace())
            .context("resolving the configured workspace")?;
        let mut session = match &request.session_id {
            Some(id) => {
                let session = self.store.load(id)?;
                if !session.workspace.as_os_str().is_empty() && session.workspace != workspace {
                    anyhow::bail!(
                        "session {} belongs to {}, but this server serves {}",
                        session.id,
                        session.workspace.display(),
                        workspace.display()
                    );
                }
                session
            }
            None => {
                let mut session = Session::new(request.role);
                session.workspace = workspace.clone();
                if let Some(agent) = &request.agent {
                    session.switch_agent(
                        crate::agent_def::canonical_name(agent),
                        crate::session::USER_SWITCH_REASON,
                    );
                }
                session
            }
        };
        session.role = request.role;
        // A handoff continuation adds no message: the incoming agent answers
        // the question already in the session, under its own prompt.
        if !continuing {
            session.push(Message::user(prompt).with_attachments(request.attachments.clone()));
            // A new message goes to the orchestrator even when the last turn
            // never reached its end to give the conversation back: the
            // process was killed, or the session predates the rule.
            self.give_back(&mut session, events).await?;
        }
        self.store.save(&session)?;

        // Auto-compact: if the previous turn's context usage was near the
        // window, fold older turns into a summary before we spend another
        // round-trip. Cheap heuristic: peek at the last stored Usage-shaped
        // hint via the message count and trigger conservatively.
        if self.config.agent.auto_compact
            && self.config.agent.auto_compact_at > 0.0
            && session.turns.len() > self.config.agent.compact_keep_last + 2
        {
            let window = self.config.model.context_window.max(1) as f32;
            let approx_tokens = estimate_session_tokens(&session);
            let ratio = approx_tokens / window;
            if ratio >= self.config.agent.auto_compact_at {
                let _ = events
                    .send(Event::Notice {
                        message: format!(
                            "auto-compacting session ({:.0}% of context used)…",
                            ratio * 100.0
                        ),
                    })
                    .await;
                let provider_pre = Provider::from_config(&self.config)?;
                let keep = self.config.agent.compact_keep_last.max(1);
                let summary = crate::compact::compact_with(
                    &mut session,
                    &provider_pre,
                    keep,
                    self.ranking_judge(),
                )
                .await?;
                if summary.is_some() {
                    self.store.save(&session)?;
                    let _ = events
                        .send(Event::Notice {
                            message: "compact done; continuing".into(),
                        })
                        .await;
                }
            }
        }
        let _ = events
            .send(Event::Session {
                id: session.id.clone(),
                title: session.title.clone(),
            })
            .await;

        // Resolve the agent before the provider: which model to build depends
        // on which agent is working, and on the tier that agent declares.
        let active = self.active_agent(&session);
        let model = self.config.model_for(&active.name, active.tier);
        let mut agent_config = self.config.clone();
        if !model.is_empty() && model != agent_config.model.active {
            if let Some(notice) = agent_model_unusable(&agent_config, &model) {
                let _ = events.send(Event::Notice { message: notice }).await;
            } else {
                agent_config.use_model(&model);
            }
        }
        let mut provider = Provider::from_config(&agent_config)?;
        let mut provider_model = agent_config.model.active.clone();
        let mut ladder =
            crate::provider::ModelLadder::new(&active.name, active.tier, &provider_model);
        // Servers still starting after a short wait are left out of this
        // step rather than holding the first token back.
        self.start_mcp();
        self.wait_for_mcp(MCP_GRACE).await;
        let tools_registry = self.tools.read().await;
        // `skill_read` only for an agent with a skill to read: a built-in one
        // it carries, or one found on disk.
        let readable = self
            .discovery
            .skills_for(
                &self.config.ui.disabled_skills,
                &self.discovery.carried_by(&active),
            )
            .next()
            .is_some();
        // Tools the user turned off are neither offered nor run.
        let allowed_tools: Vec<String> = active
            .tools
            .iter()
            .filter(|tool| self.config.agent.preview || tool.as_str() != "preview")
            .cloned()
            .collect();
        let mut schemas = tools_registry.schemas_for_agent(&allowed_tools, readable);
        // Only the agent holding the user's conversation can ask them, and
        // only where the host can put the question to them.
        if self.asks_user && session.parent.is_none() {
            schemas.push(crate::ask::schema());
        }
        if active.delegation != crate::agent_def::Delegation::None {
            // The orchestrator hands the conversation to anyone. A specialist
            // holding the user's conversation may only give it back to the
            // orchestrator, and one working in a delegated branch has no
            // conversation to give: it reports back instead.
            let hand_off = if active.delegation == crate::agent_def::Delegation::Orchestrator {
                crate::routing::HandOff::Anyone
            } else if session.parent.is_none() {
                crate::routing::HandOff::BackToOrchestrator
            } else {
                crate::routing::HandOff::No
            };
            schemas.extend(crate::routing::routing_schemas(hand_off));
        }
        // Set when a routing call is accepted during a step, and acted on once
        // the step's tool results are recorded — switching mid-loop would
        // leave the current turn's results attributed to the wrong agent.
        let mut pending_switch: Option<crate::routing::Switch> = None;
        // Every delegation asked for in one step runs at the same time.
        let mut pending_delegations: Vec<crate::routing::Delegation> = Vec::new();
        // `agent_prompt` already carries the instruction files and the skill
        // list. Appending them again here sent both twice on every model call.
        let mut prompt = self.agent_prompt(&active, &workspace.to_string_lossy());
        // A delegated branch owes its caller a structured report. It rides here,
        // in the system prompt, so it holds the front of the context rather than
        // trailing the task where a long brief buried it. `parent` is what marks
        // a branch: a session the user talks to directly never has one, and an
        // agent handed the conversation answers the user, not a caller.
        if session.parent.is_some() {
            prompt.push_str(REPORT_CONTRACT);
        }
        // An agent taking over sees the previous agent's messages as its own.
        // Continuing them, one described the handoff to the user ("I've passed
        // this to fe") and stopped, with the work undone. So it is told whose
        // turn this is and why.
        if continuing && session.parent.is_none() {
            if let Some(switch) = session.switches.last().filter(|s| s.to == active.name) {
                prompt.push_str(&handoff_note(switch));
            }
        }
        let system = Message::system(prompt);
        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<(String, String)>(64);
        // Forward every tool progress delta to the UI event stream so long
        // running tools (write, bash) reveal output while they work instead
        // of dropping it in one lump at the end.
        let events_forward = events.clone();
        tokio::spawn(async move {
            while let Some((id, delta)) = progress_rx.recv().await {
                let _ = events_forward.send(Event::ToolProgress { id, delta }).await;
            }
        });
        let tool_ctx = ToolCtx {
            skills: self.discovery.carried_by(&active),
            lsp: self.config.agent.lsp.then(|| self.lsp.clone()),
            vision: agent_config.model.vision,
            repair: Some(Arc::new(ModelRepair {
                config: self.config.clone(),
            })),
            workspace,
            shell_timeout: Duration::from_secs(self.config.agent.shell_timeout_secs),
            cancel: cancel.clone(),
            progress: Some(progress_tx),
            call_id: String::new(),
        };

        let mut stop_reason = "stop".to_string();
        // Counts model calls the agent chose to make. A model swapped in by the
        // ladder retries the same step, so it must not consume one — a turn
        // that fell down two tiers would otherwise lose two of its steps to
        // failures it did not cause.
        let mut step = 0u32;
        let limit = self.config.agent.max_steps;
        // The tool calls of the last step, and how many steps in a row made
        // exactly those. With no cap on steps, this is what ends a turn stuck
        // repeating itself.
        let mut last_calls = String::new();
        let mut repeats = 0u32;
        // Interface files this turn wrote or edited, and whether the check
        // the harness runs on them before the agent finishes has run.
        let mut touched: Vec<String> = Vec::new();
        let mut skill_reads = SkillReads::from(&session);
        let mut checked_ui = false;
        // How many times a delegated agent that stopped without its report
        // has been sent back to finish.
        let mut nudged = 0u32;
        while limit == 0 || step < limit {
            if cancel.is_cancelled() {
                stop_reason = "aborted".into();
                break;
            }
            // A long turn fills the window within itself: a sub-agent works
            // in one turn of a hundred steps. Fold older steps before the
            // window fills, or the provider answers with a summary of its own
            // instead of the next step.
            if step > 0 {
                self.compact_mid_turn(&mut session, &agent_config, events)
                    .await;
            }
            let mut wire = Vec::with_capacity(session.turns.len() + 1);
            wire.push(system.clone());
            wire.extend(session.replay());

            let assistant_id = uuid::Uuid::new_v4().to_string();
            let _ = events
                .send(Event::MessageStart {
                    id: assistant_id.clone(),
                })
                .await;
            let (chunk_tx, mut chunk_rx) = mpsc::channel(256);
            let text = Arc::new(Mutex::new(String::new()));
            let reasoning = Arc::new(Mutex::new(String::new()));
            let event_sink = events.clone();
            let text_capture = text.clone();
            let reasoning_capture = reasoning.clone();
            let forward = tokio::spawn(async move {
                while let Some(chunk) = chunk_rx.recv().await {
                    let event = match chunk {
                        Chunk::Text(delta) => {
                            text_capture.lock().expect("text capture").push_str(&delta);
                            Event::Text { delta }
                        }
                        Chunk::Reasoning(delta) => {
                            reasoning_capture
                                .lock()
                                .expect("reasoning capture")
                                .push_str(&delta);
                            Event::Reasoning { delta }
                        }
                    };
                    if event_sink.send(event).await.is_err() {
                        break;
                    }
                }
            });

            // Bind the retry-notice sink to a local so its lifetime spans
            // the entire select! await; passing an inline `Some(Box::new…)`
            // is a temporary that drops before `provider.complete_with_notice`
            // can await it.
            let notice_sink: crate::provider::NoticeSink = {
                let events = events.clone();
                Some(Box::new(move |msg: String, attempt: u32, max: u32| {
                    let events = events.clone();
                    tokio::spawn(async move {
                        let _ = events
                            .send(Event::Retry {
                                message: msg,
                                attempt,
                                max,
                            })
                            .await;
                    });
                })
                    as Box<dyn Fn(String, u32, u32) + Send + Sync>)
            };
            let completion = tokio::select! {
                result = provider.complete_with_notice(&wire, &schemas, &chunk_tx, &notice_sink) => {
                    drop(chunk_tx);
                    let _ = forward.await;
                    match result {
                        Ok(completion) => completion,
                        Err(error) => {
                            // Retries are spent. Another model may still
                            // succeed where this one cannot — a capability
                            // failure never will on a retry, and a capacity
                            // one may have a sibling that is up.
                            match self
                                .step_down(&mut ladder, &mut agent_config, &error, events)
                                .await
                            {
                                Some(next_provider) => {
                                    provider = next_provider;
                                    provider_model = agent_config.model.active.clone();
                                    continue;
                                }
                                None => {
                                    persist_interrupted(&mut session, &self.store, &assistant_id, &provider_model, &text, &reasoning, format!("{error:#}"))?;
                                    return Err(ladder.exhausted(&error));
                                }
                            }
                        }
                    }
                }
                _ = cancel.cancelled() => {
                    // Stop now rather than tidily. `forward` is blocked on a
                    // channel the in-flight request still holds open, so
                    // awaiting it here waits out the rest of the stream —
                    // which is the whole thing the user just asked to stop.
                    // Aborting drops the HTTP body and closes the connection;
                    // whatever arrived before this point was already captured
                    // under the mutex, so the partial reply still persists.
                    forward.abort();
                    drop(chunk_tx);
                    persist_interrupted(&mut session, &self.store, &assistant_id, &provider_model, &text, &reasoning, "Interrupted by user.".into())?;
                    stop_reason = "aborted".into();
                    break;
                }
            };

            // Accumulate into the session as well as reporting it, so the
            // totals survive a restart: the transcript was always restored,
            // but the gauges next to it came back at zero.
            session.usage.input_tokens = session
                .usage
                .input_tokens
                .saturating_add(completion.usage.input_tokens);
            session.usage.output_tokens = session
                .usage
                .output_tokens
                .saturating_add(completion.usage.output_tokens);
            // The context gauge shows the LAST call's prompt, not a running
            // total — it is "how full is the window now".
            session.usage.context_tokens = completion.usage.input_tokens;
            let _ = events
                .send(Event::Usage {
                    input_tokens: session.usage.input_tokens,
                    output_tokens: session.usage.output_tokens,
                    context_tokens: session.usage.context_tokens,
                    context_window: self.config.model.context_window,
                })
                .await;
            session.push(Message {
                role: MessageRole::Assistant,
                // No em dashes, whatever the model wrote.
                content: crate::dashes::strip(&completion.text),
                reasoning: (!completion.reasoning.is_empty()).then_some(completion.reasoning),
                tool_calls: completion.tool_calls.clone(),
                attachments: Vec::new(),
                tool_call_id: None,
                interrupted: false,
                error: None,
                model: Some(provider_model.clone()),
                message_id: Some(assistant_id),
            });
            self.store.save(&session)?;
            if completion.tool_calls.is_empty() {
                // An interface agent about to finish is checked once for the
                // marks of generated work in what it changed. Told to run the
                // check itself, a model skipped it or ran it and reported
                // anyway; the findings now come back to it before it ends.
                if !checked_ui && active.tools.iter().any(|t| t == "ui_check") {
                    checked_ui = true;
                    let files: Vec<std::path::PathBuf> = touched
                        .iter()
                        .filter(|path| crate::ui_check::is_interface_file(path))
                        .map(|path| tool_ctx.workspace.join(path))
                        .collect();
                    let findings = crate::ui_check::check(&tool_ctx.workspace, &files);
                    let serious = findings
                        .iter()
                        .filter(|f| f.severity != crate::ui_check::Severity::Low)
                        .count();
                    if serious > 0 {
                        let report = crate::ui_check::report(&findings, files.len());
                        let _ = events
                            .send(Event::Notice {
                                message: format!("ui_check on the files changed:\n{report}"),
                            })
                            .await;
                        session.push(Message::user(crate::ui_check::gate_message(&report)));
                        self.store.save(&session)?;
                        step += 1;
                        continue;
                    }
                }
                // A delegated agent owes its caller a report. One that stopped
                // without it (a summary of its context, a sentence mid-task)
                // is sent back to finish, twice at most, rather than ending
                // with nothing the caller can use.
                if session.parent.is_some()
                    && nudged < 2
                    && extract_report(completion.text.trim()).is_none()
                    && completion.finish_reason == "stop"
                {
                    nudged += 1;
                    session.push(Message::user(
                        "[harness] You stopped without your report, and the work may not be \
                         done. If what you just wrote is a summary of your progress, carry on \
                         from it: do what is left with your tools. When the task is finished \
                         (or cannot be), end with the report: DONE, CHANGED, VERIFIED, NEXT.",
                    ));
                    self.store.save(&session)?;
                    step += 1;
                    continue;
                }
                stop_reason = completion.finish_reason;
                break;
            }

            // Pictures a tool returned, shown to the model after the step's
            // results: providers take images only in user messages, and a
            // user message between results would split them from their calls.
            let mut pictures: Vec<crate::message::Attachment> = Vec::new();
            // Calls that only look run at the same time; the rest keep their
            // order, one after another.
            let can_run = !cancel.is_cancelled()
                && matches!(completion.finish_reason.as_str(), "stop" | "tool_calls");
            let mut ready: std::collections::HashMap<String, ToolOutput> =
                std::collections::HashMap::new();
            let mut announced: std::collections::HashSet<String> = std::collections::HashSet::new();
            for (index, call) in completion.tool_calls.iter().enumerate() {
                let run = parallel_run(&completion.tool_calls[index..]);
                if can_run && run > 1 && !announced.contains(&call.id) {
                    let together = &completion.tool_calls[index..index + run];
                    for call in together {
                        announced.insert(call.id.clone());
                        let _ = events
                            .send(Event::ToolCall {
                                id: call.id.clone(),
                                name: call.name.clone(),
                                arguments: call.arguments.clone(),
                            })
                            .await;
                    }
                    let outputs = futures::future::join_all(together.iter().map(|call| {
                        let mut ctx = tool_ctx.clone();
                        ctx.call_id = call.id.clone();
                        let args = serde_json::from_str::<serde_json::Value>(&call.arguments)
                            .unwrap_or_default();
                        let registry = &tools_registry;
                        let allowed = &allowed_tools;
                        async move {
                            registry
                                .execute_for_agent(allowed, &ctx, &call.name, args)
                                .await
                        }
                    }))
                    .await;
                    for (call, output) in together.iter().zip(outputs) {
                        ready.insert(call.id.clone(), output);
                    }
                }
                if !announced.contains(&call.id) {
                    let _ = events
                        .send(Event::ToolCall {
                            id: call.id.clone(),
                            name: call.name.clone(),
                            arguments: call.arguments.clone(),
                        })
                        .await;
                }
                let mut output = if let Some(output) = ready.remove(&call.id) {
                    output
                } else if cancel.is_cancelled() {
                    ToolOutput::error("Cancelled before execution.")
                } else if !matches!(completion.finish_reason.as_str(), "stop" | "tool_calls") {
                    ToolOutput::error(format!(
                        "Not executed: provider stopped with {} (possibly truncated arguments).",
                        completion.finish_reason
                    ))
                } else {
                    match serde_json::from_str::<serde_json::Value>(&call.arguments) {
                        Ok(args @ serde_json::Value::Object(_))
                            if call.name == crate::ask::TOOL =>
                        {
                            self.ask_user(
                                &call.id,
                                &active.name,
                                &args,
                                session.parent.is_none(),
                                events,
                                &cancel,
                            )
                            .await
                        }
                        Ok(args @ serde_json::Value::Object(_))
                            if crate::routing::is_routing_tool(&call.name) =>
                        {
                            // Routing calls are intercepted rather than
                            // dispatched: one changes which agent holds the
                            // session, the other runs a whole nested turn, and
                            // neither is reachable from a `ToolCtx`.
                            match crate::routing::parse_switch(&call.name, &args) {
                                Some(crate::routing::Switch::Handoff { .. })
                                    if session.parent.is_some() =>
                                {
                                    ToolOutput::error(
                                        "a delegated agent cannot hand off: finish the task \
                                         and end with your report",
                                    )
                                }
                                Some(switch) => {
                                    match crate::routing::authorise(
                                        &active,
                                        &switch,
                                        &self.discovery.agents,
                                    ) {
                                        Ok(()) => {
                                            let accepted = accepted_message(&switch);
                                            match switch {
                                                crate::routing::Switch::Delegate(delegation) => {
                                                    pending_delegations.push(delegation)
                                                }
                                                handoff => pending_switch = Some(handoff),
                                            }
                                            ToolOutput::ok(accepted)
                                        }
                                        Err(refusal) => ToolOutput::error(refusal.message()),
                                    }
                                }
                                None => ToolOutput::error("malformed routing call"),
                            }
                        }
                        Ok(mut args @ serde_json::Value::Object(_)) => {
                            // What an agent writes into a file has no em
                            // dashes either.
                            crate::dashes::strip_written(&call.name, &mut args);
                            // The contract between agents at work together:
                            // a file another agent is editing stays closed.
                            let claimed = match args["path"].as_str() {
                                Some(path)
                                    if matches!(
                                        call.name.as_str(),
                                        "write" | "edit" | "multi_edit" | "edit_lines"
                                    ) =>
                                {
                                    self.board
                                        .claim(
                                            path,
                                            crate::contract::Claim {
                                                session: session.id.clone(),
                                                agent: active.name.clone(),
                                            },
                                        )
                                        .map_err(|holder| crate::contract::refusal(path, &holder))
                                }
                                _ => Ok(()),
                            };
                            // Reading instead of working: the same skill
                            // again, or skill after skill with no change.
                            let claimed = claimed.and_then(|()| {
                                if call.name == "skill_read" {
                                    skill_reads.check(args["name"].as_str().unwrap_or(""))
                                } else {
                                    Ok(())
                                }
                            });
                            match claimed {
                                Err(refusal) => ToolOutput::error(refusal),
                                Ok(()) => {
                                    let mut ctx = tool_ctx.clone();
                                    ctx.call_id = call.id.clone();
                                    tools_registry
                                        .execute_for_agent(&allowed_tools, &ctx, &call.name, args)
                                        .await
                                }
                            }
                        }
                        Ok(_) => ToolOutput::error("tool arguments must be a JSON object"),
                        Err(error) => {
                            ToolOutput::error(format!("tool arguments are not valid JSON: {error}"))
                        }
                    }
                };
                let _ = events
                    .send(Event::ToolResult {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        content: output.content.clone(),
                        is_error: output.is_error,
                        before: output.before.clone(),
                    })
                    .await;
                // The transcript already has the full result; what is decided
                // here is only what later turns are made to re-read. Tool
                // results are 93% of a session's context, and the three tools
                // that dominate — bash, read, skill_read — cannot be judged
                // from their names.
                if !output.is_error
                    && matches!(
                        call.name.as_str(),
                        "write" | "edit" | "multi_edit" | "edit_lines" | "plan_write"
                    )
                {
                    skill_reads.changed();
                }
                if !output.is_error
                    && matches!(
                        call.name.as_str(),
                        "write" | "edit" | "multi_edit" | "edit_lines"
                    )
                {
                    if let Some(path) = serde_json::from_str::<serde_json::Value>(&call.arguments)
                        .ok()
                        .and_then(|args| {
                            args.get("path").and_then(|p| p.as_str()).map(str::to_owned)
                        })
                    {
                        if !touched.contains(&path) {
                            touched.push(path);
                        }
                    }
                }
                // A picture already in view is not sent again: every copy
                // stays in the context of every later call.
                if !output.images.is_empty() {
                    let in_view = |image: &crate::message::Attachment| {
                        pictures.iter().any(|p| p.data_url == image.data_url)
                            || session.turns.iter().any(|turn| {
                                turn.message
                                    .attachments
                                    .iter()
                                    .any(|a| a.data_url == image.data_url)
                            })
                    };
                    let before = output.images.len();
                    let images = std::mem::take(&mut output.images);
                    output.images = images.into_iter().filter(|i| !in_view(i)).collect();
                    if output.images.len() < before {
                        output.content.push_str(
                            "\n\nThis picture is already in the conversation above, unchanged: \
                             look at it there instead of reading it again.",
                        );
                    }
                }
                let stored = if self.config.typesafe.gate_tool_results
                    && matches!(
                        crate::gating::judge(
                            self.system_one.as_ref(),
                            &call.name,
                            &call.arguments,
                            &output.content,
                            output.is_error,
                        )
                        .await,
                        crate::gating::Keep::Trimmed
                    ) {
                    let trimmed = crate::gating::trim(&output.content);
                    let _ = events
                        .send(Event::Trimmed {
                            tool: call.name.clone(),
                            was: output.content.len(),
                            now: trimmed.len(),
                        })
                        .await;
                    trimmed
                } else {
                    output.content.clone()
                };
                let mut result = Message::tool_result(&call.id, stored);
                if output.is_error {
                    result.error = Some("Tool execution failed".into());
                }
                pictures.extend(output.images.iter().cloned());
                session.push(result);
                self.store.save(&session)?;
            }
            if !pictures.is_empty() {
                let names: Vec<&str> = pictures.iter().map(|p| p.name.as_str()).collect();
                session.push(
                    Message::user(format!(
                        "[harness] The image `read` returned: {}",
                        names.join(", ")
                    ))
                    .with_attachments(pictures),
                );
            }
            self.store.save(&session)?;
            if !matches!(completion.finish_reason.as_str(), "stop" | "tool_calls") {
                stop_reason = completion.finish_reason;
                break;
            }
            if cancel.is_cancelled() {
                stop_reason = "aborted".into();
                break;
            }
            // A routing call accepted during this step takes effect now that
            // the step's results are recorded.
            if let Some(switch) = pending_switch.take() {
                match switch {
                    crate::routing::Switch::Handoff { to, reason } => {
                        session.switch_agent(&to, &reason);
                        self.store.save(&session)?;
                        let _ = events
                            .send(Event::AgentSwitched {
                                to: to.clone(),
                                reason,
                            })
                            .await;
                        // The handover ends this turn: the incoming agent needs
                        // its own system prompt, and that is set when the next
                        // turn assembles. Continuing here would run the new
                        // agent under the old one's instructions.
                        stop_reason = "handoff".into();
                        break;
                    }
                    crate::routing::Switch::Delegate(delegation) => {
                        pending_delegations.push(delegation);
                    }
                }
            }
            if !pending_delegations.is_empty() {
                let delegations = std::mem::take(&mut pending_delegations);
                // The orchestrator holding the user's conversation does not
                // wait on its specialists: they run on after this turn, and
                // their reports start a new one when they are all done.
                if self.config.agent.background_delegation
                    && session.parent.is_none()
                    && self.can_delegate_in_background()
                {
                    let started = self
                        .start_branches(&mut session, &delegations, events)
                        .await;
                    self.continue_in_background(session.id.clone(), delegations.clone(), started);
                    let names: Vec<String> =
                        delegations.iter().map(|d| format!("`{}`", d.to)).collect();
                    session.push(Message {
                        role: MessageRole::User,
                        content: format!(
                            "[harness] Started in the background: {}. They report when they \
                             finish, all together, in a new turn. End this turn now with one \
                             line to the user saying who is working on what, unless there is \
                             work that does not depend on them. Do not wait, poll or check \
                             their files.",
                            names.join(", ")
                        ),
                        reasoning: None,
                        tool_calls: Vec::new(),
                        attachments: Vec::new(),
                        tool_call_id: None,
                        interrupted: false,
                        error: None,
                        model: None,
                        message_id: None,
                    });
                    self.store.save(&session)?;
                    step += 1;
                    continue;
                }
                let reports = self
                    .run_delegations(&mut session, &delegations, events)
                    .await;
                for report in reports {
                    session.push(Message {
                        role: MessageRole::User,
                        content: crate::routing::report_message_for(
                            &report.agent,
                            &report.session_id,
                            &report.summary,
                        ),
                        reasoning: None,
                        tool_calls: Vec::new(),
                        attachments: Vec::new(),
                        tool_call_id: None,
                        interrupted: false,
                        error: None,
                        model: None,
                        message_id: None,
                    });
                }
                self.store.save(&session)?;
            }
            let calls: String = completion
                .tool_calls
                .iter()
                .map(|call| format!("{}:{}\n", call.name, call.arguments))
                .collect();
            if calls == last_calls {
                repeats += 1;
            } else {
                last_calls = calls;
                repeats = 1;
            }
            if repeats >= REPEAT_LIMIT {
                stop_reason = "repeating".into();
                let _ = events
                    .send(Event::Notice {
                        message: format!(
                            "Stopped: the agent made the same call {REPEAT_LIMIT} times in a row \
                             without getting anywhere."
                        ),
                    })
                    .await;
                break;
            }
            step += 1;
            if step == limit {
                stop_reason = "step_limit".into();
                let _ = events
                    .send(Event::Notice {
                        message: format!(
                            "Stopped after {} model calls. The turn is incomplete.",
                            self.config.agent.max_steps
                        ),
                    })
                    .await;
            }
        }

        self.store.save(&session)?;
        // A handoff is not the end of the work — the incoming agent has not
        // answered yet. Say so rather than sending `Done`, which would leave
        // the user looking at a finished-looking turn where nothing happened.
        if stop_reason == "handoff" {
            return Ok(Some(session.id.clone()));
        }
        // Sent before `Done`, which is what the interface treats as the end
        // of the reply.
        self.give_back(&mut session, events).await?;
        let _ = events.send(Event::Done { stop_reason }).await;
        Ok(None)
    }

    /// Give the conversation back to the orchestrator when a specialist holds
    /// it on the orchestrator's behalf.
    ///
    /// A handoff lends the conversation for one reply. Left with the
    /// specialist, the user's next message landed with whichever agent spoke
    /// last, which then answered things outside its domain. The specialist's
    /// answer stands and no model is called: only the holder changes.
    async fn give_back(&self, session: &mut Session, events: &mpsc::Sender<Event>) -> Result<()> {
        if !session.lent_by_orchestrator() {
            return Ok(());
        }
        session.switch_agent(
            crate::agent_def::ORCHESTRATOR,
            crate::session::RETURN_REASON,
        );
        self.store.save(session)?;
        let _ = events
            .send(Event::AgentSwitched {
                to: crate::agent_def::ORCHESTRATOR.into(),
                reason: crate::session::RETURN_REASON.into(),
            })
            .await;
        Ok(())
    }

    /// [`Self::give_back`] for a turn that did not reach its end. Best effort:
    /// the turn has already failed or been stopped, and a new message gives
    /// the conversation back anyway.
    async fn give_back_saved(&self, session_id: &str, events: &mpsc::Sender<Event>) {
        if let Ok(mut session) = self.store.load(session_id) {
            let _ = self.give_back(&mut session, events).await;
        }
    }
}

/// The result of an accepted routing call. A handoff's is the last thing the
/// incoming agent reads before its first step, and the agent that made the
/// call never reads it (its turn ends there), so it speaks to the incoming
/// agent. With only the system prompt saying so, a model taking over after
/// a long brainstorm went on as the orchestrator: "the design is agreed and
/// passed to the frontend specialist", and nothing was built.
fn accepted_message(switch: &crate::routing::Switch) -> String {
    match switch {
        crate::routing::Switch::Handoff { to, .. } => format!(
            "Handed to `{to}`, which holds the conversation from here. `{to}`: the user's \
             last request is yours to carry out now, with your own tools, then answer \
             the user. Do not repeat, describe or confirm the handoff."
        ),
        crate::routing::Switch::Delegate(delegation) => {
            format!("Delegated to `{}`; its report follows.", delegation.to)
        }
    }
}

/// Steps in a row making exactly the same tool calls before a turn is taken
/// to be stuck and stopped.
const REPEAT_LIMIT: u32 = 3;

/// What an agent that has just been handed the conversation is told.
fn handoff_note(switch: &crate::session::AgentSwitch) -> String {
    format!(
        "\n\nHANDED TO YOU\n`{from}` handed this conversation to you: {reason}\n\
         The user's last request is yours to handle now: do the work with your \
         tools, then answer the user. The messages above that routed it were \
         `{from}`'s; do not repeat, describe or confirm the handoff.",
        from = switch.from,
        reason = switch.reason.trim(),
    )
}

/// Tools that only look, and so can run beside each other in one step.
/// `bash` may change files, so at most one joins a run, beside reads.
const LOOKING_TOOLS: &[&str] = &["read", "glob", "grep", "fetch", "diagnostics", "ui_check"];

/// How many calls from the start of `calls` run together: a stretch of
/// looking calls with well-formed arguments, and at most one `bash`.
fn parallel_run(calls: &[crate::message::ToolCall]) -> usize {
    let mut bash = 0;
    calls
        .iter()
        .take_while(|call| {
            let fits = match call.name.as_str() {
                "bash" => {
                    bash += 1;
                    bash == 1
                }
                name => LOOKING_TOOLS.contains(&name),
            };
            fits && serde_json::from_str::<serde_json::Value>(&call.arguments)
                .is_ok_and(|args| args.is_object())
        })
        .count()
}

fn persist_interrupted(
    session: &mut Session,
    store: &SessionStore,
    message_id: &str,
    model: &str,
    text: &Mutex<String>,
    reasoning: &Mutex<String>,
    error: String,
) -> Result<()> {
    let content = crate::dashes::strip(&text.lock().expect("text capture"));
    let reasoning = reasoning.lock().expect("reasoning capture").clone();
    session.push(Message {
        role: MessageRole::Assistant,
        content,
        reasoning: (!reasoning.is_empty()).then_some(reasoning),
        tool_calls: Vec::new(),
        attachments: Vec::new(),
        tool_call_id: None,
        interrupted: true,
        error: Some(error),
        model: Some(model.to_string()),
        message_id: Some(message_id.to_string()),
    });
    store.save(session)
}

/// What a finished branch reports back: the specialist's own last word.
///
/// The specialist has the whole context, so its closing message is the
/// summary — no extra model call is needed to produce one.
/// What the caller is told a delegation did.
///
/// The sub-agent is asked for a structured report, and usually gives one.
/// When it does not — it ran out of steps, or stopped mid-sentence — its last
/// message is narration rather than a result ("Now rewriting main.js"), and
/// passing that up tells the caller nothing true. So a report is assembled
/// from what the branch actually did instead.
fn branch_summary(branch: &Session) -> String {
    let last = branch
        .turns
        .iter()
        .rev()
        .find(|t| {
            matches!(t.message.role, MessageRole::Assistant) && !t.message.content.trim().is_empty()
        })
        .map(|t| t.message.content.trim().to_owned());

    if let Some(report) = last.as_deref().and_then(extract_report) {
        return report.to_owned();
    }

    // No report. Say so plainly and give the facts, rather than passing off a
    // mid-task sentence as a result.
    let touched = files_touched(branch);
    let mut out = String::from("NO REPORT — the sub-agent stopped without one.");
    out.push_str(&format!(
        "\nCHANGED: {}",
        if touched.is_empty() {
            "none".to_owned()
        } else {
            touched.join(", ")
        }
    ));
    if let Some(said) = last {
        // Its last words are still evidence of where it got to, as long as
        // they are not presented as a conclusion.
        let said: String = said.chars().take(400).collect();
        out.push_str(&format!("\nLAST SAID: {said}"));
    }
    out
}

/// The report block from a sub-agent's final message, if it wrote one.
///
/// Taken from `DONE:` onwards so any reasoning before it is dropped — the
/// caller asked for a result, not a transcript.
fn extract_report(text: &str) -> Option<&str> {
    let start = text.find("DONE:")?;
    let report = text[start..].trim();
    // A bare "DONE:" with nothing under it is not a report.
    report.len().gt(&6).then_some(report)
}

/// Files a branch changed, read from its tool calls.
///
/// Taken from the calls rather than the prose so the list is what actually
/// happened, not what the model said happened.
fn files_touched(branch: &Session) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for turn in &branch.turns {
        for call in &turn.message.tool_calls {
            if !matches!(call.name.as_str(), "write" | "edit") {
                continue;
            }
            let Ok(args) = serde_json::from_str::<serde_json::Value>(&call.arguments) else {
                continue;
            };
            if let Some(path) = args.get("path").and_then(serde_json::Value::as_str) {
                if !out.iter().any(|p| p == path) {
                    out.push(path.to_owned());
                }
            }
        }
    }
    out
}

/// Why an agent cannot run on `model`, its own or its tier's: a provider
/// enx does not know, or one with no key. The agent then runs on the model
/// in use, and the user is told, rather than every call failing on a
/// missing key.
fn agent_model_unusable(config: &Config, model: &str) -> Option<String> {
    let Some(parsed) = config.parse_model(model) else {
        return Some(format!(
            "`{model}` names no provider enx knows; staying on {}",
            config.model.active
        ));
    };
    let connection = config.connection(&parsed.provider)?;
    (!connection.is_connected()).then(|| {
        format!(
            "{} is not connected, so {parsed} cannot run; staying on {}. Connect it in /provider.",
            connection.name, config.model.active
        )
    })
}

#[cfg(test)]
mod report_tests {
    use super::*;
    use crate::message::ToolCall;

    fn branch_with(messages: &[Message]) -> Session {
        let mut branch = Session::new(crate::Role::Orchestrator);
        for message in messages {
            branch.push(message.clone());
        }
        branch
    }

    fn wrote(path: &str) -> Message {
        let mut message = Message::assistant("");
        message.tool_calls = vec![ToolCall {
            id: "c1".into(),
            name: "write".into(),
            arguments: format!(r#"{{"path":"{path}"}}"#),
        }];
        message
    }

    #[test]
    fn a_report_is_passed_up() {
        let branch = branch_with(&[Message::assistant(
            "Some reasoning first.\n\nDONE: built the page\nCHANGED: index.html\n\
             VERIFIED: opened it, renders\nNEXT: nothing",
        )]);
        let summary = branch_summary(&branch);
        assert!(
            summary.starts_with("DONE:"),
            "reasoning is dropped: {summary}"
        );
        assert!(summary.contains("index.html"));
        assert!(
            !summary.contains("Some reasoning first"),
            "the caller asked for a result, not a transcript: {summary}"
        );
    }

    /// The bug this exists for. A sub-agent that stops mid-task leaves a line
    /// of narration, and passing it up as a summary tells the caller
    /// something that is not true: "Now rewriting main.js" reads as a result
    /// while nothing was finished.
    #[test]
    fn narration_is_not_passed_off_as_a_result() {
        let branch = branch_with(&[
            wrote("js/main.js"),
            Message::assistant("Now rewriting `js/main.js` with the corrected logic."),
        ]);
        let summary = branch_summary(&branch);
        assert!(
            summary.starts_with("NO REPORT"),
            "the caller must be told there was no report: {summary}"
        );
        assert!(
            summary.contains("js/main.js"),
            "and given what actually changed: {summary}"
        );
        assert!(
            summary.contains("LAST SAID"),
            "with its last words marked as such, not as a conclusion: {summary}"
        );
    }

    /// A sub-agent asking a question is the worst case: the caller cannot
    /// answer, and would otherwise pass it to the user as if it were work.
    #[test]
    fn a_question_is_not_a_report() {
        let branch = branch_with(&[Message::assistant(
            "Before I start I need 2 answers:\n1. Which mode?\n2. Which direction?",
        )]);
        let summary = branch_summary(&branch);
        assert!(summary.starts_with("NO REPORT"), "got: {summary}");
    }

    #[test]
    fn files_are_reported_when_there_is_no_report() {
        let branch = branch_with(&[wrote("a.html"), wrote("b.css")]);
        let summary = branch_summary(&branch);
        assert!(summary.contains("a.html"), "{summary}");
        assert!(summary.contains("b.css"), "{summary}");
    }

    #[test]
    fn changing_nothing_says_so() {
        let branch = branch_with(&[Message::assistant("I had a look around.")]);
        let summary = branch_summary(&branch);
        assert!(summary.contains("CHANGED: none"), "{summary}");
    }

    /// An empty branch still has to produce something the caller can read.
    #[test]
    fn an_empty_branch_still_reports() {
        let branch = branch_with(&[]);
        let summary = branch_summary(&branch);
        assert!(summary.starts_with("NO REPORT"), "{summary}");
        assert!(summary.contains("CHANGED: none"), "{summary}");
    }

    /// A bare marker with nothing under it is not a report.
    #[test]
    fn an_empty_report_block_does_not_count() {
        let branch = branch_with(&[Message::assistant("DONE:")]);
        let summary = branch_summary(&branch);
        assert!(summary.starts_with("NO REPORT"), "{summary}");
    }

    /// The last words are evidence, not a conclusion, so they are capped —
    /// a sub-agent that ended mid-essay must not paste it into the caller's
    /// context.
    #[test]
    fn the_last_words_are_capped() {
        let branch = branch_with(&[Message::assistant("x".repeat(5_000))]);
        let summary = branch_summary(&branch);
        assert!(
            summary.len() < 600,
            "a failed delegation should not cost 5k characters: {}",
            summary.len()
        );
    }
}

#[cfg(test)]
mod overview_tests {
    use super::workspace_overview;

    #[test]
    fn lists_the_top_level_and_leaves_out_the_noise() {
        let dir = std::env::temp_dir().join(format!("enx-overview-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("node_modules/x")).unwrap();
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(dir.join("index.html"), "").unwrap();
        let overview = workspace_overview(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(overview.ends_with("index.html  src/"), "{overview}");
        assert!(!overview.contains("node_modules") && !overview.contains(".git"));
    }

    #[test]
    fn a_crowded_workspace_is_cut_short_and_says_so() {
        let dir = std::env::temp_dir().join(format!("enx-overview-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        for n in 0..45 {
            std::fs::write(dir.join(format!("file{n:02}.txt")), "").unwrap();
        }
        let overview = workspace_overview(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(overview.ends_with("and 5 more"), "{overview}");
    }
}

/// Mends an edit that broke a file's syntax with a quick model call: the
/// cheap tier's model when one is set, else the model in use.
struct ModelRepair {
    config: Config,
}

const REPAIR_PROMPT: &str = "You mend code an automated edit broke. You are shown a region of a \
file as it was (it parsed) and as the edit left it (it does not parse). Answer with the region as \
it should be: the edit's intended change kept, the syntax made whole (brackets balanced, \
statements complete, duplicated or cut lines fixed). Change nothing else. Keep every line of \
context the region starts and ends with exactly as it is, indentation included. Answer with the \
code alone: no explanation, no code fence.";

#[async_trait::async_trait]
impl crate::syntax::Repair for ModelRepair {
    async fn repair(
        &self,
        language: &str,
        before: &str,
        after: &str,
        previous: Option<&str>,
    ) -> Option<String> {
        let mut config = self.config.clone();
        let cheap = config.agent.tiers.cheap.trim().to_owned();
        if !cheap.is_empty() {
            config.use_model(&cheap);
        }
        config.model.effort.clear();
        let provider = Provider::from_config(&config).ok()?;
        let mut ask = format!(
            "Language: {language}\n\nBEFORE (parsed):\n{before}\n\nAFTER (does not parse):\n{after}\n"
        );
        if let Some(previous) = previous {
            ask.push_str(&format!(
                "\nAn earlier answer still did not parse; do better:\n{previous}\n"
            ));
        }
        let messages = vec![Message::system(REPAIR_PROMPT), Message::user(ask)];
        let (tx, mut rx) = mpsc::channel(64);
        tokio::spawn(async move { while rx.recv().await.is_some() {} });
        let answer = tokio::time::timeout(
            std::time::Duration::from_secs(60),
            provider.complete(&messages, &[], &tx),
        )
        .await
        .ok()?
        .ok()?;
        let text = answer.text.trim_matches('\n').to_owned();
        (!text.trim().is_empty()).then_some(text)
    }
}

/// How many skills an agent may read, one after another, before it has to
/// change something.
const SKILLS_BEFORE_WORK: usize = 6;

/// Keeps an agent from reading skills instead of working. A weaker model
/// told to read "the skills that govern this page" read ninety of them,
/// several five times, and wrote nothing. Here a skill it already has in
/// view is not sent again, and after `SKILLS_BEFORE_WORK` reads with no file
/// changed, it is told to build with what it read.
struct SkillReads {
    /// Skills whose whole text is still in the conversation.
    seen: std::collections::HashSet<String>,
    /// Skills read since the last change.
    since_change: usize,
}

impl SkillReads {
    fn from(session: &Session) -> Self {
        let mut seen = std::collections::HashSet::new();
        let mut since_change = 0;
        let mut names: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for turn in &session.turns {
            let message = &turn.message;
            for call in &message.tool_calls {
                match call.name.as_str() {
                    "skill_read" => {
                        if let Some(name) =
                            serde_json::from_str::<serde_json::Value>(&call.arguments)
                                .ok()
                                .and_then(|a| a["name"].as_str().map(str::to_owned))
                        {
                            names.insert(call.id.clone(), name);
                        }
                    }
                    "write" | "edit" | "multi_edit" | "edit_lines" | "plan_write" => {
                        since_change = 0
                    }
                    _ => {}
                }
            }
            // A result kept whole: the skill is in view. One trimmed or
            // failed is not, and may be read again.
            if let Some(name) = message.tool_call_id.as_ref().and_then(|id| names.get(id)) {
                if message.error.is_none()
                    && !message
                        .content
                        .contains("characters dropped from the middle")
                {
                    seen.insert(name.clone());
                    since_change += 1;
                }
            }
        }
        Self { seen, since_change }
    }

    fn check(&mut self, name: &str) -> Result<(), String> {
        let name = name.trim().to_ascii_lowercase();
        if self.seen.contains(&name) {
            return Err(format!(
                "You read `{name}` earlier in this task and its text is above: work from it instead \
                 of reading it again."
            ));
        }
        if self.since_change >= SKILLS_BEFORE_WORK {
            return Err(format!(
                "You have read {} skills without changing a file. Build now with what they say. \
                 Read another skill only when you reach work none of them covers, and never the \
                 whole family: the root and the parts you are about to build are enough.",
                self.since_change
            ));
        }
        self.seen.insert(name);
        self.since_change += 1;
        Ok(())
    }

    fn changed(&mut self) {
        self.since_change = 0;
    }
}

#[cfg(test)]
mod skill_read_tests {
    use super::*;

    fn read(session: &mut Session, id: &str, name: &str, result: &str) {
        let mut call = Message::assistant("");
        call.tool_calls.push(crate::message::ToolCall {
            id: id.into(),
            name: "skill_read".into(),
            arguments: serde_json::json!({ "name": name }).to_string(),
        });
        session.push(call);
        session.push(Message::tool_result(id, result));
    }

    #[test]
    fn a_skill_in_view_is_not_sent_again_and_reading_stops_before_work() {
        let mut session = Session::new(Role::Orchestrator);
        read(&mut session, "c1", "ui", "# ui\nthe whole text");
        read(
            &mut session,
            "c2",
            "ui-layout",
            "x … [900 characters dropped from the middle] … y",
        );
        let mut reads = SkillReads::from(&session);
        assert!(reads.check("ui").is_err(), "already in view");
        assert!(
            reads.check("ui-layout").is_ok(),
            "trimmed, so it may be read again"
        );
        for name in ["a", "b", "c", "d"] {
            assert!(reads.check(name).is_ok(), "{name}");
        }
        let stop = reads.check("e").unwrap_err();
        assert!(stop.contains("without changing a file"), "{stop}");
        reads.changed();
        assert!(reads.check("e").is_ok(), "a change opens reading again");
    }
}

#[cfg(test)]
mod parallel_run_tests {
    use super::parallel_run;
    use crate::message::ToolCall;

    fn call(name: &str) -> ToolCall {
        ToolCall {
            id: name.into(),
            name: name.into(),
            arguments: "{}".into(),
        }
    }

    #[test]
    fn looking_calls_and_one_bash_run_together() {
        let step = |names: &[&str]| names.iter().map(|n| call(n)).collect::<Vec<_>>();
        assert_eq!(
            parallel_run(&step(&["read", "grep", "bash", "glob", "write"])),
            4
        );
        assert_eq!(
            parallel_run(&step(&["bash", "read", "bash"])),
            2,
            "one bash per run"
        );
        assert_eq!(parallel_run(&step(&["write", "read"])), 0);
        let mut broken = call("read");
        broken.arguments = "{\"path\":".into();
        assert_eq!(parallel_run(&[call("read"), broken]), 1);
    }
}
