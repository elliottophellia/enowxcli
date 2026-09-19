//! A sub-agent is not the agent the user is talking to.
//!
//! The user cannot send it a message, cannot steer it, and only the calling
//! agent decides what it does. So stopping the turn must not reach into it:
//! doing that left it dead mid-task while the caller was handed a `NO REPORT`
//! indistinguishable from a sub-agent that had simply given up.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use enowx_core::{config::Config, Agent, Event, Role};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Replies in order; the second one is held back until released, standing in
/// for a sub-agent that is still working.
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
            tokio::spawn(async move {
                let mut buf = [0u8; 8192];
                let _ = socket.read(&mut buf).await;
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

fn says(text: &str) -> String {
    let delta = serde_json::json!({
        "choices": [{ "delta": { "content": text }, "finish_reason": null }]
    });
    let finish = serde_json::json!({
        "choices": [{ "delta": {}, "finish_reason": "stop" }]
    });
    format!("data: {delta}\n\ndata: {finish}\n\ndata: [DONE]\n\n")
}

fn delegates_to(agent: &str, task: &str) -> String {
    let call = serde_json::json!({
        "choices": [{
            "delta": {
                "tool_calls": [{
                    "index": 0,
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "delegate",
                        "arguments": format!(
                            r#"{{"agent":"{agent}","task":"{task}"}}"#
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

struct Dir(std::path::PathBuf);
impl Dir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "enx-isolation-{}-{}-{tag}",
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

fn config_for(base_url: &str, workspace: &std::path::Path) -> Config {
    let mut config = Config::default();
    config.provider.name = "test".into();
    config.provider.base_url = base_url.into();
    config.provider.api_key = "test-key".into();
    config.model.default = "test-model".into();
    config.agent.workspace = Some(workspace.to_path_buf());
    config.agent.max_steps = 4;
    config.agent.auto_compact = false;
    config
}

/// Cancelling the turn while a sub-agent runs must not kill the sub-agent:
/// it finishes and reports, and the caller's `NO REPORT` path is not reached.
#[tokio::test]
async fn cancelling_the_turn_does_not_kill_a_running_sub_agent() {
    let dir = Dir::new("cancel");
    let (base_url, _calls) = fake_provider(vec![
        // The router delegates …
        delegates_to("fe", "write the page"),
        // … and the sub-agent answers with a proper report.
        says("DONE: wrote it\nCHANGED: index.html\nVERIFIED: opened it\nNEXT: nothing"),
        says("all finished"),
    ])
    .await;

    let agent = Agent::new(config_for(&base_url, &dir.0));
    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    let cancel = tokio_util::sync::CancellationToken::new();
    let request = enowx_core::agent::RunRequest {
        prompt: "build the page".into(),
        session_id: None,
        role: Role::Orchestrator,
        attachments: Vec::new(),
    };
    let watch = cancel.clone();
    let handle = tokio::spawn(async move { agent.run(request, tx, cancel).await });

    let mut events = Vec::new();
    let mut cancelled = false;
    while let Some(event) = rx.recv().await {
        // Cancel as soon as the delegation is under way — exactly what Ctrl+C
        // does while a sub-agent is working.
        if !cancelled && matches!(event, Event::DelegationStarted { .. }) {
            watch.cancel();
            cancelled = true;
        }
        events.push(event);
    }
    let _ = handle.await;

    assert!(cancelled, "the delegation should have started: {events:#?}");
    let finished = events.iter().find_map(|e| match e {
        Event::DelegationFinished {
            summary, failed, ..
        } => Some((summary.clone(), *failed)),
        _ => None,
    });
    let (summary, failed) = finished.expect("the sub-agent should still report back");
    assert!(
        !summary.contains("NO REPORT"),
        "a cancelled turn must not truncate the sub-agent mid-task: {summary}"
    );
    assert!(
        summary.contains("DONE:"),
        "it should have finished its work and reported: {summary}"
    );
    assert!(!failed, "and not be recorded as a failure: {summary}");
}

/// The turn itself still stops. Cancelling is not ignored — the caller does
/// not go on to another model call once its sub-agent is done.
#[tokio::test]
async fn cancelling_still_stops_the_turn_itself() {
    let dir = Dir::new("stops");
    let (base_url, calls) = fake_provider(vec![
        delegates_to("fe", "write the page"),
        says("DONE: wrote it\nCHANGED: index.html\nVERIFIED: yes\nNEXT: nothing"),
        says("the caller carries on"),
    ])
    .await;

    let agent = Agent::new(config_for(&base_url, &dir.0));
    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    let cancel = tokio_util::sync::CancellationToken::new();
    let request = enowx_core::agent::RunRequest {
        prompt: "build the page".into(),
        session_id: None,
        role: Role::Orchestrator,
        attachments: Vec::new(),
    };
    let watch = cancel.clone();
    let handle = tokio::spawn(async move { agent.run(request, tx, cancel).await });

    let mut events = Vec::new();
    let mut cancelled = false;
    while let Some(event) = rx.recv().await {
        if !cancelled && matches!(event, Event::DelegationStarted { .. }) {
            watch.cancel();
            cancelled = true;
        }
        events.push(event);
    }
    let _ = handle.await;

    let text: String = events
        .iter()
        .filter_map(|e| match e {
            Event::Text { delta } => Some(delta.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        !text.contains("the caller carries on"),
        "the cancelled turn should not make another model call: {text:?} \
         after {} calls",
        calls.load(Ordering::SeqCst)
    );
}
