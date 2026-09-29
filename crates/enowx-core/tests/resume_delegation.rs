//! A delegation that stopped short is continued in its own session, with
//! everything it already did, instead of being started over.

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

fn resume(id: &str) -> String {
    calls(
        "delegate",
        serde_json::json!({"agent": "fe", "resume": id, "task": "CONTINUE-R: finish what is left"}),
    )
}

async fn answer(body: &str) -> String {
    let sub_agent = body.contains("End every turn with this report");
    if sub_agent {
        if body.contains("CONTINUE-R") && body.contains("TASK-R: start the page") {
            return says("DONE: finished with what I had\nCHANGED: page.html\nVERIFIED: not verified\nNEXT: nothing");
        }
        return says("I stopped halfway through.");
    }
    if body.contains("finished with what I had") {
        return says("All done.");
    }
    if let Some(at) = body.find("(session `") {
        let rest = &body[at + "(session `".len()..];
        let id: String = rest.chars().take_while(|c| *c != '`').collect();
        return resume(&id);
    }
    calls(
        "delegate",
        serde_json::json!({"agent": "fe", "task": "TASK-R: start the page"}),
    )
}

async fn provider() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
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
    format!("http://127.0.0.1:{port}/v1")
}

#[tokio::test]
async fn a_part_that_stopped_short_is_continued_in_its_own_session() {
    let dir = std::env::temp_dir().join(format!("enx-resume-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let url = provider().await;
    let mut config = Config::default();
    config.use_endpoint("test", &url, "test-key", "test-model");
    config.agent.workspace = Some(dir.clone());
    config.agent.auto_compact = false;
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
        prompt: "build the page".into(),
        session_id: None,
        role: Role::Orchestrator,
        attachments: Vec::new(),
        agent: Some("orchestrator".into()),
    };
    let run = tokio::spawn(async move {
        agent
            .run(request, tx, tokio_util::sync::CancellationToken::new())
            .await
    });
    let (mut session, mut started, mut said) = (String::new(), Vec::new(), String::new());
    while let Some(event) = rx.recv().await {
        match event {
            enowx_core::Event::Session { id, .. } => session = id,
            enowx_core::Event::DelegationStarted { session_id, .. } => started.push(session_id),
            enowx_core::Event::Text { delta } => said.push_str(&delta),
            _ => {}
        }
    }
    run.await.unwrap().unwrap();
    assert_eq!(started.len(), 2, "started, then continued");
    assert_eq!(started[0], started[1], "continued in the same session");
    assert_eq!(store.branches_of(&session).len(), 1, "no second branch");
    assert!(said.contains("All done"), "{said}");
    let _ = std::fs::remove_dir_all(&dir);
}
