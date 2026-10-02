//! A delegation that finished with its report leaves the report in its
//! caller's conversation and its transcript is cleared; one that stopped
//! without a report keeps its transcript, to be resumed.

mod common;

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

fn delegate(task: &str) -> String {
    let call = serde_json::json!({
        "choices": [{
            "delta": {
                "tool_calls": [{
                    "index": 0, "id": "call_1", "type": "function",
                    "function": { "name": "delegate", "arguments": serde_json::json!({"agent": "fe", "task": task}).to_string() }
                }]
            },
            "finish_reason": null
        }]
    });
    let finish = serde_json::json!({ "choices": [{ "delta": {}, "finish_reason": "tool_calls" }] });
    format!("data: {call}\n\ndata: {finish}\n\ndata: [DONE]\n\n")
}

async fn answer(body: &str) -> String {
    let sub_agent = body.contains("End every turn with this report");
    if sub_agent && body.contains("TASK-OK") {
        return says("DONE: built it\nCHANGED: none\nVERIFIED: not verified\nNEXT: nothing");
    }
    if sub_agent {
        // Never reports, even when asked with no tools.
        return says("Still going.");
    }
    if body.contains("[delegation to `fe` finished]") {
        return says("Reported.");
    }
    if body.contains("CASE-STOPS") {
        return delegate("TASK-STOPS: build it");
    }
    delegate("TASK-OK: build it")
}

async fn run(prompt: &str) -> (SessionStore, String, String) {
    let dir = std::env::temp_dir().join(format!("enx-clear-{}", uuid::Uuid::new_v4()));
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
    config.agent.background_delegation = false;
    let store = SessionStore::new(dir.with_extension("sessions"));
    let agent = Agent::with_discovery(
        config,
        store.clone(),
        Discovery {
            agents: builtin_agents(),
            ..Discovery::default()
        },
    );
    let (tx, mut rx) = tokio::sync::mpsc::channel(1024);
    let request = enowx_core::agent::RunRequest {
        prompt: prompt.into(),
        session_id: None,
        role: Role::Orchestrator,
        attachments: Vec::new(),
        agent: Some("orchestrator".into()),
    };
    let handle = tokio::spawn(async move {
        agent
            .run(request, tx, tokio_util::sync::CancellationToken::new())
            .await
    });
    let (mut session, mut branch) = (String::new(), String::new());
    while let Some(event) = tokio::time::timeout(std::time::Duration::from_secs(60), rx.recv())
        .await
        .expect("the run ends")
    {
        match event {
            enowx_core::Event::Session { id, .. } => session = id,
            enowx_core::Event::DelegationStarted { session_id, .. } => branch = session_id,
            _ => {}
        }
    }
    let _ = handle.await;
    (store, session, branch)
}

#[tokio::test]
async fn a_finished_delegation_leaves_its_report_and_clears_its_transcript() {
    let (store, session, branch) = run("build it").await;
    assert!(store.load(&branch).is_err(), "the transcript is cleared");
    let parent = store.load(&session).expect("the conversation stays");
    let reports: Vec<String> = parent
        .replay()
        .into_iter()
        .map(|m| m.content)
        .filter(|c| c.starts_with("[delegation to `fe` finished]"))
        .collect();
    assert_eq!(reports.len(), 1);
    assert!(reports[0].contains("DONE: built it"), "{}", reports[0]);
    assert!(
        !reports[0].contains("resume"),
        "nothing to resume: {}",
        reports[0]
    );
}

#[tokio::test]
async fn one_that_stopped_without_a_report_is_kept_to_resume() {
    let (store, session, branch) = run("CASE-STOPS build it").await;
    assert!(store.load(&branch).is_ok(), "kept, to be resumed");
    let parent = store.load(&session).unwrap();
    assert!(parent
        .replay()
        .iter()
        .any(|m| m.content.contains(&format!("session `{branch}`"))));
}
