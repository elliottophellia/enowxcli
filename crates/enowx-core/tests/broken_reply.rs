//! A reply the provider breaks off (`finish_reason: "error"`, which a gateway
//! sends when its upstream went quiet) is asked for again; one that keeps
//! breaking reaches the agent with what to do instead of resending it.

mod common;

use enowx_core::{builtin_agents, config::Config, Agent, Discovery, Event, Role, SessionStore};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn says(text: &str) -> String {
    let delta =
        serde_json::json!({ "choices": [{ "delta": { "content": text }, "finish_reason": null }] });
    let finish = serde_json::json!({ "choices": [{ "delta": {}, "finish_reason": "stop" }] });
    format!("data: {delta}\n\ndata: {finish}\n\ndata: [DONE]\n\n")
}

/// A write whose arguments stop after the path, then the provider breaks off.
fn broken_write() -> String {
    let call = serde_json::json!({ "choices": [{ "delta": { "tool_calls": [{
        "index": 0, "id": "call_w", "type": "function",
        "function": { "name": "write", "arguments": "{\"path\":\"page.html\"" }
    }] }, "finish_reason": null }] });
    let finish = serde_json::json!({ "choices": [{ "delta": {}, "finish_reason": "error" }] });
    format!("data: {call}\n\ndata: {finish}\n\ndata: [DONE]\n\n")
}

fn whole_write() -> String {
    let call = serde_json::json!({ "choices": [{ "delta": { "tool_calls": [{
        "index": 0, "id": "call_w", "type": "function",
        "function": { "name": "write", "arguments": "{\"path\":\"page.html\",\"content\":\"<p>ok</p>\\n\"}" }
    }] }, "finish_reason": null }] });
    let finish = serde_json::json!({ "choices": [{ "delta": {}, "finish_reason": "tool_calls" }] });
    format!("data: {call}\n\ndata: {finish}\n\ndata: [DONE]\n\n")
}

async fn provider(replies: Vec<String>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let seen = bodies.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            if !common::is_model_call(&socket).await {
                continue;
            }
            let mut data = Vec::new();
            let mut buf = [0u8; 65536];
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
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .and_then(|v| v.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                if data.len() >= end + 4 + length {
                    break;
                }
            }
            let reply = {
                let mut seen = seen.lock().unwrap();
                seen.push(String::from_utf8_lossy(&data).into_owned());
                replies
                    .get(seen.len() - 1)
                    .cloned()
                    .unwrap_or_else(|| says("DONE"))
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                reply.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
        }
    });
    (format!("http://127.0.0.1:{port}/v1"), bodies)
}

async fn run(replies: Vec<String>, tag: &str) -> (Vec<Event>, usize, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("enx-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let (url, bodies) = provider(replies).await;
    let mut config = Config::default();
    config.use_endpoint("test", &url, "test-key", "test-model");
    config.agent.workspace = Some(dir.clone());
    config.agent.auto_compact = false;
    config.agent.lsp = false;
    let agent = Agent::with_discovery(
        config,
        SessionStore::new(dir.join(".enx-sessions")),
        Discovery {
            agents: builtin_agents(),
            ..Discovery::default()
        },
    );
    let (tx, mut rx) = tokio::sync::mpsc::channel(1024);
    let request = enowx_core::agent::RunRequest {
        prompt: "write the page".into(),
        session_id: None,
        role: Role::Orchestrator,
        attachments: Vec::new(),
        agent: Some("be".into()),
    };
    let handle = tokio::spawn(async move {
        agent
            .run(request, tx, tokio_util::sync::CancellationToken::new())
            .await
    });
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    let _ = handle.await;
    let calls = bodies.lock().unwrap().len();
    (events, calls, dir)
}

#[tokio::test]
async fn a_broken_off_reply_is_asked_for_again() {
    let (_, calls, dir) = run(
        vec![broken_write(), whole_write(), says("DONE")],
        "broken-once",
    )
    .await;
    assert_eq!(calls, 3, "the break, the retry, the report");
    assert_eq!(
        std::fs::read_to_string(dir.join("page.html")).unwrap(),
        "<p>ok</p>\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn one_that_keeps_breaking_tells_the_agent_to_send_less() {
    let replies = vec![broken_write(), broken_write(), broken_write(), says("DONE")];
    let (events, calls, dir) = run(replies, "broken-always").await;
    assert_eq!(calls, 4, "three tries, then the agent's next step");
    let refused = events
        .iter()
        .find_map(|e| match e {
            Event::ToolResult {
                content,
                is_error: true,
                ..
            } => Some(content.clone()),
            _ => None,
        })
        .expect("the write was refused");
    assert!(
        refused.contains("broke off") && refused.contains("one `write` per step"),
        "{refused}"
    );
    assert!(!dir.join("page.html").exists());
    let _ = std::fs::remove_dir_all(&dir);
}
