//! ACP: config, sign-in parsing, the MCP host's protocol, launching.
//! Nothing here starts a real agent or reaches the network.

use serde_json::json;

use super::mcp_host::{self, Route};
use super::*;

#[test]
fn no_engine_is_assigned_by_default() {
    let config = crate::config::Config::default();
    assert!(!config.acp.any_assigned());
    assert_eq!(config.acp.engine_for("orchestrator"), None);
}

#[test]
fn an_agent_runs_on_its_engine_and_enowx_means_none() {
    let mut acp = AcpConfig::default();
    acp.agents.insert("orchestrator".into(), "claude".into());
    acp.agents.insert("fe".into(), "enowx".into());
    acp.agents.insert("be".into(), "nope".into());
    assert_eq!(acp.engine_for("orchestrator"), Some("claude"));
    assert_eq!(acp.engine_for("fe"), None, "enowx is the configured model");
    assert_eq!(acp.engine_for("be"), None, "an unknown engine is ignored");
    acp.custom.insert("opencode".into(), CustomAgent::default());
    acp.agents.insert("be".into(), "opencode".into());
    assert_eq!(acp.engine_for("be"), Some("opencode"));
    assert_eq!(
        acp.engine_ids(),
        vec!["claude", "codex", "gemini", "opencode"]
    );
}

#[test]
fn the_acp_table_round_trips_through_toml() {
    let text = "[acp.agents]\norchestrator = \"claude\"\n\n\
                [acp.engines.claude]\nmodel = \"opus\"\npermission = \"enowx\"\n\n\
                [acp.custom.mine]\ncommand = \"my-agent\"\nargs = [\"--acp\"]\n\
                env = { TOKEN_FILE = \"/tmp/x\" }\n";
    let config: crate::config::Config = toml::from_str(text).expect("parses");
    assert_eq!(config.acp.engine("claude").model, "opus");
    assert_eq!(config.acp.engine("claude").permission, "enowx");
    assert_eq!(
        config.acp.engine("codex").permission,
        "ask",
        "safe by default"
    );
    assert_eq!(config.acp.custom["mine"].args, vec!["--acp"]);
    let back = toml::to_string(&config).expect("serialises");
    assert!(back.contains("[acp.agents]"), "{back}");
}

#[test]
fn a_custom_agent_that_is_not_there_says_so() {
    let mut acp = AcpConfig::default();
    acp.custom.insert(
        "ghost".into(),
        CustomAgent {
            command: "/definitely/not/here/agent".into(),
            ..CustomAgent::default()
        },
    );
    let error = launch(&acp, "ghost").expect_err("missing");
    assert!(error.contains("not found"), "{error}");
    assert!(launch(&acp, "nothing-by-that-name").is_err());
}

#[test]
fn a_custom_agent_starts_with_its_args_and_env() {
    let mut acp = AcpConfig::default();
    let sh = which("sh").expect("a shell");
    acp.custom.insert(
        "shell".into(),
        CustomAgent {
            command: sh.display().to_string(),
            args: vec!["-c".into(), "true".into()],
            env: [("A".to_owned(), "1".to_owned())].into(),
        },
    );
    let started = launch(&acp, "shell").expect("found");
    assert_eq!(started.program, sh);
    assert_eq!(started.args, vec!["-c", "true"]);
    assert!(started.env.iter().any(|(k, v)| k == "A" && v == "1"));
    assert!(started.env.iter().any(|(k, _)| k == "PATH"));
}

#[test]
fn claude_sign_in_is_read_without_the_email() {
    let s = parse_login(
        "claude",
        true,
        r#"{"loggedIn":true,"email":"a@b.c","subscriptionType":"max"}"#,
        "",
    );
    assert_eq!(
        s,
        LoginStatus {
            signed_in: true,
            label: Some("Claude Max".into())
        }
    );
    assert!(!parse_login("claude", false, r#"{"loggedIn":false}"#, "").signed_in);
}

#[test]
fn codex_sign_in_is_read() {
    let s = parse_login("codex", true, "Logged in using ChatGPT\n", "");
    assert_eq!(s.label.as_deref(), Some("ChatGPT"));
    assert!(!parse_login("codex", false, "", "Not logged in\n").signed_in);
}

#[test]
fn the_mcp_host_answers_initialize_and_lists_its_tools() {
    let route = Route::default();
    route.offer(
        vec![mcp_host::mcp_tool(&json!({"type": "function", "function": {
            "name": "skill_read", "description": "Read a skill",
            "parameters": {"type": "object"}
        }}))
        .unwrap()],
        "enowx tools".into(),
    );
    let init = mcp_host::handle(
        &route,
        &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": {"protocolVersion": "2025-06-18"}}),
    )
    .unwrap();
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["instructions"], "enowx tools");
    let list = mcp_host::handle(
        &route,
        &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    )
    .unwrap();
    assert_eq!(list["result"]["tools"][0]["name"], "skill_read");
    assert!(list["result"]["tools"][0]["inputSchema"].is_object());
    assert!(
        mcp_host::handle(
            &route,
            &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
        )
        .is_none(),
        "a notification has no answer"
    );
}

#[test]
fn a_call_with_no_turn_running_or_to_an_unknown_tool_is_an_error() {
    let route = Route::default();
    route.offer(
        vec![json!({"name": "delegate", "inputSchema": {"type": "object"}})],
        String::new(),
    );
    let unknown = mcp_host::handle(
        &route,
        &json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                "params": {"name": "rm_rf", "arguments": {}}}),
    )
    .unwrap();
    assert_eq!(unknown["result"]["isError"], true);
    assert!(unknown["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("Available: delegate"));
    let idle = mcp_host::handle(
        &route,
        &json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call",
                "params": {"name": "delegate", "arguments": {}}}),
    )
    .unwrap();
    assert!(idle["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("not running a turn"));
}

#[tokio::test]
async fn a_call_reaches_the_running_turn() {
    let route = std::sync::Arc::new(Route::default());
    route.offer(
        vec![json!({"name": "ask", "inputSchema": {"type": "object"}})],
        String::new(),
    );
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<mcp_host::Call>();
    route.attach(tx);
    let answering = tokio::spawn(async move {
        let call = rx.recv().await.expect("a call");
        assert_eq!(call.name, "ask");
        let _ = call.reply.send(mcp_host::tool_text("answered", false));
    });
    let r = route.clone();
    let reply = tokio::task::spawn_blocking(move || {
        mcp_host::handle(
            &r,
            &json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call",
                    "params": {"name": "ask", "arguments": {"q": 1}}}),
        )
    })
    .await
    .unwrap()
    .unwrap();
    answering.await.unwrap();
    assert_eq!(reply["result"]["content"][0]["text"], "answered");
}

#[test]
fn the_host_route_entry_carries_a_bearer_token() {
    let host = mcp_host::host().expect("binds 127.0.0.1");
    let (_, entry) = host.route();
    assert_eq!(entry["type"], "http");
    assert!(entry["url"]
        .as_str()
        .unwrap()
        .starts_with("http://127.0.0.1:"));
    let auth = entry["headers"][0]["value"].as_str().unwrap();
    assert!(auth.starts_with("Bearer ") && auth.len() > 60, "{auth}");
}

#[test]
fn a_detection_line_says_what_is_missing() {
    let missing = Detection {
        id: "claude".into(),
        ..Detection::default()
    };
    assert!(missing.line().contains("not installed"));
    assert!(missing.line().contains("Node.js not found"));
    assert!(!missing.ready());
    let ready = Detection {
        id: "codex".into(),
        node: Some("/usr/bin/node".into()),
        source: Some("enowx"),
        adapter: Some("/x".into()),
        adapter_version: Some("2.1.1".into()),
        login: Some(LoginStatus {
            signed_in: true,
            label: Some("ChatGPT".into()),
        }),
        ..Detection::default()
    };
    assert!(ready.ready());
    assert!(
        ready.line().contains("signed in (ChatGPT)"),
        "{}",
        ready.line()
    );
}
