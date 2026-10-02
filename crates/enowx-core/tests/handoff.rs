//! `/handoff`: a conversation carries on in a fresh session, its history
//! folded into a summary, the old session left as it was.

mod common;

use enowx_core::{
    builtin_agents, config::Config, message::Message, Agent, Discovery, Role, Session, SessionStore,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn says(text: &str) -> String {
    let delta = serde_json::json!({
        "choices": [{ "delta": { "content": text }, "finish_reason": null }]
    });
    let finish = serde_json::json!({ "choices": [{ "delta": {}, "finish_reason": "stop" }] });
    format!("data: {delta}\n\ndata: {finish}\n\ndata: [DONE]\n\n")
}

async fn summariser() -> String {
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
            tokio::spawn(async move {
                let mut buf = vec![0u8; 1 << 20];
                let _ = socket.read(&mut buf).await;
                let reply = says("Goal: build the shop. Done: the cart page. Next: checkout.");
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
async fn a_conversation_carries_on_light_in_a_new_session() {
    let dir = std::env::temp_dir().join(format!("enx-handoff-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut config = Config::default();
    config.use_endpoint("test", &summariser().await, "test-key", "test-model");
    config.agent.workspace = Some(dir.clone());
    let store = SessionStore::new(dir.with_extension("sessions"));
    let agent = Agent::with_discovery(
        config,
        store.clone(),
        Discovery {
            agents: builtin_agents(),
            ..Discovery::default()
        },
    );

    let mut old = Session::new(Role::Orchestrator);
    old.agent = "fe".into();
    for n in 0..12 {
        old.push(Message::user(format!("request {n}")));
        old.push(Message::assistant(format!("did {n} ").repeat(50)));
    }
    store.save(&old).unwrap();

    let new = agent.handoff(&old.id).await.expect("handoff");
    assert_ne!(new.id, old.id);
    assert_eq!(new.agent, "fe", "the same agent holds it");
    assert!(new.title.ends_with("(continued)"), "{}", new.title);
    assert!(
        new.turns.len() < old.turns.len(),
        "{} turns",
        new.turns.len()
    );
    let text: String = new.replay().iter().map(|m| m.content.clone()).collect();
    assert!(
        text.contains("the cart page"),
        "the summary carries the history"
    );
    assert!(
        text.contains("request 11"),
        "the last turns are kept as they were"
    );
    assert_eq!(
        store.load(&old.id).unwrap().turns.len(),
        24,
        "the old one is left alone"
    );
    assert!(store.load(&new.id).is_ok(), "the new one is saved");

    let short = Session::new(Role::Orchestrator);
    store.save(&short).unwrap();
    assert!(agent.handoff(&short.id).await.is_err(), "nothing to fold");
    let _ = std::fs::remove_dir_all(&dir);
}
