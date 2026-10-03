//! The orchestrator stops one of its delegations still at work: it sees the
//! session in the list of what is running, calls `stop_delegation`, and the
//! delegation's report comes back saying it was stopped, by whom and why.

mod common;

use std::time::Duration;

use enowx_core::{builtin_agents, config::Config, Agent, Discovery, Role, SessionStore};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn request_body(socket: &mut tokio::net::TcpStream) -> String {
    let mut data = Vec::new();
    let mut buf = [0u8; 16384];
    loop {
        let n = socket.read(&mut buf).await.unwrap_or(0);
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
        let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") else {
            continue;
        };
        let head = String::from_utf8_lossy(&data[..end]).to_ascii_lowercase();
        let length = head
            .lines()
            .find_map(|line| line.strip_prefix("content-length:"))
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        if data.len() >= end + 4 + length {
            return String::from_utf8_lossy(&data[end + 4..end + 4 + length]).into_owned();
        }
    }
    String::from_utf8_lossy(&data).into_owned()
}

fn says(text: &str) -> String {
    let delta = serde_json::json!({
        "choices": [{ "delta": { "content": text }, "finish_reason": null }]
    });
    let finish = serde_json::json!({ "choices": [{ "delta": {}, "finish_reason": "stop" }] });
    format!("data: {delta}\n\ndata: {finish}\n\ndata: [DONE]\n\n")
}

fn calls(name: &str, arguments: serde_json::Value) -> String {
    let call = serde_json::json!({
        "choices": [{
            "delta": {
                "tool_calls": [{
                    "index": 0,
                    "id": "call_1",
                    "type": "function",
                    "function": { "name": name, "arguments": arguments.to_string() }
                }]
            },
            "finish_reason": null
        }]
    });
    let finish = serde_json::json!({ "choices": [{ "delta": {}, "finish_reason": "tool_calls" }] });
    format!("data: {call}\n\ndata: {finish}\n\ndata: [DONE]\n\n")
}

/// What the stand-in model answers.
async fn answer(body: &str) -> String {
    if body.contains("End every turn with this report") {
        // At work: one slow look after another, until it is stopped.
        tokio::time::sleep(Duration::from_millis(400)).await;
        // A different look each step, so the loop guard does not end it.
        let step = body.matches("\"role\":\"tool\"").count();
        return calls("glob", serde_json::json!({"pattern": format!("*.{step}")}));
    }
    if body.contains("STOP-NOW") {
        if body.contains("Stopping `fe`") {
            return says("Stopped it.");
        }
        let id = body
            .split("(session `")
            .nth(1)
            .and_then(|rest| rest.split('`').next())
            .unwrap_or("")
            .to_owned();
        return calls(
            "stop_delegation",
            serde_json::json!({"session": id, "reason": "the user changed the design"}),
        );
    }
    if body.contains("Started in the background") {
        return says("Frontend is on it.");
    }
    calls(
        "delegate",
        serde_json::json!({"agent": "fe", "task": "build the page"}),
    )
}

#[tokio::test]
async fn the_orchestrator_stops_a_delegation_and_hears_why() {
    let dir = std::env::temp_dir().join(format!("enx-stop-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
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
                let body = request_body(&mut socket).await;
                let reply = answer(&body).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.flush().await;
            });
        }
    });
    let mut config = Config::default();
    config.use_endpoint(
        "test",
        &format!("http://127.0.0.1:{port}/v1"),
        "test-key",
        "test-model",
    );
    config.agent.workspace = Some(dir.clone());
    config.agent.auto_compact = false;
    config.agent.lsp = false;
    let agent = Agent::with_discovery(
        config,
        SessionStore::new(dir.with_extension("sessions")),
        Discovery {
            agents: builtin_agents(),
            ..Discovery::default()
        },
    )
    .into_shared();
    let mut background = agent.background_events();

    // First turn: fe starts in the background.
    let (tx, mut rx) = tokio::sync::mpsc::channel(1024);
    let turn = agent.clone();
    tokio::spawn(async move {
        turn.run(
            enowx_core::agent::RunRequest {
                prompt: "build the page".into(),
                session_id: None,
                role: Role::Orchestrator,
                attachments: Vec::new(),
                agent: Some("orchestrator".into()),
            },
            tx,
            tokio_util::sync::CancellationToken::new(),
        )
        .await
    });
    let mut session = String::new();
    while let Some(event) = tokio::time::timeout(Duration::from_secs(20), rx.recv())
        .await
        .expect("first turn events")
    {
        if let enowx_core::Event::Session { id, .. } = event {
            session = id;
        }
    }
    assert!(agent.delegations_running() > 0, "fe is at work");

    // Second turn: the orchestrator stops it.
    let (tx, _rx) = tokio::sync::mpsc::channel(1024);
    tokio::time::timeout(
        Duration::from_secs(30),
        agent.run(
            enowx_core::agent::RunRequest {
                prompt: "STOP-NOW the design changed".into(),
                session_id: Some(session.clone()),
                role: Role::Orchestrator,
                attachments: Vec::new(),
                agent: Some("orchestrator".into()),
            },
            tx,
            tokio_util::sync::CancellationToken::new(),
        ),
    )
    .await
    .expect("second turn finishes")
    .unwrap();

    // Its report comes back on the background channel, saying so.
    let mut report = String::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while report.is_empty() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "no report within 30s"
        );
        let event = tokio::time::timeout(Duration::from_secs(20), background.recv())
            .await
            .expect("the stopped delegation reports")
            .expect("the channel stays open");
        match event {
            enowx_core::Event::DelegationFinished { summary, .. } => report = summary,
            enowx_core::Event::DelegationsReported { reports, .. } => {
                report = reports
                    .first()
                    .map(|r| r.summary.clone())
                    .unwrap_or_default()
            }
            _ => {}
        }
    }
    assert!(
        report
            .starts_with("STOPPED by orchestrator before it finished: the user changed the design"),
        "{report}"
    );
    assert!(report.contains("delegation_log"), "{report}");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(dir.with_extension("sessions"));
}
