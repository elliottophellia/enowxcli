//! DeepSeek's own API, checked on the wire.
//!
//! With thinking on (its default) and `tools` in the request, DeepSeek
//! requires every earlier turn's `reasoning_content` to be sent back, and
//! answers 400 otherwise. The agent always sends tools, so the second step of
//! every turn depends on it.

mod common;

use std::sync::{Arc, Mutex};

use enowx_core::{builtin_agents, config::Config, Agent, Discovery, Role, SessionStore};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Replies in order, one per request, and keeps each request body.
async fn scripted_provider(replies: Vec<String>) -> (String, Arc<Mutex<Vec<String>>>) {
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
                .or(replies.last())
                .cloned()
                .unwrap_or_default();
            index += 1;
            let seen = seen.clone();
            tokio::spawn(async move {
                let request = read_request(&mut socket).await;
                if let Some((_, body)) = request.split_once("\r\n\r\n") {
                    seen.lock().unwrap().push(body.to_owned());
                }
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
    (format!("http://127.0.0.1:{port}"), bodies)
}

async fn read_request(socket: &mut tokio::net::TcpStream) -> String {
    let mut data = Vec::new();
    let mut buf = [0u8; 16384];
    while let Ok(n) = socket.read(&mut buf).await {
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
        let text = String::from_utf8_lossy(&data);
        if let Some((head, body)) = text.split_once("\r\n\r\n") {
            let length = head
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            if body.len() >= length {
                break;
            }
        }
    }
    String::from_utf8_lossy(&data).into_owned()
}

/// A thinking step that ends in a tool call, as DeepSeek streams one.
fn thinks_then_globs() -> String {
    let thought = serde_json::json!({
        "choices": [{ "delta": { "reasoning_content": "FIRST-THOUGHT" }, "finish_reason": null }]
    });
    let call = serde_json::json!({
        "choices": [{
            "delta": { "tool_calls": [{
                "index": 0, "id": "call_1", "type": "function",
                "function": { "name": "glob", "arguments": "{\"pattern\":\"*\"}" }
            }]},
            "finish_reason": null
        }]
    });
    let finish = serde_json::json!({ "choices": [{ "delta": {}, "finish_reason": "tool_calls" }] });
    format!("data: {thought}\n\ndata: {call}\n\ndata: {finish}\n\ndata: [DONE]\n\n")
}

fn answers() -> String {
    let text = serde_json::json!({ "choices": [{ "delta": { "content": "done" }, "finish_reason": null }] });
    let finish = serde_json::json!({ "choices": [{ "delta": {}, "finish_reason": "stop" }] });
    format!("data: {text}\n\ndata: {finish}\n\ndata: [DONE]\n\n")
}

struct Dir(std::path::PathBuf);
impl Dir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "enx-deepseek-{}-{}-{tag}",
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

/// The assistant message of the second request, the one after the tool ran.
async fn second_request_assistant(preset: &str) -> serde_json::Value {
    let dir = Dir::new(preset);
    let (base_url, bodies) = scripted_provider(vec![thinks_then_globs(), answers()]).await;
    let mut config = Config::default();
    if preset == "deepseek" {
        // DeepSeek's own provider, pointed at the fixture server.
        config.provider.insert(
            "deepseek".into(),
            enowx_core::config::ProviderEntry {
                base_url,
                ..Default::default()
            },
        );
        config
            .auth
            .set_for_session("deepseek", "test-key", enowx_core::auth::KeySource::Session);
        assert!(config.use_model("deepseek/deepseek-flash"));
    } else {
        config.use_endpoint(preset, &base_url, "test-key", "deepseek-flash");
    }
    config.agent.workspace = Some(dir.0.clone());
    config.agent.max_steps = 4;
    config.agent.auto_compact = false;
    let discovery = Discovery {
        agents: builtin_agents(),
        ..Discovery::default()
    };
    let store = SessionStore::new(dir.0.join(".enx-sessions"));
    let agent = Agent::with_discovery(config, store, discovery);
    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    let request = enowx_core::agent::RunRequest {
        prompt: "list the files".into(),
        session_id: None,
        role: Role::Orchestrator,
        attachments: Vec::new(),
        agent: None,
    };
    let cancel = tokio_util::sync::CancellationToken::new();
    let handle = tokio::spawn(async move { agent.run(request, tx, cancel).await });
    while rx.recv().await.is_some() {}
    let _ = handle.await;

    let bodies = bodies.lock().unwrap().clone();
    assert!(
        bodies.len() >= 2,
        "expected two model calls, got {}",
        bodies.len()
    );
    let second: serde_json::Value = serde_json::from_str(&bodies[1]).expect("JSON");
    second["messages"]
        .as_array()
        .and_then(|messages| messages.iter().find(|m| m["role"] == "assistant"))
        .cloned()
        .expect("the first step's assistant message is sent back")
}

#[tokio::test]
async fn deepseek_gets_its_reasoning_back() {
    let assistant = second_request_assistant("deepseek").await;
    assert_eq!(
        assistant["reasoning_content"], "FIRST-THOUGHT",
        "DeepSeek answers 400 without it: {assistant}"
    );
    assert_eq!(assistant["tool_calls"][0]["function"]["name"], "glob");
}

/// Other OpenAI-style APIs can reject a field they do not know.
#[tokio::test]
async fn other_providers_do_not() {
    let assistant = second_request_assistant("custom").await;
    assert!(assistant.get("reasoning_content").is_none(), "{assistant}");
}
