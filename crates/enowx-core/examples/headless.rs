//! Run one request through the agent loop on the configured model and print
//! what happened: each tool call as it runs, then every session involved
//! (the main one and each sub-agent's branch) with its steps, calls and spend.
//! For checking how the agents behave on a real model without the interface.
//!
//! cargo run -q -p enowx-core --example headless -- "build a simple portfolio" [WORKSPACE]
//!
//! This calls the configured provider and costs what the run costs. The
//! workspace defaults to a fresh temporary directory; MCP servers are not
//! started, and sessions are stored beside the workspace, not in ~/.enx.

use std::path::PathBuf;
use std::time::Instant;

use enowx_core::{config::Config, Agent, Discovery, Event, Role, Session, SessionStore};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let prompt = args.next().expect("usage: headless PROMPT [WORKSPACE]");
    let workspace = args.next().map(PathBuf::from).unwrap_or_else(|| {
        std::env::temp_dir().join(format!(
            "enx-headless-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        ))
    });
    std::fs::create_dir_all(&workspace)?;
    let workspace = std::fs::canonicalize(&workspace)?;

    let mut config = Config::load()?;
    config.agent.workspace = Some(workspace.clone());
    let mut discovery = Discovery::run(&workspace);
    discovery.mcp_servers.clear();
    // Beside the workspace, not in it: the agents would find the session
    // files with a glob and read them, which no real run can do.
    let sessions = workspace.with_file_name(format!(
        "{}-sessions",
        workspace
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    let store = SessionStore::new(sessions.clone());
    let agent = Agent::with_discovery(config.clone(), store.clone(), discovery);

    println!("workspace {}", workspace.display());
    println!("sessions  {}", sessions.display());
    println!("model     {}", config.model.default);
    let started = Instant::now();
    let (tx, mut rx) = tokio::sync::mpsc::channel(1024);
    let request = enowx_core::agent::RunRequest {
        prompt,
        session_id: None,
        role: Role::Orchestrator,
        attachments: Vec::new(),
        agent: None,
    };
    let cancel = tokio_util::sync::CancellationToken::new();
    let handle = tokio::spawn(async move { agent.run(request, tx, cancel).await });

    let mut session_id: Option<String> = None;
    let mut reply = String::new();
    while let Some(event) = rx.recv().await {
        match event {
            Event::Session { id, .. } => {
                session_id.get_or_insert(id);
            }
            Event::ToolCall {
                name, arguments, ..
            } => println!("  {name:<10} {}", short(&arguments)),
            Event::AgentSwitched { to, reason } => println!("→ {to}: {reason}"),
            Event::DelegationStarted { agent, task, .. } => {
                println!("delegate → {agent}: {}", first_line(&task))
            }
            Event::DelegationFinished {
                agent,
                summary,
                failed,
                ..
            } => println!(
                "{agent} {}\n{}",
                if failed { "failed" } else { "reported" },
                indent(&summary)
            ),
            Event::Text { delta } => reply.push_str(&delta),
            Event::Notice { message } => println!("notice: {message}"),
            Event::Error { message } => println!("error: {message}"),
            Event::Done { stop_reason } => println!("done ({stop_reason})"),
            _ => {}
        }
    }
    handle.await??;
    let elapsed = started.elapsed().as_secs();

    println!("\nreply\n{}", indent(reply.trim()));
    let Some(id) = session_id else {
        return Ok(());
    };
    let main = store.load(&id)?;
    let mut sessions = vec![main.clone()];
    for record in &main.delegations {
        sessions.push(store.load(&record.session_id)?);
    }
    println!("\nsession     steps  calls  in        out     tools");
    let (mut total_in, mut total_out) = (0u64, 0u64);
    for session in &sessions {
        let name = if session.parent.is_some() {
            format!("  {}", session.agent)
        } else {
            "main".to_owned()
        };
        let (steps, calls, tools) = tally(session);
        // The main session's usage already includes its branches'.
        if session.parent.is_none() {
            total_in = session.usage.input_tokens as u64;
            total_out = session.usage.output_tokens as u64;
        }
        println!(
            "{name:<11} {steps:>5}  {calls:>5}  {:>8}  {:>6}  {tools}",
            session.usage.input_tokens, session.usage.output_tokens
        );
    }
    let cost = total_in as f64 * config.model.price_input / 1e6
        + total_out as f64 * config.model.price_output / 1e6;
    println!("\ntotal {total_in} in, {total_out} out, ${cost:.4}, {elapsed}s");

    println!("\nfiles");
    for entry in walk(&workspace) {
        println!("  {entry}");
    }
    Ok(())
}

/// Model calls, tool calls, and calls per tool, for one session.
fn tally(session: &Session) -> (usize, usize, String) {
    let mut counts: std::collections::BTreeMap<&str, usize> = Default::default();
    let mut steps = 0;
    for turn in &session.turns {
        if matches!(turn.message.role, enowx_core::message::Role::Assistant) {
            steps += 1;
        }
        for call in &turn.message.tool_calls {
            *counts.entry(call.name.as_str()).or_default() += 1;
        }
    }
    let calls = counts.values().sum();
    let tools = counts
        .iter()
        .map(|(name, n)| format!("{name}×{n}"))
        .collect::<Vec<_>>()
        .join(" ");
    (steps, calls, tools)
}

fn walk(root: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(relative) = path.strip_prefix(root) {
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                out.push(format!("{} ({size} bytes)", relative.display()));
            }
        }
    }
    out.sort();
    out
}

fn short(arguments: &str) -> String {
    let flat: String = arguments.chars().filter(|c| *c != '\n').take(90).collect();
    flat
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or("")
}

fn indent(text: &str) -> String {
    text.lines()
        .map(|line| format!("  {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}
