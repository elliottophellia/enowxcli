//! The interface suite: its seeds score as they should, a case runs through
//! an agent and is scored, and the table shows the change between runs. The
//! model here is a stand-in, so nothing is spent.

use std::sync::{Arc, Mutex};

use enowx_core::config::Config;
use enowx_core::eval::{self, Case, Score, Setup};
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

fn folder(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("enx-eval-test-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const PAGE: &str = r##"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Roti Minggu</title>
<style>
:root { --ink: #1a1a1a; --paper: #fffdf8; }
body { margin: 0; font: 17px/1.6 Georgia, serif; color: var(--ink); background: var(--paper); }
main { max-width: 60ch; margin: 0 auto; padding: 24px 16px; }
</style></head>
<body><main>
<h1>Bread for the weekend, baked on Saturday morning</h1>
<p>Order by Thursday evening and collect it on Saturday from [address].</p>
</main></body></html>"##;

#[tokio::test]
async fn the_generated_seed_scores_low_and_the_designed_one_is_clean() {
    let cases = eval::cases();
    let seeded = |name: &str| cases.iter().find(|c| c.name == name).unwrap();

    let dir = folder("slop");
    for (path, content) in seeded("fix-generated").seed {
        std::fs::write(dir.join(path), content).unwrap();
    }
    let slop = eval::measure(&dir, "index.html", "fix-generated").await;
    assert!(slop.points < 30, "{slop:#?}");
    for rule in [
        "invented-figure",
        "emoji",
        "default-gradient",
        "em-dash",
        "dead-link",
    ] {
        assert!(slop.rules.contains_key(rule), "{rule}: {:?}", slop.rules);
    }
    assert!(!slop.design_md);

    // The clinic app is the direction a new page must keep to, so it must
    // not be flagged itself.
    let dir = folder("clinic");
    for (path, content) in seeded("settings-page").seed {
        std::fs::write(dir.join(path), content).unwrap();
    }
    let clinic = eval::measure(&dir, "index.html", "settings-page").await;
    assert_eq!(
        (clinic.ui_high, clinic.ui_medium),
        (0, 0),
        "{:?}",
        clinic.rules
    );
    assert!(clinic.design_md);
    if enowx_core::preview::find_chrome().is_some() {
        assert!(clinic.overflow.is_empty(), "{clinic:#?}");
        assert_eq!(clinic.low_contrast, 0, "{clinic:#?}");
    }
    let missing = eval::measure(&dir, "settings.html", "settings-page").await;
    assert!(
        missing.page_missing,
        "a page not built is not replaced by another"
    );
}

#[tokio::test]
async fn a_case_runs_through_the_agent_and_is_scored() {
    let question = serde_json::json!({"questions": [{
        "question": "Light or dark?",
        "header": "Theme",
        "options": [{"label": "Light (recommended)"}, {"label": "Dark"}]
    }]});
    let (url, bodies) = provider(vec![
        calls("ask", question),
        calls("skill_read", serde_json::json!({"name": "ui-page-local-business"})),
        calls(
            "write",
            serde_json::json!({"path": "DESIGN.md", "content": "# Design\n\nWarm and plain; light.\n"}),
        ),
        calls("write", serde_json::json!({"path": "index.html", "content": PAGE})),
        says("Built the page."),
    ])
    .await;
    let mut config = Config::default();
    config.provider.name = "test".into();
    config.provider.base_url = url;
    config.provider.api_key = "test-key".into();
    config.model.default = "test-model".into();
    config.model.price_input = 1.0;
    config.model.price_output = 2.0;
    config.agent.auto_compact = false;
    let case = Case {
        name: "bakery",
        prompt: "Build a page for a bakery.",
        seed: &[],
        page: "index.html",
    };
    let root = folder("run");
    let score = eval::run_case(&config, "fe", &case, &root, Setup::BuiltIn).await;

    assert_eq!(score.error, None);
    let bodies = bodies.lock().unwrap().clone();
    assert_eq!(
        bodies.len(),
        5,
        "the question, the skill, two writes, the report"
    );
    assert!(
        bodies[1].contains("Chose: Light (recommended)"),
        "a question is answered with its first option"
    );
    assert_eq!(score.skills_read, ["ui-page-local-business"]);
    assert_eq!(score.tools.get("write"), Some(&2));
    assert!(score.design_md && !score.page_missing);
    assert_eq!(
        (score.ui_high, score.ui_medium),
        (0, 0),
        "{:?}",
        score.rules
    );
    assert!(!score.gate_fired);
    assert_eq!(score.steps, 5);
    assert_eq!(score.before, None);
    if enowx_core::preview::find_chrome().is_some() {
        assert_eq!(score.h1_count, 1);
        assert!(score.points >= 95, "{score:#?}");
    }
    assert!(score.workspace.join("index.html").is_file());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_table_shows_each_case_against_the_run_before() {
    let score = |case: &str, points: i64| Score {
        case: case.into(),
        points,
        ..Score::default()
    };
    let mut fixed = score("fix-generated", 64);
    fixed.before = Some(12);
    let now = vec![score("bakery", 81), fixed.clone()];
    let table = eval::table(&now, &[]);
    assert!(table.contains("from 12"), "{table}");

    let before = vec![score("bakery", 70), score("fix-generated", 70)];
    let table = eval::table(&now, &before);
    let row = |name: &str| {
        table
            .lines()
            .find(|line| line.starts_with(name))
            .unwrap()
            .to_owned()
    };
    assert!(row("bakery").contains("+11"), "{table}");
    assert!(row("fix-generated").contains("-6"), "{table}");
    assert!(table.contains("mean 72.5 over 2 cases"), "{table}");

    let saved = serde_json::to_string(&now).unwrap();
    let loaded: Vec<Score> = serde_json::from_str(&saved).unwrap();
    assert_eq!(loaded[1].before, Some(12));
}

#[test]
fn points_fall_with_what_a_person_would_run_into() {
    let clean = Score {
        design_md: true,
        viewport_meta: true,
        h1_count: 1,
        ..Score::default()
    };
    assert_eq!(eval::points(&clean), 100);
    let worse = Score {
        ui_high: 2,
        overflow: vec![360],
        dead_links: 1,
        design_md: false,
        ..clean.clone()
    };
    assert_eq!(eval::points(&worse), 100 - 16 - 6 - 4 - 5);
    let missing = Score {
        page_missing: true,
        ..clean
    };
    assert_eq!(eval::points(&missing), 60);
}
