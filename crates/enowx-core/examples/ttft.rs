//! Where the wait before the first token goes. Builds the agent the way the
//! interface does (the real config, its MCP servers and TypeSafe), runs two
//! turns, and prints each event's time since the turn started, next to a bare
//! provider call with no agent around it.
//!
//! cargo run -q -p enowx-core --example ttft -- [PROMPT] [WORKSPACE]
//!
//! Spends a few model calls on the configured provider.

use std::path::PathBuf;
use std::time::Instant;

use enowx_core::{config::Config, Agent, Event, Role, SessionStore};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let prompt = args
        .next()
        .unwrap_or_else(|| "Reply with one word: ok".into());
    let workspace = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join(format!("enx-ttft-{}", std::process::id())));
    std::fs::create_dir_all(&workspace)?;
    let workspace = std::fs::canonicalize(&workspace)?;

    let mut config = Config::load()?;
    config.agent.workspace = Some(workspace.clone());
    println!(
        "provider  {} ({})",
        config.provider.name, config.provider.base_url
    );
    println!("model     {}", config.model.default);
    println!(
        "typesafe  gate_tool_results={} rank_compaction={} key={}",
        config.typesafe.gate_tool_results,
        config.typesafe.rank_compaction,
        if config.typesafe.api_key.is_empty() {
            "no"
        } else {
            "yes"
        }
    );

    // A bare call: the provider alone, no agent, no tools.
    let provider = enowx_core::provider::Provider::from_config(&config)?;
    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    let started = Instant::now();
    let call = tokio::spawn(async move {
        provider
            .complete(
                &[enowx_core::message::Message::user(
                    "Reply with one word: ok",
                )],
                &[],
                &tx,
            )
            .await
    });
    let mut first = None;
    while let Some(_chunk) = rx.recv().await {
        first.get_or_insert(started.elapsed());
    }
    let _ = call.await?;
    println!(
        "\nbare call: first chunk {:.2}s, done {:.2}s",
        first.map(|d| d.as_secs_f64()).unwrap_or(f64::NAN),
        started.elapsed().as_secs_f64()
    );

    // The agent, built as the interface builds it.
    let built = Instant::now();
    let sessions = workspace.with_file_name(format!(
        "{}-sessions",
        workspace
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    let agent = std::sync::Arc::new(Agent::with_store(
        config.clone(),
        SessionStore::new(sessions),
    ));
    println!("\nagent built in {:.2}s", built.elapsed().as_secs_f64());
    let discovery = enowx_core::Discovery::run(&workspace);
    let enabled: Vec<&str> = discovery
        .mcp_servers
        .iter()
        .filter(|s| s.enabled)
        .map(|s| s.name.as_str())
        .collect();
    println!(
        "discovery: {} MCP servers enabled: {}; {} skills; {} instruction files",
        enabled.len(),
        enabled.join(", "),
        discovery.skills.len(),
        discovery.instructions.len()
    );

    let mut session_id: Option<String> = None;
    for turn in 1..=2 {
        println!("\nturn {turn}");
        let (tx, mut rx) = tokio::sync::mpsc::channel(1024);
        let request = enowx_core::agent::RunRequest {
            prompt: prompt.clone(),
            session_id: session_id.clone(),
            role: Role::Orchestrator,
            attachments: Vec::new(),
        };
        let cancel = tokio_util::sync::CancellationToken::new();
        let runner = agent.clone();
        let started = Instant::now();
        let handle = tokio::spawn(async move { runner.run(request, tx, cancel).await });
        let mut first_reasoning = None;
        let mut first_text = None;
        while let Some(event) = rx.recv().await {
            let at = started.elapsed().as_secs_f64();
            match &event {
                Event::Session { id, .. } => {
                    session_id.get_or_insert_with(|| id.clone());
                    println!("  {at:6.2}s session");
                }
                Event::MessageStart { .. } => println!("  {at:6.2}s model call starts"),
                Event::Reasoning { .. } if first_reasoning.is_none() => {
                    first_reasoning = Some(at);
                    println!("  {at:6.2}s first reasoning token");
                }
                Event::Text { .. } if first_text.is_none() => {
                    first_text = Some(at);
                    println!("  {at:6.2}s first answer token");
                }
                Event::ToolCall { name, .. } => println!("  {at:6.2}s tool call {name}"),
                Event::ToolResult { name, .. } => println!("  {at:6.2}s tool result {name}"),
                Event::Trimmed { tool, .. } => println!("  {at:6.2}s typesafe trimmed {tool}"),
                Event::Notice { message } => println!("  {at:6.2}s notice: {message}"),
                Event::Usage { .. } => println!("  {at:6.2}s usage"),
                Event::Error { message } => println!("  {at:6.2}s error: {message}"),
                Event::Done { .. } => println!("  {at:6.2}s done"),
                _ => {}
            }
        }
        handle.await??;
    }
    Ok(())
}
