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
     NEXT: what the caller must know to carry on, or `nothing`\n";

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
- When you need several files or searches, request them together in one step, not one per turn.\n\
- Create a file whole with one `write`. Change an existing file with `edit`; do not rewrite a file to change a detail of it.\n\
- Verify with what the project already has (its build, tests, linter) or by reading the result. Do not write throwaway scripts (a python heredoc, an ad-hoc validator) to check your own output.\n\
- Leave nothing running: no servers or background processes (`&`, nohup) that outlive the command that started them.\n\
- `todo` is for work of four or more steps. Set the list once and mark finished steps together; skip it for small tasks.\n\
- Read a skill only when the task needs its instructions, and only that skill.\n\
- When a detail is open and a sensible default exists, choose it and say what you chose, rather than stopping to ask.\n\
- Stop when the request is met. Do not add files, features or polish nobody asked for.\n";

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
             - Answer in the user's language. Be concrete: exact paths, symbols, and commands.\n\
             \n\
             {EFFORT_RULES}\n\
             You are `{}`.\n{}\n",
            agent.name, agent.prompt
        );
        if agent.delegation != crate::agent_def::Delegation::None {
            prompt.push_str(&self.roster_block(agent));
        }
        if let Some(extra) = self
            .discovery
            .system_prompt_with_disabled(&self.config.ui.disabled_skills)
        {
            prompt.push('\n');
            prompt.push_str(&extra);
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
        match self.run_inner(request, &events, cancel).await {
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
        let step = ladder.next(&self.config)?;
        let drop = crate::provider::tier_drop(previous, &step, error);
        let _ = events
            .send(Event::Notice {
                message: drop.message(),
            })
            .await;
        agent_config.model.default = step.model.clone();
        Provider::from_config(agent_config).ok()
    }

    async fn run_delegated(
        &self,
        parent: &mut Session,
        delegation: &crate::routing::Delegation,
        events: &mpsc::Sender<Event>,
    ) -> String {
        let mut branch = parent.branch(&delegation.to);
        if let Err(error) = self.store.save(&branch) {
            // Said on screen as well as to the caller: the delegate call
            // itself went through, so without this the failure shows nowhere.
            let summary = format!("failed before changing anything: could not start: {error:#}");
            let _ = events
                .send(Event::DelegationFinished {
                    agent: delegation.to.clone(),
                    summary: summary.clone(),
                    session_id: branch.id.clone(),
                    failed: true,
                })
                .await;
            return summary;
        }
        // The task alone. The report the sub-agent owes (DONE/CHANGED/…) rides
        // in its system prompt via REPORT_CONTRACT, keyed off the branch's
        // `parent`, rather than trailing the task here — appended to a long
        // brief it was getting lost, and the sub-agent stopped with narration
        // mid-task ("Now rewriting main.js") or a question the caller cannot
        // answer standing in for a result.
        let request = RunRequest {
            prompt: delegation.task.clone(),
            session_id: Some(branch.id.clone()),
            role: parent.role,
            attachments: Vec::new(),
        };
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

        // The sub-agent's own events are not forwarded: the caller sees a
        // summary, and interleaving two agents' tool calls in one transcript
        // is unreadable. The branch session holds the detail, and the id
        // above is how the interface reaches it.
        let (sink, mut drain) = mpsc::channel::<Event>(64);
        tokio::spawn(async move { while drain.recv().await.is_some() {} });

        // Its own token, NOT a child of the caller's.
        //
        // Stopping the turn stops the agent the user is talking to. A
        // sub-agent is not that agent: the user cannot send it anything,
        // cannot steer it, and only the calling agent decides what it does.
        // Cancelling it from the outside left it dead mid-task and the caller
        // holding a `NO REPORT` for work that was interrupted rather than
        // failed — indistinguishable, from the report, from a sub-agent that
        // simply gave up.
        //
        // It still ends: its own step limit, its own errors, and the caller
        // waits for it either way.
        let own = CancellationToken::new();
        let outcome = Box::pin(self.run_inner(request, &sink, own)).await;

        // Reload: the nested run owns the branch on disk from here.
        if let Ok(finished) = self.store.load(&branch.id) {
            branch = finished;
        }
        parent.absorb_usage(&branch.usage);

        let summary = match outcome {
            Ok(()) => branch_summary(&branch),
            Err(error) => {
                // Whether files were already changed decides what the caller
                // may safely do next, so the report says which — a caller that
                // treats a partial failure as "try again" has the next
                // specialist building on a state it knows nothing about.
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
        if let Some(record) = parent
            .delegations
            .iter_mut()
            .rev()
            .find(|record| record.session_id == branch.id)
        {
            record.failed = failed;
        }
        let _ = events
            .send(Event::DelegationFinished {
                agent: delegation.to.clone(),
                summary: summary.clone(),
                session_id: branch.id.clone(),
                failed,
            })
            .await;
        summary
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
            if cancel.is_cancelled() {
                return Ok(());
            }
            next = self
                .run_turn(
                    RunRequest {
                        prompt: String::new(),
                        session_id: Some(session_id),
                        role,
                        attachments: Vec::new(),
                    },
                    events,
                    cancel.clone(),
                )
                .await?;
        }
        // Still handing over after four turns is a loop, not progress.
        if next.is_some() {
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
        anyhow::ensure!(
            self.config.agent.max_steps > 0,
            "agent.max_steps must be greater than zero"
        );
        anyhow::ensure!(
            self.config.is_ready(),
            "Configure a provider with /provider, or set ENX_BASE_URL, ENX_MODEL and ENX_API_KEY."
        );
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
                session
            }
        };
        session.role = request.role;
        // A handoff continuation adds no message: the incoming agent answers
        // the question already in the session, under its own prompt.
        if !continuing {
            session.push(Message::user(prompt).with_attachments(request.attachments.clone()));
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
        if !model.is_empty() {
            agent_config.model.default = model;
        }
        let mut provider = Provider::from_config(&agent_config)?;
        let mut provider_model = agent_config.model.default.clone();
        let mut ladder =
            crate::provider::ModelLadder::new(&active.name, active.tier, &provider_model);
        // Servers still starting after a short wait are left out of this
        // step rather than holding the first token back.
        self.start_mcp();
        self.wait_for_mcp(MCP_GRACE).await;
        let tools_registry = self.tools.read().await;
        let mut schemas = tools_registry.schemas_for_agent(&active.tools, Some(&self.discovery));
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
        while step < self.config.agent.max_steps {
            if cancel.is_cancelled() {
                stop_reason = "aborted".into();
                break;
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
                                    provider_model = agent_config.model.default.clone();
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
                content: completion.text,
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
                stop_reason = completion.finish_reason;
                break;
            }

            for call in &completion.tool_calls {
                let _ = events
                    .send(Event::ToolCall {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        arguments: call.arguments.clone(),
                    })
                    .await;
                let output = if cancel.is_cancelled() {
                    ToolOutput::error("Cancelled before execution.")
                } else if !matches!(completion.finish_reason.as_str(), "stop" | "tool_calls") {
                    ToolOutput::error(format!(
                        "Not executed: provider stopped with {} (possibly truncated arguments).",
                        completion.finish_reason
                    ))
                } else {
                    match serde_json::from_str::<serde_json::Value>(&call.arguments) {
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
                                            pending_switch = Some(switch);
                                            ToolOutput::ok(
                                                "accepted; the session continues with that agent",
                                            )
                                        }
                                        Err(refusal) => ToolOutput::error(refusal.message()),
                                    }
                                }
                                None => ToolOutput::error("malformed routing call"),
                            }
                        }
                        Ok(args @ serde_json::Value::Object(_)) => {
                            let mut ctx = tool_ctx.clone();
                            ctx.call_id = call.id.clone();
                            tools_registry
                                .execute_for_agent(&active.tools, &ctx, &call.name, args)
                                .await
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
                    })
                    .await;
                // The transcript already has the full result; what is decided
                // here is only what later turns are made to re-read. Tool
                // results are 93% of a session's context, and the three tools
                // that dominate — bash, read, skill_read — cannot be judged
                // from their names.
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
                session.push(result);
                self.store.save(&session)?;
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
                        let summary = self.run_delegated(&mut session, &delegation, events).await;
                        session.push(Message {
                            role: MessageRole::User,
                            content: crate::routing::report_message(&delegation.to, &summary),
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
                    }
                }
            }
            step += 1;
            if step == self.config.agent.max_steps {
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
        let _ = events.send(Event::Done { stop_reason }).await;
        Ok(None)
    }
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
    let content = text.lock().expect("text capture").clone();
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
