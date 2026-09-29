//! A turn runs until the work is done: no cap on model calls by default.
//! What stops a runaway turn is the same calls three steps in a row.

mod common;

use std::sync::{Arc, Mutex};

use enowx_core::{builtin_agents, config::Config, Agent, Discovery, Event, Role, SessionStore};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Replies in order, one per request, keeping every request body.
async fn provider(replies: Vec<String>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let seen = bodies.clone();
    tokio::spawn(async move {
        let mut index = 0usize;
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            if !common::is_model_call(&socket).await {
                continue;
            }
            let reply = replies
                .get(index)
                .cloned()
                .unwrap_or_else(|| replies.last().cloned().unwrap_or_default());
            index += 1;
            let seen = seen.clone();
            tokio::spawn(async move {
                let body = request_body(&mut socket).await;
                seen.lock().unwrap().push(body);
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

struct Dir(std::path::PathBuf);
impl Dir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "enx-ask-{}-{tag}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn run(replies: Vec<String>, max_steps: u32, tag: &str) -> (Vec<Event>, usize) {
    let dir = Dir::new(tag);
    let (url, bodies) = provider(replies).await;
    let mut config = Config::default();
    config.use_endpoint("test", &url, "test-key", "test-model");
    config.agent.workspace = Some(dir.0.clone());
    config.agent.max_steps = max_steps;
    config.agent.auto_compact = false;
    let discovery = Discovery {
        agents: builtin_agents(),
        ..Discovery::default()
    };
    let agent = Agent::with_discovery(
        config,
        SessionStore::new(dir.0.join(".enx-sessions")),
        discovery,
    );
    let (tx, mut rx) = tokio::sync::mpsc::channel(1024);
    let request = enowx_core::agent::RunRequest {
        prompt: "look around".into(),
        session_id: None,
        role: Role::Orchestrator,
        attachments: Vec::new(),
        agent: None,
    };
    let cancel = tokio_util::sync::CancellationToken::new();
    let handle = tokio::spawn(async move { agent.run(request, tx, cancel).await });
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    let _ = handle.await;
    let calls = bodies.lock().unwrap().len();
    (events, calls)
}

fn glob(pattern: &str) -> String {
    calls("glob", serde_json::json!({ "pattern": pattern }))
}

/// Forty different steps, past the old cap of 32, and the turn finishes.
#[tokio::test]
async fn a_long_turn_runs_to_the_end() {
    let mut replies: Vec<String> = (0..40).map(|i| glob(&format!("*.{i}"))).collect();
    replies.push(says("ALL-DONE"));
    let (events, calls) = run(replies, 0, "long").await;
    assert_eq!(calls, 41);
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::Text { delta } if delta.contains("ALL-DONE"))));
    assert!(!events
        .iter()
        .any(|e| matches!(e, Event::Notice { message } if message.contains("Stopped"))));
}

/// The same call again and again is stuck, not working: stopped at three.
#[tokio::test]
async fn the_same_call_three_times_running_stops_the_turn() {
    let (events, calls) = run(vec![glob("*.rs")], 0, "stuck").await;
    assert_eq!(calls, 3);
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Notice { message } if message.contains("same call 3 times")
    )));
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::Done { stop_reason } if stop_reason == "repeating")));
}

/// A cap set in the config still holds.
#[tokio::test]
async fn a_configured_cap_still_holds() {
    let replies: Vec<String> = (0..20).map(|i| glob(&format!("*.{i}"))).collect();
    let (events, calls) = run(replies, 5, "cap").await;
    assert_eq!(calls, 5);
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::Notice { message } if message.contains("Stopped after 5"))));
}
