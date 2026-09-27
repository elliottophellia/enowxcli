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

use enowx_core::{builtin_agents, config::Config, Agent, Discovery, Event, Role, SessionStore};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Replies in order, one per request. Each is a full SSE stream.
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
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
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

fn config_for(base_url: &str, workspace: &std::path::Path) -> Config {
    let mut config = Config::default();
    config.provider.name = "test".into();
    config.provider.base_url = base_url.into();
    config.provider.api_key = "test-key".into();
    config.model.default = "test-model".into();
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
    let dir = Dir::new(tag);
    let (base_url, calls) = fake_provider(replies).await;
    let agent = isolated_agent(config_for(&base_url, &dir.0));
    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    let cancel = tokio_util::sync::CancellationToken::new();
    let request = enowx_core::agent::RunRequest {
        prompt: "make the page nicer".into(),
        session_id: None,
        role: Role::Orchestrator,
        attachments: Vec::new(),
    };
    let handle = tokio::spawn(async move { agent.run(request, tx, cancel).await });
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    let _ = handle.await;
    let n = calls.load(Ordering::SeqCst);
    (events, n)
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
                if message.contains("handing over") || message.contains("Stopped after")
        )),
        "the user should be told why it stopped rather than it just ending: \
         {events:#?}"
    );
}
