//! Agents working together (Settings > Team): a reviewer checks what a
//! delegate changed and sends it back with corrections until it passes, and
//! agents at work at the same time can message each other.

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

/// Tool calls, all in one step.
fn calls(list: &[(&str, serde_json::Value)]) -> String {
    let tool_calls: Vec<_> = list
        .iter()
        .enumerate()
        .map(|(i, (name, args))| {
            serde_json::json!({
                "index": i, "id": format!("call_{i}"), "type": "function",
                "function": { "name": name, "arguments": args.to_string() }
            })
        })
        .collect();
    let call = serde_json::json!({ "choices": [{ "delta": { "tool_calls": tool_calls }, "finish_reason": null }] });
    let finish = serde_json::json!({ "choices": [{ "delta": {}, "finish_reason": "tool_calls" }] });
    format!("data: {call}\n\ndata: {finish}\n\ndata: [DONE]\n\n")
}

const REPORT: &str = "End every turn with this report";

type Script = Arc<dyn Fn(&str) -> (String, u64) + Send + Sync>;

/// Run `prompt` with Team on, the provider answering by `script` (a reply
/// and a delay in ms). Every DelegationFinished summary, by agent.
async fn run(prompt: &str, script: Script) -> Vec<(String, String)> {
    let dir = std::env::temp_dir().join(format!("enx-team-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            if !common::is_model_call(&socket).await {
                continue;
            }
            let script = script.clone();
            tokio::spawn(async move {
                let body = request_body(&mut socket).await;
                let (reply, delay) = script(&body);
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
    config.agent.comms.enabled = true;
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
    let mut finished = Vec::new();
    while let Some(event) = tokio::time::timeout(std::time::Duration::from_secs(60), rx.recv())
        .await
        .expect("the run ends")
    {
        if let enowx_core::Event::DelegationFinished { agent, summary, .. } = event {
            finished.push((agent, summary));
        }
    }
    let _ = handle.await;
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(dir.with_extension("sessions"));
    finished
}

/// The reviewer asks for a fix, the delegate makes it, the reviewer passes
/// it, and the caller's report says so.
#[tokio::test]
async fn a_review_sends_the_work_back_until_it_passes() {
    let reviews = Arc::new(Mutex::new(0u32));
    let counted = reviews.clone();
    let same_session = Arc::new(Mutex::new(false));
    let same = same_session.clone();
    let script: Script = Arc::new(move |body: &str| {
        if body.contains("Review the work `fe` just did") {
            *counted.lock().unwrap() += 1;
            // The second round is in the same session: it carries the first.
            if body.contains("made its fixes") {
                *same.lock().unwrap() = true;
            }
            return if body.contains("fixed the title") {
                (says("DONE: VERDICT: PASS\nCHANGED: none\nVERIFIED: read page.html\nNEXT: nothing"), 0)
            } else {
                (says("DONE: VERDICT: FIX\n1. page.html:1 the title says Shp, it should say Shop\nCHANGED: none\nVERIFIED: read page.html\nNEXT: nothing"), 0)
            };
        }
        if body.contains(REPORT) {
            if body.contains("asks for these corrections") {
                return (says("DONE: fixed the title\nCHANGED: page.html\nVERIFIED: read it back\nNEXT: nothing"), 0);
            }
            if body.contains("\"role\":\"tool\"") {
                return (says("DONE: built the page\nCHANGED: page.html\nVERIFIED: not verified\nNEXT: nothing"), 0);
            }
            return (
                calls(&[(
                    "write",
                    serde_json::json!({"path": "page.html", "content": "<h1>Shp</h1>\n"}),
                )]),
                0,
            );
        }
        if body.contains("[delegation to `fe`") {
            return (says("Reported."), 0);
        }
        (
            calls(&[(
                "delegate",
                serde_json::json!({"agent": "fe", "task": "build the page"}),
            )]),
            0,
        )
    });
    let finished = run("build the page", script).await;
    let fe = finished
        .iter()
        .find(|(a, _)| a == "fe")
        .expect("fe reported");
    assert!(
        fe.1.contains("CROSS-REVIEW by review: PASS after 1 correction round(s)"),
        "{}",
        fe.1
    );
    assert!(
        fe.1.contains("fixed the title"),
        "the report after the fix: {}",
        fe.1
    );
    assert_eq!(*reviews.lock().unwrap(), 2, "checked, then checked again");
    assert!(
        *same_session.lock().unwrap(),
        "the reviewer checked again in the session it reviewed in"
    );
}

/// Two agents at work at once: one messages the other, which reads it at
/// its next step.
#[tokio::test]
async fn an_agent_reads_a_message_from_another_at_its_next_step() {
    let heard = Arc::new(Mutex::new(false));
    let noted = heard.clone();
    let script: Script = Arc::new(move |body: &str| {
        if body.contains(REPORT) && body.contains("build the api") {
            if body.contains("[message from fe") {
                *noted.lock().unwrap() = true;
                return (
                    says("DONE: api built\nCHANGED: none\nVERIFIED: not verified\nNEXT: nothing"),
                    0,
                );
            }
            if body.contains("\"role\":\"tool\"") {
                // Still at work: one more step, after fe's message is in.
                return (
                    calls(&[("glob", serde_json::json!({"pattern": "*.rs"}))]),
                    400,
                );
            }
            return (
                calls(&[("glob", serde_json::json!({"pattern": "*.md"}))]),
                400,
            );
        }
        if body.contains(REPORT) && body.contains("build the page") {
            if body.contains("\"role\":\"tool\"") {
                return (
                    says("DONE: page built\nCHANGED: none\nVERIFIED: not verified\nNEXT: nothing"),
                    0,
                );
            }
            return (
                calls(&[(
                    "message_agent",
                    serde_json::json!({"to": "be", "message": "orders need a total field"}),
                )]),
                0,
            );
        }
        if body.contains("[delegation to `") {
            return (says("Reported."), 0);
        }
        (
            calls(&[
                (
                    "delegate",
                    serde_json::json!({"agent": "fe", "task": "build the page"}),
                ),
                (
                    "delegate",
                    serde_json::json!({"agent": "be", "task": "build the api"}),
                ),
            ]),
            0,
        )
    });
    let finished = run("build it", script).await;
    assert!(finished.iter().any(|(a, _)| a == "be"), "{finished:?}");
    assert!(*heard.lock().unwrap(), "be read fe's message");
}
