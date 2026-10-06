//! Settings > Team: independently committed master, child, and cycle rows.

use crossterm::event::KeyCode;
use enowx_tui::testing::TestApp;

fn screen(app: &mut TestApp) -> String {
    app.render_to_text(150, 44).join("\n")
}

#[test]
fn team_master_hides_children_until_enabled() {
    let mut app = TestApp::new();
    app.run_command("/team").unwrap();
    let text = screen(&mut app);
    assert_eq!(
        app.settings_row_id().as_deref(),
        Some("agent.comms.enabled")
    );
    assert!(text.contains("Enable agent teamwork"), "{text}");
    assert!(
        !text.contains("Agent messages"),
        "children should be hidden while off: {text}"
    );
    app.press_key(KeyCode::Char(' ')).unwrap();
    assert_eq!(
        app.config_value("agent.comms.enabled").as_deref(),
        Some("true")
    );
    assert_eq!(
        app.settings_row_id().as_deref(),
        Some("agent.comms.enabled")
    );
    let text = screen(&mut app);
    for label in [
        "Agent messages",
        "Shared team board",
        "Review delegated changes",
        "Review rounds",
        "Reviewer agent",
    ] {
        assert!(text.contains(label), "{label}: {text}");
    }
}

#[test]
fn disabling_team_hides_children_and_keeps_the_master_selected() {
    let mut app = TestApp::new();
    app.run_command("/team").unwrap();
    if app.config_value("agent.comms.enabled").as_deref() != Some("true") {
        app.press_key(KeyCode::Char(' ')).unwrap();
    }
    app.type_keys("agent.comms.enabled");
    assert_eq!(
        app.settings_row_id().as_deref(),
        Some("agent.comms.enabled")
    );
    app.press_key(KeyCode::Char(' ')).unwrap();
    assert_eq!(
        app.config_value("agent.comms.enabled").as_deref(),
        Some("false")
    );
    assert_eq!(
        app.settings_row_id().as_deref(),
        Some("agent.comms.enabled")
    );
    assert!(!screen(&mut app).contains("Reviewer agent"));
}

#[test]
fn rounds_cycle_commits_without_saving_other_team_rows() {
    let mut app = TestApp::new();
    app.run_command("/team").unwrap();
    app.press_key(KeyCode::Char(' ')).unwrap();
    app.type_keys("review_rounds");
    assert_eq!(
        app.settings_row_id().as_deref(),
        Some("agent.comms.review_rounds")
    );
    app.press_key(KeyCode::Right).unwrap();
    assert_eq!(
        app.config_value("agent.comms.review_rounds").as_deref(),
        Some("3")
    );
    assert!(screen(&mut app).contains("3"));
}

#[test]
fn team_description_search_and_empty_query_escape_keep_settings_open() {
    let mut app = TestApp::new();
    app.run_command("/team").unwrap();
    if app.config_value("agent.comms.enabled").as_deref() != Some("true") {
        app.press_key(KeyCode::Char(' ')).unwrap();
    }
    app.type_keys("working in parallel");
    assert_eq!(
        app.settings_row_id().as_deref(),
        Some("agent.comms.messages"),
        "{}",
        screen(&mut app)
    );
    assert_eq!(app.settings_query(), "working in parallel");

    app.press_key(KeyCode::Esc).unwrap();
    assert!(app.settings_query().is_empty());
    app.type_keys("definitely-no-setting-matches");
    assert!(screen(&mut app).contains("No settings match this query"));
    app.press_key(KeyCode::Esc).unwrap();
    assert_eq!(app.active_tab(), "Settings");
    assert!(app.settings_query().is_empty());
}
