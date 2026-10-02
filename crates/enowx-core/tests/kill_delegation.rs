//! The user stops a sub-agent at work: its caller gets a report saying so,
//! with what it did, and can read its transcript with `delegation_log`
//! before briefing the next one.

mod common;

use enowx_core::{builtin_agents, config::Config, Agent, Discovery, Role, SessionStore};
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
    let delta =
        serde_json::json!({ "choices": [{ "delta": { "content": text }, "finish_reason": null }] });
    let end = serde_json::json!({ "choices": [{ "delta": {}, "finish_reason": "stop" }] });
    format!("data: {delta}\n\ndata: {end}\n\ndata: [DONE]\n\n")
}

fn call(name: &str, args: serde_json::Value) -> String {
    let call = serde_json::json!({ "choices": [{ "delta": { "tool_calls": [{
        "index": 0, "id": "call_0", "type": "function",
        "function": { "name": name, "arguments": args.to_string() } }] }, "finish_reason": null }] });
    let finish = serde_json::json!({ "choices": [{ "delta": {}, "finish_reason": "tool_calls" }] });
    format!("data: {call}\n\ndata: {finish}\n\ndata: [DONE]\n\n")
}

#[tokio::test]
async fn a_stopped_sub_agent_reports_and_its_transcript_can_be_read() {
    let dir = std::env::temp_dir().join(format!("enx-kill-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let read_log = Arc::new(Mutex::new(false));
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    {
        let read_log = read_log.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                if !common::is_model_call(&socket).await {
                    continue;
                }
                let read_log = read_log.clone();
                tokio::spawn(async move {
                    let body = request_body(&mut socket).await;
                    let (reply, delay) = if body.contains("End every turn with this report") {
                        // The sub-agent: one look, then slow steps until stopped.
                        if body.contains("\"role\":\"tool\"") {
                            (call("glob", serde_json::json!({"pattern": "*.md"})), 1500)
                        } else {
                            (
                                says("Starting on the checkout page now.")
                                    .replace("data: [DONE]", "")
                                    .replace(
                                        "\"finish_reason\":\"stop\"",
                                        "\"finish_reason\":null",
                                    )
                                    + &call("glob", serde_json::json!({"pattern": "*.html"})),
                                0,
                            )
                        }
                    } else if body.contains("Transcript of `fe`") {
                        *read_log.lock().unwrap() = true;
                        (
                            says(
                                "It had only looked around; I will brief the next one from there.",
                            ),
                            0,
                        )
                    } else if body.contains("KILLED by the user") {
                        let id = body
                            .split("session `")
                            .nth(1)
                            .and_then(|rest| rest.split('`').next())
                            .unwrap_or("")
                            .to_owned();
                        (
                            call("delegation_log", serde_json::json!({"session": id})),
                            0,
                        )
                    } else {
                        (
                            call(
                                "delegate",
                                serde_json::json!({"agent": "fe", "task": "build the checkout page"}),
                            ),
                            0,
                        )
                    };
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
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
    }
    let mut config = Config::default();
    config.use_endpoint(
        "test",
        &format!("http://127.0.0.1:{port}/v1"),
        "test-key",
        "test-model",
    );
    config.agent.workspace = Some(dir.clone());
    config.agent.auto_compact = false;
    config.agent.lsp = false;
    config.agent.background_delegation = false;
    let agent = Arc::new(Agent::with_discovery(
        config,
        SessionStore::new(dir.with_extension("sessions")),
        Discovery {
            agents: builtin_agents(),
            ..Discovery::default()
        },
    ));
    let (tx, mut rx) = tokio::sync::mpsc::channel(1024);
    let request = enowx_core::agent::RunRequest {
        prompt: "build the checkout".into(),
        session_id: None,
        role: Role::Orchestrator,
        attachments: Vec::new(),
        agent: Some("orchestrator".into()),
    };
    let runner = agent.clone();
    let handle = tokio::spawn(async move {
        runner
            .run(request, tx, tokio_util::sync::CancellationToken::new())
            .await
    });
    let mut report = String::new();
    while let Some(event) = tokio::time::timeout(std::time::Duration::from_secs(60), rx.recv())
        .await
        .expect("the run ends")
    {
        match event {
            enowx_core::Event::DelegationStarted { session_id, .. } => {
                // Let it take a step, then stop it as the user would.
                let agent = agent.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    assert!(agent.kill_delegation(&session_id), "it was at work");
                });
            }
            enowx_core::Event::DelegationFinished {
                summary, failed, ..
            } => {
                assert!(failed, "a stopped delegation is not a finished one");
                report = summary;
            }
            _ => {}
        }
    }
    let _ = handle.await;
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(dir.with_extension("sessions"));
    assert!(report.starts_with("KILLED by the user"), "{report}");
    assert!(
        report.contains("Starting on the checkout page"),
        "what it said last: {report}"
    );
    assert!(
        *read_log.lock().unwrap(),
        "the orchestrator read the transcript"
    );
}
