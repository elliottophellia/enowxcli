//! Calls in one step that only look run at the same time, and a picture
//! already in view is not sent to the model again.

use enowx_core::{builtin_agents, config::Config, Agent, Discovery, Event, Role, SessionStore};
use std::sync::{Arc, Mutex};
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

fn calls(list: &[(&str, serde_json::Value)]) -> String {
    let calls: Vec<_> = list
        .iter()
        .enumerate()
        .map(|(index, (name, arguments))| {
            serde_json::json!({
                "index": index,
                "id": format!("call_{index}"),
                "type": "function",
                "function": { "name": name, "arguments": arguments.to_string() }
            })
        })
        .collect();
    let call = serde_json::json!({
        "choices": [{ "delta": { "tool_calls": calls }, "finish_reason": null }]
    });
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
            let body = request_body(&mut socket).await;
            let reply = {
                let mut seen = seen.lock().unwrap();
                seen.push(body);
                replies
                    .get(seen.len() - 1)
                    .cloned()
                    .unwrap_or_else(|| says("DONE"))
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                reply.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.flush().await;
        }
    });
    (format!("http://127.0.0.1:{port}/v1"), bodies)
}

/// A 1x1 PNG.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0,
    0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99, 0x3D, 0x1D, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

#[tokio::test]
async fn looking_calls_run_together_and_a_picture_is_sent_once() {
    let dir = std::env::temp_dir().join(format!("enx-parallel-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("shot.png"), PNG).unwrap();
    std::fs::write(dir.join("note.txt"), "NOTE-TEXT\n").unwrap();
    let (url, bodies) = provider(vec![
        calls(&[
            ("read", serde_json::json!({"path": "shot.png"})),
            ("bash", serde_json::json!({"command": "echo BASH-RAN"})),
            ("read", serde_json::json!({"path": "note.txt"})),
        ]),
        calls(&[("read", serde_json::json!({"path": "shot.png"}))]),
        says("DONE: looked\nCHANGED: none\nVERIFIED: read\nNEXT: nothing"),
    ])
    .await;
    let mut config = Config::default();
    config.use_endpoint("test", &url, "test-key", "test-model");
    config.agent.workspace = Some(dir.clone());
    config.agent.auto_compact = false;
    config.agent.lsp = false;
    config.model.vision = true;
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
        prompt: "look at the screenshot and the note".into(),
        session_id: None,
        role: Role::Orchestrator,
        attachments: Vec::new(),
        agent: Some("be".into()),
    };
    let run = tokio::spawn(async move {
        agent
            .run(request, tx, tokio_util::sync::CancellationToken::new())
            .await
    });
    let mut order = Vec::new();
    let mut results = Vec::new();
    while let Some(event) = rx.recv().await {
        match event {
            Event::ToolCall { name, .. } => order.push(format!("call {name}")),
            Event::ToolResult { name, content, .. } => {
                order.push(format!("result {name}"));
                results.push(content);
            }
            _ => {}
        }
    }
    run.await.unwrap().unwrap();

    // All three started before any finished, and the results kept their order.
    assert_eq!(
        &order[..6],
        ["call read", "call bash", "call read", "result read", "result bash", "result read"],
        "{order:?}"
    );
    assert!(results[1].contains("BASH-RAN") && results[2].contains("NOTE-TEXT"), "{results:?}");

    let bodies = bodies.lock().unwrap().clone();
    assert!(bodies.len() >= 3, "{}", bodies.len());
    assert_eq!(bodies[1].matches("data:image/png").count(), 1, "the picture, once");
    assert!(
        bodies[2].contains("already in the conversation above"),
        "a second read is told to look back"
    );
    assert_eq!(bodies[2].matches("data:image/png").count(), 1, "and not sent again");
    let _ = std::fs::remove_dir_all(&dir);
}
