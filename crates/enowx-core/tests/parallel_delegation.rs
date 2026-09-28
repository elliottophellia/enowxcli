//! Delegations asked for in one step run at the same time, several to the
//! same specialist, and keep the contract between them: a file one is
//! editing is refused to the others until it finishes. The model is a
//! stand-in that answers by what each request is.

use std::sync::{Arc, Mutex};
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

fn two_delegations() -> String {
    let call = |index: usize, id: &str, task: &str| {
        serde_json::json!({
            "index": index,
            "id": id,
            "type": "function",
            "function": {
                "name": "delegate",
                "arguments": serde_json::json!({"agent": "fe", "task": task}).to_string()
            }
        })
    };
    let chunk = serde_json::json!({
        "choices": [{
            "delta": {"tool_calls": [call(0, "d1", "TASK-A: write shared.txt"), call(1, "d2", "TASK-B: write b.txt")]},
            "finish_reason": null
        }]
    });
    let finish = serde_json::json!({ "choices": [{ "delta": {}, "finish_reason": "tool_calls" }] });
    format!("data: {chunk}\n\ndata: {finish}\n\ndata: [DONE]\n\n")
}

fn write(path: &str, content: &str) -> String {
    calls(
        "write",
        serde_json::json!({"path": path, "content": content}),
    )
}

/// What the stand-in model answers, by what the request is.
async fn answer(body: &str) -> String {
    let sub_agent = body.contains("End every turn with this report");
    if sub_agent && body.contains("TASK-A") {
        if body.contains("Created shared.txt") {
            // Holds the file a while, so B runs into the claim.
            tokio::time::sleep(Duration::from_millis(1500)).await;
            return says("DONE: A\nCHANGED: shared.txt\nVERIFIED: not verified\nNEXT: nothing");
        }
        return write("shared.txt", "from A\n");
    }
    if sub_agent && body.contains("TASK-B") {
        if body.contains("Created b.txt") {
            return says("DONE: B\nCHANGED: b.txt\nVERIFIED: not verified\nNEXT: nothing");
        }
        if body.contains("is being edited by") {
            return write("b.txt", "from B\n");
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
        return write("shared.txt", "from B\n");
    }
    if body.contains("DONE: A") && body.contains("DONE: B") {
        return says("Both parts are built.");
    }
    two_delegations()
}

async fn provider() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let seen = bodies.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let seen = seen.clone();
            tokio::spawn(async move {
                let body = request_body(&mut socket).await;
                seen.lock().unwrap().push(body.clone());
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
    (format!("http://127.0.0.1:{port}/v1"), bodies)
}

#[tokio::test]
async fn one_step_delegates_run_together_and_keep_to_the_contract() {
    let dir = std::env::temp_dir().join(format!("enx-parallel-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let (url, bodies) = provider().await;
    let mut config = Config::default();
    config.use_endpoint("test", &url, "test-key", "test-model");
    config.agent.workspace = Some(dir.clone());
    config.agent.auto_compact = false;
    let agent = Agent::with_discovery(
        config,
        SessionStore::new(dir.with_extension("sessions")),
        Discovery {
            agents: builtin_agents(),
            ..Discovery::default()
        },
    );
    let (tx, mut rx) = tokio::sync::mpsc::channel(1024);
    let request = enowx_core::agent::RunRequest {
        prompt: "build both parts".into(),
        session_id: None,
        role: Role::Orchestrator,
        attachments: Vec::new(),
        agent: Some("orchestrator".into()),
    };
    let started = std::time::Instant::now();
    let handle = tokio::spawn(async move {
        agent
            .run(request, tx, tokio_util::sync::CancellationToken::new())
            .await
    });
    let mut delegations = 0;
    while let Some(event) = rx.recv().await {
        if matches!(event, enowx_core::Event::DelegationStarted { .. }) {
            delegations += 1;
        }
    }
    handle.await.unwrap().unwrap();

    assert_eq!(delegations, 2, "both delegations of the step ran");
    let bodies = bodies.lock().unwrap().clone();
    assert!(
        bodies
            .iter()
            .any(|b| b.contains("is being edited by Frontend (fe)")),
        "B was refused the file A was editing, so they ran at the same time"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("shared.txt")).unwrap(),
        "from A\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("b.txt")).unwrap(),
        "from B\n"
    );
    let last = bodies.last().unwrap();
    assert!(
        last.contains("DONE: A") && last.contains("DONE: B"),
        "both reports came back"
    );
    assert!(started.elapsed() < Duration::from_secs(20));
    let _ = std::fs::remove_dir_all(&dir);
}
