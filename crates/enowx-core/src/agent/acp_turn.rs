//! A turn run by an ACP engine (Claude Code, Codex, Gemini CLI, a custom
//! agent) instead of the configured model.
//!
//! The engine runs its own loop with its own tools. enowx gives it the
//! agent's prompt (Claude Code takes it as an appended system prompt; the
//! others get it at the top of the first message), its own tools over MCP
//! (skills, delegation to enowx's specialists, asking the user, the MCP
//! servers enowx has), and its permission prompts, answered by the user or by
//! enowx's rules. What the engine streams is shown as enowx's own events, and
//! its reply is kept in the session like any other.

use std::collections::HashMap;

use serde_json::{json, Value};

use super::*;
use crate::acp::manager::{configure, manager, Bound, Incoming};
use crate::acp::{mcp_host, EngineConfig};

/// What an engine is told about running inside enowx.
fn acp_note(agent: &str) -> String {
    format!(
        "\n\n## Running inside enowx\nYou run as enowx's `{agent}` agent, through the Agent \
         Client Protocol. Use your own tools for files and the shell. enowx's own tools (such \
         as `delegate`, `skill_read` and `ask`, where the text above names them) are on the \
         MCP server named `enowx`: call them there. For a tool the text names that you do not \
         have, use your own nearest equivalent."
    )
}

/// The earlier conversation, for an engine session that starts partway
/// through it: the engine restarted, or the session was opened again.
fn history(session: &Session) -> String {
    let turns: Vec<&crate::session::StoredTurn> = session
        .turns
        .iter()
        .filter(|t| {
            matches!(t.message.role, MessageRole::User | MessageRole::Assistant)
                && !t.message.content.trim().is_empty()
                && !t.message.content.starts_with("[harness]")
        })
        .collect();
    // The last turn is the message being sent now.
    let earlier = &turns[..turns.len().saturating_sub(1)];
    if earlier.is_empty() {
        return String::new();
    }
    let start = earlier.len().saturating_sub(20);
    let mut out = String::from("Earlier in this conversation:\n");
    for turn in &earlier[start..] {
        let who = if turn.message.role == MessageRole::User {
            "USER"
        } else {
            "ASSISTANT"
        };
        let text: String = turn.message.content.chars().take(1500).collect();
        out.push_str(&format!("{who}: {text}\n"));
    }
    out.push('\n');
    out
}

/// enowx's name for an engine's tool call, and arguments its interface can
/// show.
fn tool_shape(update: &Value) -> (String, String) {
    let kind = update
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("other");
    let title = update
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("tool")
        .to_owned();
    let raw = update.get("rawInput").cloned().unwrap_or(Value::Null);
    let path = update
        .pointer("/locations/0/path")
        .and_then(Value::as_str)
        .or_else(|| raw.get("file_path").and_then(Value::as_str))
        .or_else(|| raw.get("path").and_then(Value::as_str))
        .unwrap_or_default()
        .to_owned();
    let pick = |key: &str| raw.get(key).and_then(Value::as_str).map(str::to_owned);
    let (name, args) = match kind {
        "execute" => (
            "bash",
            json!({ "command": pick("command").unwrap_or_else(|| title.clone()) }),
        ),
        "read" => ("read", json!({ "path": path })),
        "edit" => ("edit", json!({ "path": path })),
        "delete" => ("delete", json!({ "path": path })),
        "move" => ("move", json!({ "path": path })),
        "search" => (
            "grep",
            json!({ "pattern": pick("pattern").or_else(|| pick("query")).unwrap_or_else(|| title.clone()) }),
        ),
        "fetch" => (
            "fetch",
            json!({ "url": pick("url").unwrap_or_else(|| title.clone()) }),
        ),
        _ => return (title, raw.to_string()),
    };
    (name.to_owned(), args.to_string())
}

/// The text of a tool call's output, and the text a diff replaced.
fn tool_output(update: &Value) -> (String, Option<String>) {
    let mut text = Vec::new();
    let mut before = None;
    for block in update
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        match block.get("type").and_then(Value::as_str) {
            Some("diff") => {
                before = block
                    .get("oldText")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                if let Some(new) = block.get("newText").and_then(Value::as_str) {
                    text.push(new.to_owned());
                }
            }
            Some("content") => {
                if let Some(t) = block.pointer("/content/text").and_then(Value::as_str) {
                    text.push(t.to_owned());
                }
            }
            Some("terminal") => text.push("(terminal output)".into()),
            _ => {}
        }
    }
    if text.is_empty() {
        if let Some(raw) = update.get("rawOutput") {
            text.push(match raw {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            });
        }
    }
    (text.join("\n"), before)
}

/// The option of a permission prompt whose kind starts with `prefix`
/// (`allow`, `reject`), once options preferred over always ones.
fn option_of(params: &Value, prefix: &str) -> Option<String> {
    let options = params.get("options").and_then(Value::as_array)?;
    let pick = |want: &str| {
        options
            .iter()
            .find(|o| o.get("kind").and_then(Value::as_str) == Some(want))
            .and_then(|o| o.get("optionId").and_then(Value::as_str))
            .map(str::to_owned)
    };
    pick(&format!("{prefix}_once")).or_else(|| pick(&format!("{prefix}_always")))
}

/// A data URL as an ACP image block.
fn image_block(attachment: &Attachment) -> Option<Value> {
    let rest = attachment.data_url.strip_prefix("data:")?;
    let (mime, data) = rest.split_once(";base64,")?;
    Some(json!({ "type": "image", "mimeType": mime, "data": data }))
}

impl Agent {
    /// Run `active`'s turn on `engine_id`. Ends the turn the way a native
    /// one does: the reply in the session, `Done`, the conversation given
    /// back.
    pub(super) async fn run_acp_turn(
        &self,
        engine_id: &str,
        active: &crate::agent_def::AgentDef,
        mut session: Session,
        events: &mpsc::Sender<Event>,
        cancel: CancellationToken,
    ) -> Result<Option<String>> {
        let workspace = std::fs::canonicalize(self.config.workspace())
            .context("resolving the configured workspace")?;
        let title = crate::acp::title(engine_id);
        let engine = match manager()
            .engine(&self.config.acp, engine_id, &workspace)
            .await
        {
            Ok(engine) => engine,
            Err(why) => anyhow::bail!("{why}"),
        };
        let settings = self.config.acp.engine(engine_id);
        let holds_conversation = session.parent.is_none();
        let key = format!("{}:{}", session.id, active.name);

        let (bound, fresh) = match manager().bound(&key, &engine) {
            Some(bound) => (bound, false),
            None => {
                let host = mcp_host::host().map_err(|e| anyhow::anyhow!(e))?;
                let (route, entry) = host.route();
                // Offered before the session exists: an agent lists the
                // server's tools while `session/new` runs, and keeps that list.
                let (tools, instructions) = self.acp_tools(active, holds_conversation).await;
                route.offer(tools, instructions);
                let mut prompt = self.agent_prompt(active, &workspace.to_string_lossy());
                if session.parent.is_some() {
                    prompt.push_str(REPORT_CONTRACT);
                }
                prompt.push_str(&acp_note(&active.name));
                let mut params = json!({
                    "cwd": workspace.display().to_string(),
                    "mcpServers": [entry],
                });
                if engine_id == "claude" {
                    // `strictMcpConfig`: only enowx's server, not every one in
                    // the user's own Claude config, some of which time out and
                    // held the first token back by about 30 s. `summarized`
                    // thinking: recent models otherwise stream thinking with
                    // no text to show.
                    params["_meta"] = json!({
                        "systemPrompt": { "append": prompt },
                        "claudeCode": { "options": {
                            "strictMcpConfig": true,
                            "thinking": { "type": "adaptive", "display": "summarized" }
                        } }
                    });
                }
                let created = match engine.process.request("session/new", params).await {
                    Ok(created) => created,
                    Err(error) if error.needs_login() => anyhow::bail!(
                        "{title} is not signed in. {}",
                        crate::acp::kind(engine_id).map_or("", |k| k.login_hint)
                    ),
                    Err(error) => anyhow::bail!("{title}: {error}"),
                };
                let acp_session = created
                    .get("sessionId")
                    .and_then(Value::as_str)
                    .context("the agent made no session")?
                    .to_owned();
                for note in configure(&engine, &acp_session, &created, &settings).await {
                    let _ = events
                        .send(Event::Notice {
                            message: format!("{title}: {note}"),
                        })
                        .await;
                }
                manager().remember(&engine, &created);
                let bound = Bound {
                    engine: engine_id.to_owned(),
                    acp_session,
                    route,
                    generation: engine.generation,
                };
                manager().bind(&key, bound.clone());
                (bound, true)
            }
        };

        // The message: the last one in the session (a handoff continuation
        // adds none), with the prompt and history for a fresh session.
        let last_user = session
            .turns
            .iter()
            .rev()
            .find(|t| t.message.role == MessageRole::User)
            .map(|t| t.message.clone())
            .unwrap_or_else(|| Message::user(""));
        let mut text = String::new();
        if fresh && engine_id != "claude" {
            let mut prompt = self.agent_prompt(active, &workspace.to_string_lossy());
            if session.parent.is_some() {
                prompt.push_str(REPORT_CONTRACT);
            }
            prompt.push_str(&acp_note(&active.name));
            text.push_str(&format!("<instructions>\n{prompt}\n</instructions>\n\n"));
        }
        if fresh {
            text.push_str(&history(&session));
        }
        text.push_str(&last_user.content);
        let mut blocks = vec![json!({ "type": "text", "text": text })];
        if engine.takes_images() {
            blocks.extend(last_user.attachments.iter().filter_map(image_block));
        }

        let mut incoming = engine.listen(&bound.acp_session);
        let (call_tx, mut calls) = mpsc::unbounded_channel();
        bound.route.attach(call_tx);
        let message_id = uuid::Uuid::new_v4().to_string();
        let _ = events
            .send(Event::MessageStart {
                id: message_id.clone(),
            })
            .await;

        let process = engine.process.clone();
        let sid = bound.acp_session.clone();
        let prompt = process.request(
            "session/prompt",
            json!({ "sessionId": sid, "prompt": blocks }),
        );
        tokio::pin!(prompt);
        let mut reply = String::new();
        let mut reasoning = String::new();
        let mut tools_seen: HashMap<String, String> = HashMap::new();
        let mut last_plan = String::new();
        let mut cancel_sent: Option<tokio::time::Instant> = None;
        let stop_reason = loop {
            let deadline = cancel_sent.map(|at| at + Duration::from_secs(10));
            tokio::select! {
                answer = &mut prompt => break match answer {
                    Ok(done) => match done.get("stopReason").and_then(Value::as_str) {
                        Some("end_turn") | None => "stop".to_owned(),
                        Some("max_tokens") => "length".to_owned(),
                        Some(other) => other.to_owned(),
                    },
                    Err(error) => {
                        let _ = events
                            .send(Event::Error { message: format!("{title}: {error}") })
                            .await;
                        "error".to_owned()
                    }
                },
                Some(message) = incoming.recv() => match message {
                    Incoming::Update(params) => {
                        self.acp_update(&params, events, &mut reply, &mut reasoning, &mut tools_seen, &mut last_plan).await;
                    }
                    Incoming::Permission(params, answer) => {
                        let choice = self
                            .acp_permission(engine_id, &settings, &params, active, holds_conversation, events, &cancel)
                            .await;
                        let _ = answer.send(choice);
                    }
                    Incoming::Exited(detail) => {
                        let _ = events
                            .send(Event::Error { message: format!("{title} stopped{detail}") })
                            .await;
                        break "error".to_owned();
                    }
                },
                Some(call) = calls.recv() => {
                    let result = self.acp_call(&call.name, call.arguments, &mut session, active, holds_conversation, events, &cancel).await;
                    let _ = call.reply.send(result);
                }
                _ = cancel.cancelled(), if cancel_sent.is_none() => {
                    process.notify("session/cancel", json!({ "sessionId": sid }));
                    cancel_sent = Some(tokio::time::Instant::now());
                }
                _ = async { tokio::time::sleep_until(deadline.unwrap_or_else(tokio::time::Instant::now)).await }, if deadline.is_some() => {
                    break "cancelled".to_owned();
                }
            }
        };
        // Updates sent just before the answer to the prompt can still be
        // queued: the end of the reply is among them.
        while let Ok(message) = incoming.try_recv() {
            if let Incoming::Update(params) = message {
                self.acp_update(
                    &params,
                    events,
                    &mut reply,
                    &mut reasoning,
                    &mut tools_seen,
                    &mut last_plan,
                )
                .await;
            }
        }
        bound.route.detach();
        engine.stop_listening(&bound.acp_session);

        let mut message = Message::assistant(reply);
        if !reasoning.is_empty() {
            message.reasoning = Some(reasoning);
        }
        message.model = Some(format!("{engine_id} (ACP)"));
        message.message_id = Some(message_id);
        message.interrupted = stop_reason == "cancelled";
        session.push(message);
        self.store.save(&session)?;
        self.give_back(&mut session, events).await?;
        let _ = events.send(Event::Done { stop_reason }).await;
        Ok(None)
    }

    /// The tools enowx offers an engine, as MCP tools, and the server's
    /// instructions.
    async fn acp_tools(
        &self,
        active: &crate::agent_def::AgentDef,
        holds_conversation: bool,
    ) -> (Vec<Value>, String) {
        let mut schemas = self
            .tools
            .read()
            .await
            .schemas_for_agent(&[], !self.discovery.carried_by(active).is_empty());
        let mut said =
            vec!["`skill_read` reads one of the skills your instructions list".to_owned()];
        if self.asks_user && holds_conversation {
            schemas.push(crate::ask::schema());
            said.push("`ask` puts questions to the user and waits for the answers".into());
        }
        if active.delegation != crate::agent_def::Delegation::None {
            schemas.extend(
                crate::routing::routing_schemas(crate::routing::HandOff::No)
                    .into_iter()
                    .filter(|s| {
                        s.pointer("/function/name").and_then(Value::as_str) == Some("delegate")
                    }),
            );
            said.push(
                "`delegate` gives a self-contained task to one of enowx's specialists and returns \
                 its report"
                    .into(),
            );
        }
        let tools = schemas.iter().filter_map(mcp_host::mcp_tool).collect();
        let instructions = format!(
            "enowx is the app running you. Its tools here: {}. Tools named mcp__… are MCP servers \
             enowx has connected.",
            said.join("; ")
        );
        (tools, instructions)
    }

    /// Run one of enowx's tools for an engine.
    #[allow(clippy::too_many_arguments)]
    async fn acp_call(
        &self,
        name: &str,
        arguments: Value,
        session: &mut Session,
        active: &crate::agent_def::AgentDef,
        holds_conversation: bool,
        events: &mpsc::Sender<Event>,
        cancel: &CancellationToken,
    ) -> Value {
        match name {
            crate::ask::TOOL => {
                let id = uuid::Uuid::new_v4().to_string();
                let output = self
                    .ask_user(
                        &id,
                        &active.name,
                        &arguments,
                        holds_conversation,
                        events,
                        cancel,
                    )
                    .await;
                mcp_host::tool_text(&output.content, output.is_error)
            }
            "delegate" => {
                let switch = crate::routing::parse_switch("delegate", &arguments);
                let Some(switch @ crate::routing::Switch::Delegate(_)) = switch else {
                    return mcp_host::tool_error("malformed delegate call");
                };
                if let Err(refusal) =
                    crate::routing::authorise(active, &switch, &self.discovery.agents)
                {
                    return mcp_host::tool_error(&refusal.message());
                }
                let crate::routing::Switch::Delegate(delegation) = switch else {
                    unreachable!("matched above");
                };
                let reports = self.run_delegations(session, &[delegation], events).await;
                let _ = self.store.save(session);
                let text = reports
                    .iter()
                    .map(|r| crate::routing::report_message_of(r, is_kept(r)))
                    .collect::<Vec<_>>()
                    .join("\n\n");
                mcp_host::tool_text(&text, reports.iter().any(|r| r.failed))
            }
            _ => {
                let workspace = std::fs::canonicalize(self.config.workspace())
                    .unwrap_or_else(|_| self.config.workspace());
                let ctx = ToolCtx {
                    skills: self.discovery.carried_by(active),
                    lsp: None,
                    vision: false,
                    cloudflare_token: None,
                    repair: None,
                    workspace,
                    shell_timeout: Duration::from_secs(self.config.agent.shell_timeout_secs),
                    cancel: cancel.clone(),
                    progress: None,
                    call_id: uuid::Uuid::new_v4().to_string(),
                };
                let output = self
                    .tools
                    .read()
                    .await
                    .execute_for_agent(&[], &ctx, name, arguments)
                    .await;
                mcp_host::tool_text(&output.content, output.is_error)
            }
        }
    }

    /// Show one `session/update` as enowx's events.
    async fn acp_update(
        &self,
        params: &Value,
        events: &mpsc::Sender<Event>,
        reply: &mut String,
        reasoning: &mut String,
        tools_seen: &mut HashMap<String, String>,
        last_plan: &mut String,
    ) {
        let Some(update) = params.get("update") else {
            return;
        };
        let text_of = |u: &Value| {
            u.pointer("/content/text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        match update.get("sessionUpdate").and_then(Value::as_str) {
            Some("agent_message_chunk") => {
                let delta = text_of(update);
                if !delta.is_empty() {
                    reply.push_str(&delta);
                    let _ = events.send(Event::Text { delta }).await;
                }
            }
            Some("agent_thought_chunk") => {
                let delta = text_of(update);
                if !delta.is_empty() {
                    reasoning.push_str(&delta);
                    let _ = events.send(Event::Reasoning { delta }).await;
                }
            }
            Some(kind @ ("tool_call" | "tool_call_update")) => {
                let id = update
                    .get("toolCallId")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                if kind == "tool_call" || !tools_seen.contains_key(&id) {
                    let (name, arguments) = tool_shape(update);
                    if kind == "tool_call" || update.get("title").is_some() {
                        tools_seen.insert(id.clone(), name.clone());
                        let _ = events
                            .send(Event::ToolCall {
                                id: id.clone(),
                                name,
                                arguments,
                            })
                            .await;
                    }
                }
                let status = update.get("status").and_then(Value::as_str);
                if matches!(status, Some("completed" | "failed")) {
                    let name = tools_seen
                        .get(&id)
                        .cloned()
                        .unwrap_or_else(|| "tool".into());
                    let (content, before) = tool_output(update);
                    let _ = events
                        .send(Event::ToolResult {
                            id,
                            name,
                            content,
                            is_error: status == Some("failed"),
                            before,
                        })
                        .await;
                }
            }
            Some("plan") => {
                let entries = update
                    .get("entries")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let lines: Vec<String> = entries
                    .iter()
                    .map(|e| {
                        let mark = match e.get("status").and_then(Value::as_str) {
                            Some("completed") => "[x]",
                            Some("in_progress") => "[~]",
                            _ => "[ ]",
                        };
                        format!(
                            "{mark} {}",
                            e.get("content").and_then(Value::as_str).unwrap_or_default()
                        )
                    })
                    .collect();
                let plan = lines.join("\n");
                if plan != *last_plan && !plan.is_empty() {
                    *last_plan = plan.clone();
                    let id = format!("acp-plan-{}", uuid::Uuid::new_v4());
                    let items: Vec<Value> = entries
                        .iter()
                        .filter_map(|e| e.get("content").cloned())
                        .collect();
                    let _ = events
                        .send(Event::ToolCall {
                            id: id.clone(),
                            name: "todo".into(),
                            arguments: json!({ "op": "set", "items": items }).to_string(),
                        })
                        .await;
                    let _ = events
                        .send(Event::ToolResult {
                            id,
                            name: "todo".into(),
                            content: plan,
                            is_error: false,
                            before: None,
                        })
                        .await;
                }
            }
            _ => {}
        }
    }

    /// Answer an engine's permission prompt: `Some(option)` or `None` to
    /// cancel it.
    #[allow(clippy::too_many_arguments)]
    async fn acp_permission(
        &self,
        engine_id: &str,
        settings: &EngineConfig,
        params: &Value,
        active: &crate::agent_def::AgentDef,
        holds_conversation: bool,
        events: &mpsc::Sender<Event>,
        cancel: &CancellationToken,
    ) -> Option<String> {
        let call = params.get("toolCall").cloned().unwrap_or(Value::Null);
        let (name, arguments) = tool_shape(&call);
        let command = serde_json::from_str::<Value>(&arguments)
            .ok()
            .and_then(|a| a.get("command").and_then(Value::as_str).map(str::to_owned));
        let allow = || option_of(params, "allow");
        let reject = || option_of(params, "reject");
        match settings.permission.as_str() {
            "bypass" => return allow(),
            "enowx" => {
                // enowx's own tools run without asking; the shell gate, when
                // the decision model is on, is the one check.
                let Some(command) = command
                    .clone()
                    .filter(|_| name == "bash" && self.decider.is_on(crate::decision::Use::Shell))
                else {
                    return allow();
                };
                let ruling = crate::decision::uses::shell(&self.decider, &command).await;
                Self::report_decision(ruling.record, events).await;
                if ruling.verdict == crate::decision::uses::ShellVerdict::Run {
                    return allow();
                }
            }
            _ => {}
        }
        // Ask the user.
        if !self.asks_user || !holds_conversation {
            let _ = events
                .send(Event::Notice {
                    message: format!(
                        "{}: refused `{}`, with no one to ask in a delegated agent. Set its \
                         permissions to enowx's rules in Settings > ACP agents to let it run.",
                        crate::acp::title(engine_id),
                        call.get("title").and_then(Value::as_str).unwrap_or(&name)
                    ),
                })
                .await;
            return reject();
        }
        let options: Vec<(String, String)> = params
            .get("options")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|o| {
                Some((
                    o.get("optionId")?.as_str()?.to_owned(),
                    o.get("name")?.as_str()?.to_owned(),
                ))
            })
            .collect();
        if options.is_empty() {
            return None;
        }
        let what = call
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or(&name)
            .to_owned();
        let detail = command.map(|c| format!("\n\n{c}")).unwrap_or_default();
        let question = json!({ "questions": [{
            "question": format!("{} wants to: {what}{detail}", crate::acp::title(engine_id)),
            "header": "Permission",
            "options": options.iter().map(|(_, label)| json!({ "label": label })).collect::<Vec<_>>(),
        }] });
        let questions = crate::ask::parse(&question).ok()?;
        let id = uuid::Uuid::new_v4().to_string();
        let answer = self
            .wait_for_answer(&id, &active.name, &questions, events, cancel)
            .await?;
        let chosen = answer.replies.first()?.chosen.first()?.clone();
        options
            .into_iter()
            .find(|(_, label)| *label == chosen)
            .map(|(id, _)| id)
    }
}
