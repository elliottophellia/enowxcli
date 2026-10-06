//! Independent General and Display preferences in the Settings row workspace.

use crossterm::event::KeyCode;
use enowx_tui::testing::TestApp;

fn screen(app: &mut TestApp) -> String {
    app.render_to_text(150, 50).join("\n")
}

#[test]
fn general_rows_show_help_and_persist_individual_switches() {
    let mut app = TestApp::new();
    app.press(KeyCode::Char('p'), true).expect("Settings");
    let text = screen(&mut app);
    assert_eq!(app.settings_row_id().as_deref(), Some("agent.preview"));
    for label in [
        "Look at pages in a browser (preview)",
        "Check edits with the language server",
        "Delegations run in the background",
        "Compact when the context is this full",
    ] {
        assert!(text.contains(label), "{label}: {text}");
    }
    assert!(
        text.contains("85%"),
        "percentage choice is formatted readably: {text}"
    );
    let before = app.config_value("agent.preview").expect("preview setting");
    app.type_keys("agent.preview");
    assert_eq!(app.settings_row_id().as_deref(), Some("agent.preview"));
    app.press_key(KeyCode::Char(' ')).expect("toggle preview");
    assert_ne!(
        app.config_value("agent.preview").as_deref(),
        Some(before.as_str())
    );
    assert!(
        screen(&mut app).contains("changed"),
        "successful toggle reports its effect"
    );
}

#[test]
fn invalid_text_keeps_the_old_value_and_editor_draft() {
    let mut app = TestApp::new();
    app.press(KeyCode::Char('p'), true).unwrap();
    app.type_keys("agent.shell_timeout_secs");
    assert_eq!(
        app.settings_row_id().as_deref(),
        Some("agent.shell_timeout_secs")
    );
    app.press_key(KeyCode::Enter).expect("open text editor");
    app.press(KeyCode::Char('u'), true).expect("clear editor");
    app.type_keys("0");
    app.press_key(KeyCode::Enter)
        .expect("reject invalid timeout");
    let text = screen(&mut app);
    assert!(
        text.contains("shell_timeout_secs must be 1..=86400"),
        "{text}"
    );
    assert_eq!(
        app.config_value("agent.shell_timeout_secs").as_deref(),
        Some("120")
    );
    assert!(app.is_modal_open(), "the failed editor remains open");
    assert_eq!(app.settings_field_value(), "0");
}

#[test]
fn a_valid_text_saves_only_its_row_and_escape_discards_the_next_draft() {
    let mut app = TestApp::new();
    app.press(KeyCode::Char('p'), true).unwrap();
    app.type_keys("agent.max_steps");
    app.press_key(KeyCode::Enter).unwrap();
    app.press(KeyCode::Char('u'), true).unwrap();
    app.type_keys("12");
    app.press_key(KeyCode::Enter).unwrap();
    assert_eq!(app.config_value("agent.max_steps").as_deref(), Some("12"));
    assert_eq!(app.active_tab(), "Settings");
    assert_eq!(app.settings_row_id().as_deref(), Some("agent.max_steps"));
    assert!(
        app.status_line().contains("changed"),
        "{}",
        app.status_line()
    );

    app.press_key(KeyCode::Enter).unwrap();
    app.press(KeyCode::Char('u'), true).unwrap();
    app.type_keys("13");
    app.press_key(KeyCode::Esc).unwrap();
    assert_eq!(app.config_value("agent.max_steps").as_deref(), Some("12"));
    assert_eq!(app.settings_row_id().as_deref(), Some("agent.max_steps"));
}

#[test]
fn display_and_typesafe_categories_remain_available_in_the_catalog() {
    let mut app = TestApp::new();
    app.press(KeyCode::Char('p'), true).unwrap();
    app.type_keys("ui.currency");
    assert_eq!(app.settings_row_id().as_deref(), Some("ui.currency"));
    app.press_key(KeyCode::Esc).unwrap();
    app.type_keys("TypeSafe");
    assert_eq!(
        app.settings_row_id().as_deref(),
        Some("typesafe.gate_tool_results")
    );
    let text = screen(&mut app);
    assert!(text.contains("TypeSafe API key"), "{text}");
}
