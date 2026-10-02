//! MCP servers start in the background and never hold the first token.
//!
//! They used to start one after another before the first model call, each
//! with a 30-second handshake timeout: one server that never answered (a
//! `dokploy` whose backend was gone) made every first message wait 33
//! seconds, against a provider answering in half a second.

mod common;

use std::time::{Duration, Instant};

use enowx_core::{
    builtin_agents,
    config::Config,
    discovery::{McpServer, McpTransport, SkillScope},
    Agent, Discovery, Event, Role, SessionStore,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn answering_provider() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            if !common::is_model_call(&socket).await {
                continue;
            }
            tokio::spawn(async move {
                let mut buf = [0u8; 65536];
                let _ = socket.read(&mut buf).await;
                let text = serde_json::json!({
                    "choices": [{ "delta": { "content": "ok" }, "finish_reason": null }]
                });
                let finish = serde_json::json!({
                    "choices": [{ "delta": {}, "finish_reason": "stop" }]
                });
                let sse = format!("data: {text}\n\ndata: {finish}\n\ndata: [DONE]\n\n");
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{sse}",
                    sse.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
            });
        }
    });
    format!("http://127.0.0.1:{port}/v1")
}

fn server(name: &str, command: &str, args: &[&str]) -> McpServer {
    McpServer {
        name: name.into(),
        scope: SkillScope::User,
        command_or_url: command.into(),
        args: args.iter().map(|a| (*a).to_owned()).collect(),
        env: Default::default(),
        transport: McpTransport::Stdio,
        source: Default::default(),
        enabled: true,
        builtin: false,
        configured: true,
    }
}

fn agent_with(servers: Vec<McpServer>, base_url: String, dir: &std::path::Path) -> Agent {
    let mut config = Config::default();
    config.use_endpoint("test", &base_url, "test-key", "test-model");
    config.agent.workspace = Some(dir.to_path_buf());
    config.agent.max_steps = 2;
    config.agent.auto_compact = false;
    let discovery = Discovery {
        agents: builtin_agents(),
        mcp_servers: servers,
        ..Discovery::default()
    };
    Agent::with_discovery(
        config,
        SessionStore::new(dir.join(".enx-sessions")),
        discovery,
    )
}

/// Seconds from the start of a turn to its first answer token.
async fn first_token_after(agent: &std::sync::Arc<Agent>) -> f64 {
    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    let request = enowx_core::agent::RunRequest {
        prompt: "hi".into(),
        session_id: None,
        role: Role::Orchestrator,
        attachments: Vec::new(),
        agent: None,
    };
    let runner = agent.clone();
    let started = Instant::now();
    let handle = tokio::spawn(async move {
        runner
            .run(request, tx, tokio_util::sync::CancellationToken::new())
            .await
    });
    let mut first = None;
    while let Some(event) = rx.recv().await {
        if matches!(event, Event::Text { .. }) && first.is_none() {
            first = Some(started.elapsed().as_secs_f64());
        }
    }
    let _ = handle.await;
    first.expect("the model should have answered")
}

fn scratch(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("enx-mcp-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A server that starts but never answers its handshake costs the first
/// turn a short grace at most, and later turns nothing.
#[tokio::test]
async fn a_hung_server_does_not_hold_the_first_token() {
    let dir = scratch("hung");
    let agent = std::sync::Arc::new(agent_with(
        vec![server("hung", "sleep", &["60"])],
        answering_provider().await,
        &dir,
    ));
    let first = first_token_after(&agent).await;
    assert!(
        first < 5.0,
        "first turn waited {first:.1}s on a hung server"
    );
    assert!(agent.mcp_starting(), "the hung server is still starting");
    let second = first_token_after(&agent).await;
    assert!(second < 1.0, "the second turn waited {second:.1}s");
    let _ = std::fs::remove_dir_all(dir);
}

/// A server that cannot start says why, so it can be fixed or switched off.
#[tokio::test]
async fn a_server_that_fails_records_why() {
    let dir = scratch("ghost");
    let agent = agent_with(
        vec![server("ghost", "/definitely/not/a/command", &[])],
        answering_provider().await,
        &dir,
    );
    agent.start_mcp();
    let deadline = Instant::now() + Duration::from_secs(5);
    while agent.mcp_starting() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let reason = agent.mcp_failure("ghost").expect("a recorded failure");
    assert_eq!(reason, "command not found: /definitely/not/a/command");
    let _ = std::fs::remove_dir_all(dir);
}
