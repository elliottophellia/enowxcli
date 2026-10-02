//! A delegation still at work in the background is never run a second time.
//! The user says "continue" while `fe` is busy: the orchestrator is told what
//! is still running, and resuming that session is refused, so two runs of one
//! session never edit the same files.

mod common;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

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

/// What the stand-in model answers. `branch` is the session `fe` was given.
async fn answer(body: &str, branch: &str) -> String {
    let sub_agent = body.contains("End every turn with this report");
    if sub_agent {
        // Busy long enough for the user to say "continue" meanwhile.
        tokio::time::sleep(Duration::from_millis(2500)).await;
        return says(
            "DONE: built the page\nCHANGED: page.html\nVERIFIED: not verified\nNEXT: nothing",
        );
    }
    if body.contains("CONTINUE-NOW") {
        if body.contains("is still at work in the background") {
            return says("Frontend is still on it.");
        }
        // The note is there; resume anyway, as a model may.
        return calls(
            "delegate",
            serde_json::json!({"agent": "fe", "resume": branch, "task": "Lanjutkan: build the page"}),
        );
    }
    if body.contains("Started in the background") {
        return says("Frontend is on it.");
    }
    calls(
        "delegate",
        serde_json::json!({"agent": "fe", "task": "TASK-SLOW: build the page"}),
    )
}

#[tokio::test]
async fn a_delegation_at_work_is_not_resumed_twice() {
    let dir = std::env::temp_dir().join(format!("enx-double-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let branch = Arc::new(Mutex::new(String::new()));
    let bodies = Arc::new(Mutex::new(Vec::<String>::new()));

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    {
        let (branch, bodies) = (branch.clone(), bodies.clone());
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                if !common::is_model_call(&socket).await {
                    continue;
                }
                let (branch, bodies) = (branch.clone(), bodies.clone());
                tokio::spawn(async move {
                    let body = request_body(&mut socket).await;
                    bodies.lock().unwrap().push(body.clone());
                    let id = branch.lock().unwrap().clone();
                    let reply = answer(&body, &id).await;
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
    }

    let mut config = Config::default();
    config.use_endpoint(
        "test",
        &format!("http://127.0.0.1:{port}/v1"),
        "test-key",
        "test-model",
    );
    config.agent.workspace = Some(dir.clone());
    config.agent.auto_compact = false;
    let agent = Agent::with_discovery(
        config,
        SessionStore::new(dir.with_extension("sessions")),
        Discovery {
            agents: builtin_agents(),
            ..Discovery::default()
        },
    )
    .into_shared();
    let _background = agent.background_events();

    // First turn: fe starts in the background and the turn ends.
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
        match event {
            enowx_core::Event::Session { id, .. } => session = id,
            enowx_core::Event::DelegationStarted { session_id, .. } => {
                *branch.lock().unwrap() = session_id
            }
            _ => {}
        }
    }
    assert!(!branch.lock().unwrap().is_empty(), "fe was started");
    assert!(agent.delegations_running() > 0, "fe is still at work");
    // Second turn, while fe works: "continue".
    let (tx, mut rx) = tokio::sync::mpsc::channel(1024);
    tokio::time::timeout(
        Duration::from_secs(30),
        agent.run(
            enowx_core::agent::RunRequest {
                prompt: "CONTINUE-NOW lanjutkan".into(),
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
    let (mut started_again, mut said) = (0, String::new());
    while let Ok(event) = rx.try_recv() {
        match event {
            enowx_core::Event::DelegationStarted { .. } => started_again += 1,
            enowx_core::Event::Text { delta } => said.push_str(&delta),
            _ => {}
        }
    }
    assert_eq!(started_again, 0, "the busy session was run a second time");
    assert!(said.contains("Frontend is still on it"), "{said}");
    let told = bodies.lock().unwrap().iter().any(|body| {
        body.contains("CONTINUE-NOW") && body.contains("Still at work in the background")
    });
    assert!(told, "the orchestrator was not told what is still running");
    let _ = std::fs::remove_dir_all(&dir);
}
