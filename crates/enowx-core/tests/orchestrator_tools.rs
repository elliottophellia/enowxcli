//! The orchestrator looks and checks with `bash`, and an agent that does not
//! edit is refused the commands that would change something. The model is a
//! stand-in, so nothing is spent.

use std::sync::{Arc, Mutex};

use enowx_core::{builtin_agents, config::Config, Agent, Discovery, Role, SessionStore};
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

async fn run(agent_name: &str, replies: Vec<String>) -> (Vec<String>, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("enx-orch-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("seeded-file.txt"), "here").unwrap();
    let (url, bodies) = provider(replies).await;
    let mut config = Config::default();
    config.provider.name = "test".into();
    config.provider.base_url = url;
    config.provider.api_key = "test-key".into();
    config.model.default = "test-model".into();
    config.agent.workspace = Some(dir.clone());
    config.agent.auto_compact = false;
    let discovery = Discovery {
        agents: builtin_agents(),
        ..Discovery::default()
    };
    let agent = Agent::with_discovery(
        config,
        SessionStore::new(dir.with_extension("sessions")),
        discovery,
    );
    let (tx, mut rx) = tokio::sync::mpsc::channel(1024);
    let request = enowx_core::agent::RunRequest {
        prompt: "look around".into(),
        session_id: None,
        role: Role::Orchestrator,
        attachments: Vec::new(),
        agent: Some(agent_name.into()),
    };
    let cancel = tokio_util::sync::CancellationToken::new();
    let handle = tokio::spawn(async move { agent.run(request, tx, cancel).await });
    while rx.recv().await.is_some() {}
    let _ = handle.await;
    let bodies = bodies.lock().unwrap().clone();
    (bodies, dir)
}

fn bash(command: &str) -> String {
    calls("bash", serde_json::json!({ "command": command }))
}

#[test]
fn the_orchestrator_looks_and_checks_but_has_no_tool_that_edits() {
    let roster = builtin_agents();
    let orchestrator = roster.iter().find(|a| a.name == "orchestrator").unwrap();
    for tool in ["read", "glob", "grep", "bash", "fetch", "todo"] {
        assert!(orchestrator.tools.iter().any(|t| t == tool), "{tool}");
    }
    for tool in ["write", "edit", "multi_edit"] {
        assert!(!orchestrator.tools.iter().any(|t| t == tool), "{tool}");
    }
}

#[tokio::test]
async fn a_change_through_bash_is_refused_and_a_look_runs() {
    let (bodies, dir) = run(
        "orchestrator",
        vec![bash("echo hi > notes.txt"), bash("ls"), says("Looked.")],
    )
    .await;
    assert_eq!(bodies.len(), 3);
    assert!(
        bodies[1].contains("Not run: this command writes to `notes.txt` with a redirect"),
        "{}",
        bodies[1]
    );
    assert!(
        !dir.join("notes.txt").exists(),
        "the refused command never ran"
    );
    assert!(
        bodies[2].contains("seeded-file.txt"),
        "ls ran: {}",
        bodies[2]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn review_is_held_to_looking_too() {
    let (bodies, dir) = run("review", vec![bash("git stash"), says("Reviewed.")]).await;
    assert!(
        bodies[1].contains("Not run: this command changes the repository with `git stash`"),
        "{}",
        bodies[1]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn an_agent_that_edits_is_not_held_back() {
    let (bodies, dir) = run("fe", vec![bash("echo hi > notes.txt"), says("Wrote it.")]).await;
    assert!(!bodies[1].contains("Not run"), "{}", bodies[1]);
    assert_eq!(
        std::fs::read_to_string(dir.join("notes.txt"))
            .unwrap()
            .trim(),
        "hi"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
