//! A delegated sub-agent always comes back with a report. One that keeps
//! narrating is sent back twice, then asked once with no tools, so all it
//! can do is write the report; one that still will not gets a report built
//! from what it did, in the same four fields.

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

/// `text`, ending the way `finish` says: `stop`, or `length` for a reply cut
/// off at the output limit.
fn says(text: &str, finish: &str) -> String {
    let delta = serde_json::json!({
        "choices": [{ "delta": { "content": text }, "finish_reason": null }]
    });
    let end = serde_json::json!({ "choices": [{ "delta": {}, "finish_reason": finish }] });
    format!("data: {delta}\n\ndata: {end}\n\ndata: [DONE]\n\n")
}

fn delegate(task: &str) -> String {
    let call = serde_json::json!({
        "choices": [{
            "delta": {
                "tool_calls": [{
                    "index": 0,
                    "id": "call_1",
                    "type": "function",
                    "function": { "name": "delegate", "arguments": serde_json::json!({"agent": "fe", "task": task}).to_string() }
                }]
            },
            "finish_reason": null
        }]
    });
    let finish = serde_json::json!({ "choices": [{ "delta": {}, "finish_reason": "tool_calls" }] });
    format!("data: {call}\n\ndata: {finish}\n\ndata: [DONE]\n\n")
}

/// The sub-agent's requests, with whether each offered tools.
type Seen = Arc<Mutex<Vec<bool>>>;

async fn answer(body: &str, seen: &Seen) -> String {
    let sub_agent = body.contains("End every turn with this report");
    if sub_agent {
        let tools = body.contains("\"tools\"");
        seen.lock().unwrap().push(tools);
        // Narrates, cut off at the length limit, until it has no tools left.
        if tools {
            return says("Now rewriting the layout, then the styles", "length");
        }
        if body.contains("STUBBORN") {
            return says("Still working on the layout.", "stop");
        }
        return says(
            "**DONE:** built the page\n**CHANGED:** page.html\n**VERIFIED:** not verified\n**NEXT:** nothing",
            "stop",
        );
    }
    if body.contains("[delegation to `fe`") {
        return says("Reported.", "stop");
    }
    if body.contains("STUBBORN") {
        return delegate("STUBBORN: build the page");
    }
    delegate("build the page")
}

async fn run(prompt: &str) -> (String, Vec<bool>) {
    let dir = std::env::temp_dir().join(format!("enx-report-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let seen: Seen = Arc::default();
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    {
        let seen = seen.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                if !common::is_model_call(&socket).await {
                    continue;
                }
                let seen = seen.clone();
                tokio::spawn(async move {
                    let body = request_body(&mut socket).await;
                    let reply = answer(&body, &seen).await;
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
    // In the turn, so the report comes back as a finished delegation.
    config.agent.background_delegation = false;
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
        prompt: prompt.into(),
        session_id: None,
        role: Role::Orchestrator,
        attachments: Vec::new(),
        agent: Some("orchestrator".into()),
    };
    let handle = tokio::spawn(async move {
        agent
            .run(request, tx, tokio_util::sync::CancellationToken::new())
            .await
    });
    let mut summary = String::new();
    while let Some(event) = tokio::time::timeout(std::time::Duration::from_secs(60), rx.recv())
        .await
        .expect("the run ends")
    {
        if let enowx_core::Event::DelegationFinished { summary: s, .. } = event {
            summary = s;
        }
    }
    let _ = handle.await;
    let _ = std::fs::remove_dir_all(&dir);
    let seen = seen.lock().unwrap().clone();
    (summary, seen)
}

/// Narration cut off at the length limit is sent back twice, then the report
/// is asked for with no tools offered, and the report it writes, in markdown,
/// is what the caller gets.
#[tokio::test]
async fn a_sub_agent_that_narrates_is_made_to_report() {
    let (summary, seen) = run("build the page").await;
    assert_eq!(
        seen,
        vec![true, true, true, false],
        "two nudges, then no tools"
    );
    assert_eq!(
        summary,
        "DONE: built the page\nCHANGED: page.html\nVERIFIED: not verified\nNEXT: nothing"
    );
}

/// One that will not report even then still hands its caller all four
/// fields, built from what it did.
#[tokio::test]
async fn a_sub_agent_that_never_reports_still_yields_a_report() {
    let (summary, seen) = run("STUBBORN build the page").await;
    assert_eq!(seen, vec![true, true, true, false]);
    assert!(summary.starts_with("NO REPORT"), "{summary}");
    for field in ["DONE:", "CHANGED: none", "VERIFIED: not verified", "NEXT:"] {
        assert!(summary.contains(field), "{field} in {summary}");
    }
}
