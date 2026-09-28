//! The agent holding the conversation asks the user a question and waits:
//! the answer comes back as the tool's result and the turn goes on.
//!
//! Driven through a fake provider that keeps each request, so a test can see
//! what the model was offered and what it was told the user answered.

use std::sync::{Arc, Mutex};

use enowx_core::{
    ask::{Answer, Reply},
    builtin_agents,
    config::Config,
    Agent, Discovery, Event, Role, SessionStore,
};
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

fn asks() -> String {
    calls(
        "ask",
        serde_json::json!({"questions": [{
            "question": "Which layout?",
            "options": [{"label": "Sidebar (recommended)"}, {"label": "Top bar"}]
        }]}),
    )
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

fn agent(base_url: &str, dir: &std::path::Path) -> Agent {
    let mut config = Config::default();
    config.use_endpoint("test", base_url, "test-key", "test-model");
    config.agent.workspace = Some(dir.to_path_buf());
    config.agent.max_steps = 6;
    config.agent.auto_compact = false;
    let discovery = Discovery {
        agents: builtin_agents(),
        ..Discovery::default()
    };
    Agent::with_discovery(
        config,
        SessionStore::new(dir.join(".enx-sessions")),
        discovery,
    )
}

/// Runs "build me a dashboard", answering each question with `answer`, or
/// stopping the turn at it when `answer` is None.
async fn run(agent: Agent, answer: Option<Answer>) -> Vec<Event> {
    let agent = Arc::new(agent);
    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    let cancel = tokio_util::sync::CancellationToken::new();
    let request = enowx_core::agent::RunRequest {
        prompt: "build me a dashboard".into(),
        session_id: None,
        role: Role::Orchestrator,
        attachments: Vec::new(),
        agent: None,
    };
    let runner = agent.clone();
    let stop = cancel.clone();
    let handle = tokio::spawn(async move { runner.run(request, tx, stop).await });
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        if let Event::Question { id, .. } = &event {
            match &answer {
                Some(answer) => assert!(agent.answer(id, answer.clone()), "someone was waiting"),
                None => cancel.cancel(),
            }
        }
        events.push(event);
    }
    let _ = handle.await;
    events
}

fn offered(body: &str, tool: &str) -> bool {
    body.contains(&format!("\"name\":\"{tool}\""))
}

#[tokio::test]
async fn the_answer_comes_back_to_the_agent_that_asked() {
    let dir = Dir::new("answer");
    let (url, bodies) = provider(vec![asks(), says("BUILDING-WITH-A-SIDEBAR")]).await;
    let chosen = Answer {
        replies: vec![Reply {
            chosen: vec!["Sidebar (recommended)".into()],
            other: String::new(),
            notes: vec![("Sidebar (recommended)".into(), "collapsible".into())],
        }],
    };
    let events = run(agent(&url, &dir.0).asking_user(), Some(chosen)).await;

    let asked = events.iter().find_map(|e| match e {
        Event::Question {
            agent, questions, ..
        } => Some((agent.clone(), questions.clone())),
        _ => None,
    });
    let (who, questions) = asked.expect("the user was asked");
    assert_eq!(who, "orchestrator");
    assert_eq!(questions[0].question, "Which layout?");
    assert_eq!(questions[0].options.len(), 2);

    let bodies = bodies.lock().unwrap();
    assert!(offered(&bodies[0], "ask"), "the tool is offered");
    assert_eq!(bodies.len(), 2, "one call to ask, one after the answer");
    assert!(
        bodies[1].contains("Chose: Sidebar (recommended).")
            && bodies[1].contains("Note on \\\"Sidebar (recommended)\\\": collapsible"),
        "the answer and its note are the tool's result: {}",
        bodies[1]
    );
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::Text { delta } if delta.contains("BUILDING"))));
}

/// With no one to answer (the HTTP server, a script), the tool is not
/// offered, and a model that calls it anyway is told to decide itself.
#[tokio::test]
async fn with_no_one_to_answer_the_agent_decides() {
    let dir = Dir::new("no-host");
    let (url, bodies) = provider(vec![asks(), says("DECIDED")]).await;
    let events = run(agent(&url, &dir.0), None).await;
    assert!(!events.iter().any(|e| matches!(e, Event::Question { .. })));
    let bodies = bodies.lock().unwrap();
    assert!(!offered(&bodies[0], "ask"), "not offered");
    assert!(
        bodies[1].contains("There is no user to ask here"),
        "and refused when called anyway"
    );
}

/// Stopping the turn is the way out of a question: the turn ends, and the
/// record says the user stopped instead of answering.
#[tokio::test]
async fn stopping_the_turn_at_a_question_ends_it() {
    let dir = Dir::new("stop");
    let (url, bodies) = provider(vec![asks(), says("NEVER-SENT")]).await;
    let events = run(agent(&url, &dir.0).asking_user(), None).await;
    assert!(events.iter().any(|e| matches!(e, Event::Question { .. })));
    assert_eq!(bodies.lock().unwrap().len(), 1, "no call after the stop");
    assert!(events.iter().any(|e| matches!(
        e,
        Event::ToolResult { content, is_error: true, .. } if content.contains("stopped")
    )));
}

/// A delegated sub-agent cannot reach the user, so it is not offered the
/// tool: its questions belong in its report.
#[tokio::test]
async fn a_delegated_agent_is_not_offered_the_tool() {
    let dir = Dir::new("delegated");
    let (url, bodies) = provider(vec![
        calls(
            "delegate",
            serde_json::json!({"agent": "fe", "task": "build the dashboard"}),
        ),
        says("DONE: built it"),
        says("ALL-DONE"),
    ])
    .await;
    let _ = run(agent(&url, &dir.0).asking_user(), None).await;
    let bodies = bodies.lock().unwrap();
    assert!(bodies.len() >= 2, "the orchestrator, then fe in its branch");
    assert!(
        offered(&bodies[0], "ask"),
        "the orchestrator holds the conversation"
    );
    assert!(offered(&bodies[1], "delegate") || !offered(&bodies[1], "ask"));
    assert!(!offered(&bodies[1], "ask"), "fe works in a branch");
}
