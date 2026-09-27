//! The agent the user talks to is the orchestrator (it was `router`), and the
//! `/role` menu from before agents is gone.

use enowx_tui::testing::TestApp;

const W: u16 = 120;
const H: u16 = 36;

#[test]
fn a_new_session_talks_to_the_orchestrator() {
    let mut app = TestApp::new();
    assert_eq!(app.active_agent(), "orchestrator");
    assert!(app.status_bar(W, H).contains("orchestrator"));
}

/// The old name still works where a user might type it.
#[test]
fn agent_router_still_reaches_the_orchestrator() {
    let mut app = TestApp::new();
    app.run_command("/agent fe").expect("/agent fe");
    assert_eq!(app.active_agent(), "fe");
    app.run_command("/agent router").expect("/agent router");
    assert_eq!(app.active_agent(), "orchestrator");
}

/// The compactor is machinery the loop runs itself: listing it, or letting
/// `/agent` switch to it, offered something no request can use.
#[test]
fn the_roster_lists_only_agents_a_request_can_go_to() {
    let mut app = TestApp::new();
    app.select_sidebar_tab(0);
    let side = app.side_column(W, H).join("\n");
    assert!(!side.contains("compactor"), "{side}");
    let routable = TestApp::new()
        .roster_names()
        .into_iter()
        .filter(|name| name != "compactor")
        .count();
    assert!(side.contains(&format!("ROSTER · {routable}")), "{side}");
    assert!(app.run_command("/agent compactor").is_err());
}

#[test]
fn the_role_menu_is_gone() {
    assert!(!TestApp::command_names()
        .iter()
        .any(|(name, _)| *name == "role"));
    let mut app = TestApp::new();
    app.run_command("/role")
        .expect("an unknown command is reported, not an error");
    assert!(
        app.block_texts()
            .iter()
            .any(|text| text.contains("Unknown command /role")),
        "typing it says so"
    );
}

#[test]
fn status_names_the_agent_not_a_role() {
    let mut app = TestApp::new();
    app.run_command("/status").expect("/status");
    let status = app.block_texts().join("\n");
    assert!(status.contains("Agent: orchestrator"), "{status}");
    assert!(!status.contains("Role:"), "{status}");
}

/// The Tools card marks what the agent holding the session cannot use,
/// rather than what a retired role could not.
#[test]
fn the_tools_card_shows_what_the_active_agent_cannot_use() {
    let mut app = TestApp::new();
    app.select_sidebar_tab(1);
    let side = app.side_column(W, H).join("\n");
    assert!(side.contains("BLOCKED"), "{side}");
    for tool in ["write", "edit", "bash"] {
        assert!(
            side.contains(tool),
            "the orchestrator cannot {tool}: {side}"
        );
    }

    app.run_command("/agent fe").expect("/agent fe");
    let side = app.side_column(W, H).join("\n");
    assert!(
        !side.contains("\nwrite") && !side.contains(" write\n"),
        "fe can write: {side}"
    );
}
