//! Each agent can run on a model from any connected provider: the model is
//! named with its provider, and the call goes to that provider with its own
//! key. An agent whose model is on a provider with no key runs on the model
//! in use, and says so.

mod common;

use std::sync::{Arc, Mutex};

use enowx_core::{builtin_agents, config::Config, Agent, Discovery, Event, Role, SessionStore};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Answers every request with `text`, keeping each request's body and
/// `Authorization` header.
async fn provider(text: &'static str) -> (String, Arc<Mutex<Vec<(String, String)>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            if !common::is_model_call(&socket).await {
                continue;
            }
            let log = log.clone();
            tokio::spawn(async move {
                let (head, body) = read_request(&mut socket).await;
                let auth = head
                    .lines()
                    .find_map(|line| line.strip_prefix("authorization:"))
                    .unwrap_or("")
                    .trim()
                    .to_owned();
                log.lock().unwrap().push((body, auth));
                let delta = serde_json::json!({
                    "choices": [{ "delta": { "content": text }, "finish_reason": null }]
                });
                let finish =
                    serde_json::json!({ "choices": [{ "delta": {}, "finish_reason": "stop" }] });
                let reply = format!("data: {delta}\n\ndata: {finish}\n\ndata: [DONE]\n\n");
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
    (format!("http://127.0.0.1:{port}/v1"), seen)
}

/// The request's header block, lowercased, and its body.
async fn read_request(socket: &mut tokio::net::TcpStream) -> (String, String) {
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
            let body = String::from_utf8_lossy(&data[end + 4..end + 4 + length]).into_owned();
            return (head, body);
        }
    }
    (String::new(), String::from_utf8_lossy(&data).into_owned())
}

struct Dir(std::path::PathBuf);
impl Dir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "enx-agent-providers-{}-{tag}-{}",
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

/// Run one message to `agent` and return the events.
async fn ask(config: Config, dir: &Dir, agent: &str) -> Vec<Event> {
    let discovery = Discovery {
        agents: builtin_agents(),
        ..Discovery::default()
    };
    let agent_runner = Agent::with_discovery(
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
        agent: Some(agent.into()),
    };
    let cancel = tokio_util::sync::CancellationToken::new();
    let handle = tokio::spawn(async move { agent_runner.run(request, tx, cancel).await });
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    let _ = handle.await;
    events
}

fn base_config(dir: &Dir) -> Config {
    let mut config = Config::default();
    config.agent.workspace = Some(dir.0.clone());
    config.agent.auto_compact = false;
    config
}

#[tokio::test]
async fn an_agent_runs_on_its_own_providers_model() {
    let dir = Dir::new("own");
    let (main_url, main_seen) = provider("FROM-MAIN").await;
    let (design_url, design_seen) = provider("FROM-DESIGN").await;
    let mut config = base_config(&dir);
    config.use_endpoint("design", &design_url, "design-key", "unused");
    config.use_endpoint("main", &main_url, "main-key", "main-model");
    config
        .agent
        .models
        .insert("fe".into(), "design/vendor/design-model".into());

    let events = ask(config, &dir, "fe").await;

    let design = design_seen.lock().unwrap().clone();
    assert_eq!(design.len(), 1, "fe's call goes to its own provider");
    let body: serde_json::Value = serde_json::from_str(&design[0].0).unwrap();
    assert_eq!(
        body["model"], "vendor/design-model",
        "the id the provider knows"
    );
    assert_eq!(design[0].1, "bearer design-key", "with that provider's key");
    assert!(main_seen.lock().unwrap().is_empty(), "nothing reaches main");
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::Text { delta } if delta.contains("FROM-DESIGN"))));
}

#[tokio::test]
async fn an_agent_on_a_provider_without_a_key_stays_on_the_model_in_use() {
    std::env::remove_var("OPENROUTER_API_KEY");
    let dir = Dir::new("no-key");
    let (main_url, main_seen) = provider("FROM-MAIN").await;
    let mut config = base_config(&dir);
    config.use_endpoint("main", &main_url, "main-key", "main-model");
    config
        .agent
        .models
        .insert("fe".into(), "openrouter/vendor/design-model".into());

    let events = ask(config, &dir, "fe").await;

    let main = main_seen.lock().unwrap().clone();
    assert_eq!(main.len(), 1, "the turn still runs");
    let body: serde_json::Value = serde_json::from_str(&main[0].0).unwrap();
    assert_eq!(body["model"], "main-model");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Notice { message } if message.contains("OpenRouter is not connected")
        )),
        "the user is told why: {events:?}"
    );
}

#[tokio::test]
async fn the_chosen_thinking_effort_goes_with_each_call() {
    let dir = Dir::new("effort");
    let (url, seen) = provider("OK").await;
    let mut config = base_config(&dir);
    config.use_endpoint("main", &url, "main-key", "main-model");
    config.model.efforts = vec!["low".into(), "high".into()];
    config.model.effort = "high".into();

    ask(config, &dir, "general").await;

    let calls = seen.lock().unwrap().clone();
    let body: serde_json::Value = serde_json::from_str(&calls[0].0).unwrap();
    assert_eq!(body["reasoning_effort"], "high");
}

#[tokio::test]
async fn no_effort_is_sent_when_none_was_chosen() {
    let dir = Dir::new("no-effort");
    let (url, seen) = provider("OK").await;
    let mut config = base_config(&dir);
    config.use_endpoint("main", &url, "main-key", "main-model");
    config.model.effort.clear();

    ask(config, &dir, "general").await;

    let calls = seen.lock().unwrap().clone();
    let body: serde_json::Value = serde_json::from_str(&calls[0].0).unwrap();
    assert!(body.get("reasoning_effort").is_none(), "{body}");
    assert!(body.get("reasoning").is_none(), "{body}");
}
