//! A handoff has to continue the reply, not end it.
//!
//! The router hands over so the incoming agent can be assembled with its own
//! prompt, model and tools — which means the turn ends. Before this fix the
//! *reply* ended with it: the new agent waited for another message from the
//! user, who had no way to know one was wanted and saw the session stop dead
//! after "router → fe".
//!
//! Driven through a fake OpenAI-compatible endpoint, because the behaviour
//! under test is what the agent loop does between two model calls.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use enowx_core::{
    agent_def::ORCHESTRATOR,
    builtin_agents,
    config::Config,
    session::{RETURN_REASON, USER_SWITCH_REASON},
    Agent, Discovery, Event, Role, Session, SessionStore,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Replies in order, one per request. Each is a full SSE stream, or a whole
/// HTTP response when it starts with `HTTP/`.
async fn fake_provider(replies: Vec<String>) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let index = seen.fetch_add(1, Ordering::SeqCst);
            let body = replies
                .get(index)
                .cloned()
                .unwrap_or_else(|| replies.last().cloned().unwrap_or_default());
            let seen = seen.clone();
            tokio::spawn(async move {
                // Read the request; we do not care what is in it.
                let mut buf = [0u8; 8192];
                let _ = socket.read(&mut buf).await;
                let _ = seen;
                let response = if body.starts_with("HTTP/") {
                    body
                } else {
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\
                         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                };
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.flush().await;
            });
        }
    });
    (format!("http://127.0.0.1:{port}/v1"), calls)
}

/// An SSE stream: some text, a finish reason, then done. `[DONE]` closes the
/// stream, so it comes once and last — an earlier one ends the stream before
/// the finish reason and the provider reports a truncated response.
fn says(text: &str) -> String {
    let delta = serde_json::json!({
        "choices": [{ "delta": { "content": text }, "finish_reason": null }]
    });
    let finish = serde_json::json!({
        "choices": [{ "delta": {}, "finish_reason": "stop" }]
    });
    format!("data: {delta}\n\ndata: {finish}\n\ndata: [DONE]\n\n")
}

/// An SSE stream calling the handoff tool.
fn hands_over_to(agent: &str) -> String {
    let call = serde_json::json!({
        "choices": [{
            "delta": {
                "tool_calls": [{
                    "index": 0,
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "handoff",
                        // `arguments` is a JSON *string* holding JSON, as
                        // the wire format has it.
                        "arguments": format!(
                            r#"{{"agent":"{agent}","reason":"this is frontend work"}}"#
                        )
                    }
                }]
            },
            "finish_reason": null
        }]
    });
    let finish = serde_json::json!({
        "choices": [{ "delta": {}, "finish_reason": "tool_calls" }]
    });
    format!("data: {call}\n\ndata: {finish}\n\ndata: [DONE]\n\n")
}

/// A response no retry or cheaper model can fix: the key is refused.
fn refuses_the_key() -> String {
    let body = r#"{"error":{"message":"invalid api key"}}"#;
    format!(
        "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn config_for(base_url: &str, workspace: &std::path::Path) -> Config {
    let mut config = Config::default();
    config.use_endpoint("test", base_url, "test-key", "test-model");
    config.agent.workspace = Some(workspace.to_path_buf());
    config.agent.max_steps = 6;
    config.agent.auto_compact = false;
    config
}

struct Dir(std::path::PathBuf);
impl Dir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "enx-handoff-{}-{}-{tag}",
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

/// The built-in roster with no MCP servers: `Agent::new` would warm the
/// developer's real `~/.mcp.json` servers on the first turn — tens of seconds,
/// and a hang on a machine without them.
fn isolated_agent(config: Config) -> Agent {
    let discovery = Discovery {
        agents: builtin_agents(),
        ..Discovery::default()
    };
    // Sessions go in the test's own directory, cleaned up with it. The
    // default store is the user's real one: every run of these tests left a
    // handful of sessions there, 372 of them before this was noticed.
    let store = SessionStore::new(config.workspace().join(".enx-sessions"));
    Agent::with_discovery(config, store, discovery)
}

async fn collect(replies: Vec<String>, tag: &str) -> (Vec<Event>, usize) {
    let run = send(replies, tag, None, None).await;
    (run.events, run.calls)
}

struct Run {
    events: Vec<Event>,
    calls: usize,
    /// The session as saved once the reply ended.
    session: Session,
}

/// Sends one message, into `existing` when given (saved first) or a new
/// session starting with `agent`, and waits for the reply to end.
async fn send(
    replies: Vec<String>,
    tag: &str,
    existing: Option<Session>,
    agent: Option<&str>,
) -> Run {
    let dir = Dir::new(tag);
    let (base_url, calls) = fake_provider(replies).await;
    let config = config_for(&base_url, &dir.0);
    let store = SessionStore::new(config.workspace().join(".enx-sessions"));
    let session_id = existing.map(|session| {
        store.save(&session).expect("save the session");
        session.id
    });
    let runner = isolated_agent(config);
    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    let cancel = tokio_util::sync::CancellationToken::new();
    let request = enowx_core::agent::RunRequest {
        prompt: "make the page nicer".into(),
        session_id,
        role: Role::Orchestrator,
        attachments: Vec::new(),
        agent: agent.map(Into::into),
    };
    let handle = tokio::spawn(async move { runner.run(request, tx, cancel).await });
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    let _ = handle.await;
    let id = events
        .iter()
        .find_map(|event| match event {
            Event::Session { id, .. } => Some(id.clone()),
            _ => None,
        })
        .expect("the turn names its session");
    Run {
        session: store.load(&id).expect("the session was saved"),
        calls: calls.load(Ordering::SeqCst),
        events,
    }
}

/// Who took the conversation, in order.
fn handovers(events: &[Event]) -> Vec<(&str, &str)> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::AgentSwitched { to, reason } => Some((to.as_str(), reason.as_str())),
            _ => None,
        })
        .collect()
}

fn position(events: &[Event], wanted: impl Fn(&Event) -> bool) -> usize {
    events
        .iter()
        .position(wanted)
        .unwrap_or_else(|| panic!("missing event: {events:#?}"))
}

/// A conversation with one exchange, held by `agent` after a handover for
/// `reason`.
fn held_by(agent: &str, reason: &str) -> Session {
    let mut session = Session::new(Role::Orchestrator);
    session.push(enowx_core::message::Message::user("build the login page"));
    session.push(enowx_core::message::Message::assistant("done"));
    session.switch_agent(agent, reason);
    session
}

/// The fix: after handing over, the incoming agent answers in the same turn.
#[tokio::test]
async fn a_handoff_is_followed_by_the_new_agents_reply() {
    let (events, calls) = collect(
        vec![hands_over_to("fe"), says("HERE-IS-THE-IMPROVED-PAGE")],
        "continues",
    )
    .await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::AgentSwitched { to, .. } if to == "fe")),
        "the handoff should happen: {events:#?}"
    );
    let text: String = events
        .iter()
        .filter_map(|e| match e {
            Event::Text { delta } => Some(delta.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        text.contains("HERE-IS-THE-IMPROVED-PAGE"),
        "the incoming agent should answer without another message from the \
         user; got {text:?} after {calls} model calls"
    );
    assert!(calls >= 2, "the new agent should have been called: {calls}");
}

/// And the turn ends once, at the end — not at the handoff.
#[tokio::test]
async fn done_arrives_after_the_new_agent_has_answered() {
    let (events, _) = collect(vec![hands_over_to("fe"), says("THE-REPLY")], "done-order").await;

    let done = events
        .iter()
        .position(|e| matches!(e, Event::Done { .. }))
        .expect("the turn should finish");
    let reply = events
        .iter()
        .position(|e| matches!(e, Event::Text { delta } if delta.contains("THE-REPLY")))
        .expect("the new agent should have replied");
    assert!(
        reply < done,
        "Done at {done} should come after the reply at {reply}"
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, Event::Done { .. }))
            .count(),
        1,
        "one turn, one Done"
    );
}

/// A chain that never settles must stop rather than run up a bill.
///
/// In practice only the router may hand over, so a true ping-pong between two
/// agents cannot arise through this path — the specialist that receives the
/// session has no handoff tool. What this actually exercises is that a reply
/// which keeps calling tools is bounded and still reports back, which is the
/// same guarantee from the user's side: the turn ends and says why.
#[tokio::test]
async fn a_turn_that_never_settles_stops_and_says_so() {
    // Every reply hands over again, so nothing but the bound stops this.
    let (events, calls) = collect(vec![hands_over_to("fe")], "loop").await;
    assert!(
        calls <= 10,
        "a handoff loop should be cut short, not run forever: {calls} calls"
    );
    assert!(
        events.iter().any(|e| matches!(e, Event::Done { .. })),
        "and it should still finish the turn: {events:#?}"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Notice { message }
                if message.contains("handing over") || message.contains("Stopped")
        )),
        "the user should be told why it stopped rather than it just ending: \
         {events:#?}"
    );
}

/// A specialist that was handed the conversation can give it back when the
/// user moves on, and the orchestrator answers in the same turn. Without a
/// way back, preferring handoff for iterative work would leave the user
/// stuck talking to the specialist about everything else.
#[tokio::test]
async fn a_specialist_hands_the_conversation_back() {
    let (events, _) = collect(
        vec![
            hands_over_to("fe"),
            hands_over_to(enowx_core::agent_def::ORCHESTRATOR),
            says("ORCHESTRATOR-TAKES-IT-FROM-HERE"),
        ],
        "hand-back",
    )
    .await;
    let path: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            Event::AgentSwitched { to, .. } => Some(to.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        path,
        vec!["fe", enowx_core::agent_def::ORCHESTRATOR],
        "handed to fe, then back: {events:#?}"
    );
    let text: String = events
        .iter()
        .filter_map(|e| match e {
            Event::Text { delta } => Some(delta.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        text.contains("ORCHESTRATOR-TAKES-IT-FROM-HERE"),
        "the orchestrator should answer after the hand-back: {text:?}"
    );
}

/// The orchestrator lends the conversation for one reply. The specialist's
/// answer is the reply, then the conversation goes back, so the next message
/// is routed afresh instead of landing with whoever spoke last.
#[tokio::test]
async fn the_conversation_goes_back_to_the_orchestrator_when_the_reply_ends() {
    let run = send(
        vec![hands_over_to("fe"), says("FE-ANSWER")],
        "goes-back",
        None,
        None,
    )
    .await;
    assert_eq!(
        handovers(&run.events),
        vec![
            ("fe", "this is frontend work"),
            (ORCHESTRATOR, RETURN_REASON)
        ],
    );
    let answer = position(
        &run.events,
        |e| matches!(e, Event::Text { delta } if delta.contains("FE-ANSWER")),
    );
    let back = position(
        &run.events,
        |e| matches!(e, Event::AgentSwitched { to, .. } if to == ORCHESTRATOR),
    );
    let done = position(&run.events, |e| matches!(e, Event::Done { .. }));
    assert!(
        answer < back && back < done,
        "back after the answer, before the end: {answer} {back} {done}"
    );
    assert_eq!(run.calls, 2, "giving it back calls no model");
    assert_eq!(run.session.agent_or_default(), ORCHESTRATOR);
    let last = run.session.switches.last().expect("a recorded return");
    assert_eq!(
        (last.from.as_str(), last.to.as_str(), last.reason.as_str()),
        ("fe", ORCHESTRATOR, RETURN_REASON)
    );
}

/// A session left with the specialist, by a process that was killed mid-reply
/// or saved before the rule, still sends a new message to the orchestrator.
#[tokio::test]
async fn a_new_message_reaches_the_orchestrator_first() {
    let run = send(
        vec![says("ORCHESTRATOR-ANSWER")],
        "left-lent",
        Some(held_by("fe", "this is frontend work")),
        None,
    )
    .await;
    assert_eq!(handovers(&run.events), vec![(ORCHESTRATOR, RETURN_REASON)]);
    let back = position(&run.events, |e| matches!(e, Event::AgentSwitched { .. }));
    let call = position(&run.events, |e| matches!(e, Event::MessageStart { .. }));
    assert!(back < call, "given back before the model is called");
    assert_eq!(run.session.agent_or_default(), ORCHESTRATOR);
}

/// A specialist the user picked with `/agent` is theirs to keep talking to.
#[tokio::test]
async fn a_specialist_the_user_picked_keeps_the_conversation() {
    let run = send(
        vec![says("FE-KEEPS-IT")],
        "picked",
        Some(held_by("fe", USER_SWITCH_REASON)),
        None,
    )
    .await;
    assert!(handovers(&run.events).is_empty(), "{:#?}", run.events);
    assert_eq!(run.session.agent_or_default(), "fe");
}

/// Picked before the first message, the agent starts the session the message
/// creates, and keeps it like any other pick.
#[tokio::test]
async fn an_agent_picked_before_the_first_message_answers_it() {
    let run = send(vec![says("FE-FIRST")], "picked-first", None, Some("fe")).await;
    assert!(handovers(&run.events).is_empty(), "{:#?}", run.events);
    assert_eq!(run.session.agent_or_default(), "fe");
    let pick = &run.session.switches[0];
    assert_eq!(
        (pick.from.as_str(), pick.to.as_str(), pick.reason.as_str()),
        (ORCHESTRATOR, "fe", USER_SWITCH_REASON)
    );
}

/// A reply that fails partway still ends with the orchestrator holding the
/// conversation, so the status bar names who gets the next message.
#[tokio::test]
async fn a_failed_reply_still_gives_the_conversation_back() {
    let run = send(
        vec![hands_over_to("fe"), refuses_the_key()],
        "failed",
        None,
        None,
    )
    .await;
    assert!(
        run.events.iter().any(|e| matches!(e, Event::Error { .. })),
        "the failure is reported: {:#?}",
        run.events
    );
    assert_eq!(
        handovers(&run.events),
        vec![
            ("fe", "this is frontend work"),
            (ORCHESTRATOR, RETURN_REASON)
        ],
    );
    assert_eq!(run.session.agent_or_default(), ORCHESTRATOR);
}

/// Like `fake_provider`, keeping every request body so a test can read what
/// each agent was sent.
async fn recording_provider(replies: Vec<String>) -> (String, Arc<std::sync::Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let bodies = Arc::new(std::sync::Mutex::new(Vec::new()));
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

/// The whole body of one HTTP request, by its Content-Length.
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

/// The agent taking over is told the work is now its own. Without it, one
/// continued the orchestrator's routing message ("I've passed this to fe")
/// and stopped with the page unbuilt.
#[tokio::test]
async fn the_incoming_agent_is_told_the_work_is_now_its_own() {
    let dir = Dir::new("told");
    let (base_url, bodies) = recording_provider(vec![hands_over_to("fe"), says("BUILT")]).await;
    let agent = isolated_agent(config_for(&base_url, &dir.0));
    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    let request = enowx_core::agent::RunRequest {
        prompt: "make the page nicer".into(),
        session_id: None,
        role: Role::Orchestrator,
        attachments: Vec::new(),
        agent: None,
    };
    let cancel = tokio_util::sync::CancellationToken::new();
    let handle = tokio::spawn(async move { agent.run(request, tx, cancel).await });
    while rx.recv().await.is_some() {}
    let _ = handle.await;

    let bodies = bodies.lock().unwrap();
    assert_eq!(bodies.len(), 2, "the orchestrator, then fe");
    assert!(
        !bodies[0].contains("HANDED TO YOU"),
        "the orchestrator is routing, not taking over"
    );
    assert!(bodies[1].contains("HANDED TO YOU"), "fe is told");
    assert!(
        bodies[1].contains("`orchestrator` handed this conversation to you: this is frontend work"),
        "who handed it over, and why"
    );
    // The handoff's own result, the last thing fe reads, speaks to fe.
    assert!(
        bodies[1].contains("`fe`: the user's last request is yours to carry out now"),
        "the tool result addresses the incoming agent"
    );
}
