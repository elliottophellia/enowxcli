//! What the model is told, checked on the wire.
//!
//! The project's instruction files and the skill list reached the model twice
//! on every call: `agent_prompt` appended them and the turn loop appended them
//! again. Captured here from the request the provider actually receives.

use std::sync::{Arc, Mutex};

use enowx_core::{
    builtin_agents,
    config::Config,
    discovery::{InstructionFile, SkillScope},
    Agent, Discovery, Role, SessionStore,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Answers every request with a short reply and keeps each request body.
async fn recording_provider() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let seen = bodies.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let seen = seen.clone();
            tokio::spawn(async move {
                let request = read_request(&mut socket).await;
                if let Some((_, body)) = request.split_once("\r\n\r\n") {
                    seen.lock().unwrap().push(body.to_owned());
                }
                let delta = serde_json::json!({
                    "choices": [{ "delta": { "content": "ok" }, "finish_reason": null }]
                });
                let finish = serde_json::json!({
                    "choices": [{ "delta": {}, "finish_reason": "stop" }]
                });
                let sse = format!("data: {delta}\n\ndata: {finish}\n\ndata: [DONE]\n\n");
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{sse}",
                    sse.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.flush().await;
            });
        }
    });
    (format!("http://127.0.0.1:{port}/v1"), bodies)
}

/// The whole request: headers, then as many body bytes as Content-Length
/// says. A system prompt runs to kilobytes, past any single read.
async fn read_request(socket: &mut tokio::net::TcpStream) -> String {
    let mut data = Vec::new();
    let mut buf = [0u8; 16384];
    while let Ok(n) = socket.read(&mut buf).await {
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
        let text = String::from_utf8_lossy(&data);
        if let Some((head, body)) = text.split_once("\r\n\r\n") {
            let length = head
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            if body.len() >= length {
                break;
            }
        }
    }
    String::from_utf8_lossy(&data).into_owned()
}

struct Dir(std::path::PathBuf);
impl Dir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "enx-prompt-{}-{}-{tag}",
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

async fn system_prompt_sent(tag: &str) -> String {
    let dir = Dir::new(tag);
    let (base_url, bodies) = recording_provider().await;
    let mut config = Config::default();
    config.provider.name = "test".into();
    config.provider.base_url = base_url;
    config.provider.api_key = "test-key".into();
    config.model.default = "test-model".into();
    config.agent.workspace = Some(dir.0.clone());
    config.agent.max_steps = 2;
    config.agent.auto_compact = false;

    let discovery = Discovery {
        agents: builtin_agents(),
        instructions: vec![InstructionFile {
            path: dir.0.join("AGENTS.md"),
            scope: SkillScope::Project,
            body: "PROJECT-RULE-MARKER: keep answers short.".into(),
            imports: 0,
            truncated: false,
        }],
        ..Discovery::default()
    };
    let store = SessionStore::new(dir.0.join(".enx-sessions"));
    let agent = Agent::with_discovery(config, store, discovery);
    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    let request = enowx_core::agent::RunRequest {
        prompt: "hello".into(),
        session_id: None,
        role: Role::Orchestrator,
        attachments: Vec::new(),
        agent: None,
    };
    let cancel = tokio_util::sync::CancellationToken::new();
    let handle = tokio::spawn(async move { agent.run(request, tx, cancel).await });
    while rx.recv().await.is_some() {}
    let _ = handle.await;

    let body = bodies
        .lock()
        .unwrap()
        .first()
        .cloned()
        .expect("the model should have been called");
    let json: serde_json::Value = serde_json::from_str(&body).expect("a JSON request");
    json["messages"]
        .as_array()
        .and_then(|messages| {
            messages
                .iter()
                .find(|m| m["role"] == "system")
                .and_then(|m| m["content"].as_str())
        })
        .expect("a system message")
        .to_owned()
}

#[tokio::test]
async fn instruction_files_reach_the_model_once() {
    let system = system_prompt_sent("once").await;
    assert_eq!(
        system.matches("PROJECT-RULE-MARKER").count(),
        1,
        "the instruction file should appear exactly once:\n{system}"
    );
}

#[tokio::test]
async fn every_agent_is_told_to_match_effort_to_the_task() {
    let system = system_prompt_sent("effort").await;
    for rule in [
        "Match effort to the task",
        "never for ls, find, cat",
        "choose it and say what you chose",
    ] {
        assert!(system.contains(rule), "missing {rule:?}:\n{system}");
    }
}

/// The workspace's top level is in the prompt, so no run has to spend a
/// model call listing it.
#[tokio::test]
async fn the_prompt_says_what_the_workspace_holds() {
    let system = system_prompt_sent("overview").await;
    assert!(
        system.contains("top level is empty"),
        "an empty workspace should say so:\n{system}"
    );
}

/// What `agent` is sent when it starts a session: its system prompt and the
/// names of the tools it is offered. Only the built-in skills are discovered,
/// so what each agent is offered is down to what it carries.
async fn sent_to(agent: &str, tag: &str) -> (String, Vec<String>) {
    let dir = Dir::new(tag);
    let (base_url, bodies) = recording_provider().await;
    let mut config = Config::default();
    config.provider.name = "test".into();
    config.provider.base_url = base_url;
    config.provider.api_key = "test-key".into();
    config.model.default = "test-model".into();
    config.agent.workspace = Some(dir.0.clone());
    config.agent.max_steps = 2;
    config.agent.auto_compact = false;
    let discovery = Discovery {
        agents: builtin_agents(),
        skills: enowx_core::discovery::skills::builtin_entries(),
        ..Discovery::default()
    };
    let store = SessionStore::new(dir.0.join(".enx-sessions"));
    let runner = Agent::with_discovery(config, store, discovery);
    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    let request = enowx_core::agent::RunRequest {
        prompt: "hello".into(),
        session_id: None,
        role: Role::Orchestrator,
        attachments: Vec::new(),
        agent: Some(agent.into()),
    };
    let cancel = tokio_util::sync::CancellationToken::new();
    let handle = tokio::spawn(async move { runner.run(request, tx, cancel).await });
    while rx.recv().await.is_some() {}
    let _ = handle.await;
    let body = bodies.lock().unwrap().first().cloned().expect("a call");
    let json: serde_json::Value = serde_json::from_str(&body).expect("a JSON request");
    let system = json["messages"][0]["content"]
        .as_str()
        .expect("a system message")
        .to_owned();
    let tools = json["tools"]
        .as_array()
        .map(|tools| {
            tools
                .iter()
                .filter_map(|tool| tool["function"]["name"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    (system, tools)
}

fn lists(system: &str, skill: &str) -> bool {
    system.contains(&format!("- `{skill}` "))
}

/// A built-in skill reaches only the agents that carry it: `fe` is offered
/// all three, `be` only `code`, and the orchestrator none, nor the tool to
/// read one.
#[tokio::test]
async fn built_in_skills_reach_only_the_agents_that_carry_them() {
    let (fe, fe_tools) = sent_to("fe", "fe-skills").await;
    for skill in ["ui", "code", "writing"] {
        assert!(lists(&fe, skill), "fe is offered `{skill}`:\n{fe}");
    }
    assert!(fe_tools.iter().any(|t| t == "skill_read"), "{fe_tools:?}");

    let (be, _) = sent_to("be", "be-skills").await;
    assert!(lists(&be, "code"), "be is offered `code`:\n{be}");
    assert!(
        !lists(&be, "ui") && !lists(&be, "writing"),
        "and nothing else:\n{be}"
    );

    let (orchestrator, tools) = sent_to("orchestrator", "orchestrator-skills").await;
    for skill in ["ui", "code", "writing"] {
        assert!(
            !lists(&orchestrator, skill),
            "the orchestrator routes:\n{orchestrator}"
        );
    }
    assert!(
        !tools.iter().any(|t| t == "skill_read"),
        "nothing to read, so no tool to read it: {tools:?}"
    );
}
