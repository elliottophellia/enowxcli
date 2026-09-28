//! The agent the user talks to is the orchestrator (it was `router`), and the
//! `/role` menu from before agents is gone.

use enowx_tui::testing::TestApp;

const W: u16 = 120;
const H: u16 = 36;

#[test]
fn a_new_session_talks_to_the_orchestrator() {
    let mut app = TestApp::new();
    assert_eq!(app.active_agent(), "orchestrator");
    // Under the home screen's composer, where a new session starts.
    assert!(app
        .render_to_text(W, H)
        .iter()
        .any(|row| row.contains("READY  Orchestrator")));
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
    let mut app = TestApp::in_conversation();
    app.select_sidebar_tab(0);
    let side = app.side_column(W, H).join("\n");
    assert!(!side.contains("Compactor"), "{side}");
    // Listed by full name, grouped by what the agents do.
    for group in ["LEAD", "BUILD", "SUPPORT"] {
        assert!(side.contains(group), "{side}");
    }
    for name in ["Orchestrator", "Frontend", "Backend", "Database", "Review"] {
        assert!(side.contains(name), "{name}: {side}");
    }
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
    let mut app = TestApp::in_conversation();
    app.select_sidebar_tab(1);
    let side = app.side_column(W, H).join("\n");
    assert!(side.contains("BLOCKED"), "{side}");
    for tool in ["write", "edit", "multi_edit"] {
        assert!(
            side.contains(tool),
            "the orchestrator cannot {tool}: {side}"
        );
    }
    // It runs commands to look and check, so bash is not blocked.
    let blocked = side.split("BLOCKED").nth(1).unwrap_or("");
    assert!(
        !blocked.contains("bash"),
        "the orchestrator can bash: {side}"
    );

    app.run_command("/agent fe").expect("/agent fe");
    let side = app.side_column(W, H).join("\n");
    assert!(
        !side.contains("\nwrite") && !side.contains(" write\n"),
        "fe can write: {side}"
    );
}
