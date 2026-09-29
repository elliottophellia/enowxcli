//! An interface agent about to finish is checked once for the marks of
//! generated work in the files it changed, and sent back to them when there
//! are findings.

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

async fn run(
    agent_name: &str,
    replies: Vec<String>,
    tag: &str,
) -> (Vec<Event>, Vec<String>, std::path::PathBuf, Dir) {
    let dir = Dir::new(tag);
    let (url, bodies) = provider(replies).await;
    let mut config = Config::default();
    config.use_endpoint("test", &url, "test-key", "test-model");
    config.agent.workspace = Some(dir.0.clone());
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
        prompt: "build the page".into(),
        session_id: None,
        role: Role::Orchestrator,
        attachments: Vec::new(),
        agent: Some(agent_name.into()),
    };
    let cancel = tokio_util::sync::CancellationToken::new();
    let handle = tokio::spawn(async move { agent.run(request, tx, cancel).await });
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    let _ = handle.await;
    let bodies = bodies.lock().unwrap().clone();
    let root = dir.0.clone();
    (events, bodies, root, dir)
}

fn writes(path: &str, content: &str) -> String {
    calls(
        "write",
        serde_json::json!({ "path": path, "content": content }),
    )
}

const SLOPPY: &str = "<main><h1>Welcome</h1><a href=\"#\">Get Started</a></main>\n";
const CLEAN: &str = "<main><h1>Fisioterapi di Bandung</h1><a href=\"https://wa.me/620\">Book a session</a></main>\n";

#[tokio::test]
async fn findings_send_the_agent_back_before_it_finishes() {
    let (events, bodies, _, _dir) = run(
        "fe",
        vec![writes("index.html", SLOPPY), says("DONE"), says("FIXED")],
        "sloppy",
    )
    .await;
    assert_eq!(bodies.len(), 3, "the write, the first report, the second");
    assert!(bodies[2].contains("[ui_check]") && bodies[2].contains("dead-link"));
    assert!(bodies[2].contains("generic-action"));
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Notice { message } if message.starts_with("ui_check on the files changed")
    )));
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::Text { delta } if delta.contains("FIXED"))));
}

#[tokio::test]
async fn clean_work_finishes_at_once() {
    let (_, bodies, _, _dir) = run(
        "fe",
        vec![writes("index.html", CLEAN), says("DONE"), says("NEVER")],
        "clean",
    )
    .await;
    assert_eq!(bodies.len(), 2);
}

/// The check runs once: a second report after the findings ends the turn,
/// fixed or not, rather than looping.
#[tokio::test]
async fn the_check_runs_once_per_turn() {
    let (_, bodies, _, _dir) = run(
        "fe",
        vec![
            writes("index.html", SLOPPY),
            says("DONE"),
            says("STILL"),
            says("NEVER"),
        ],
        "once",
    )
    .await;
    assert_eq!(bodies.len(), 3);
}

/// Agents that do not build interfaces are not checked.
#[tokio::test]
async fn other_agents_are_not_checked() {
    let (_, bodies, _, _dir) = run(
        "be",
        vec![writes("index.html", SLOPPY), says("DONE"), says("NEVER")],
        "be",
    )
    .await;
    assert_eq!(bodies.len(), 2);
}
