//! Updates: a release found at start is noted in the status bar until it is
//! installed, and Settings > Updates sets the check and installing.

use crossterm::event::KeyCode;
use enowx_tui::testing::TestApp;

#[test]
fn a_new_release_is_noted_in_the_status_bar() {
    let mut app = TestApp::new();
    app.set_update_available("v9.9.9");
    let screen = app.render_to_text(140, 30).join("\n");
    assert!(screen.contains("v9.9.9 available · /update"), "{screen}");
}

#[test]
fn update_checks_commit_independently_and_explain_restart_timing() {
    let mut app = TestApp::new();
    app.run_command("/settings").unwrap();
    app.type_keys("update.check_on_start");
    assert_eq!(
        app.settings_row_id().as_deref(),
        Some("update.check_on_start")
    );
    app.press_key(KeyCode::Char(' ')).unwrap();
    assert_eq!(
        app.config_value("update.check_on_start").as_deref(),
        Some("false")
    );
    app.press_key(KeyCode::Esc).unwrap();
    app.type_keys("update.auto_install");
    assert_eq!(
        app.settings_row_id().as_deref(),
        Some("update.auto_install")
    );
    let screen = app.render_to_text(150, 44).join("\n");
    assert!(
        screen.contains("after restarting enx"),
        "restart timing: {screen}"
    );
    app.press_key(KeyCode::Char(' ')).unwrap();
    assert_eq!(
        app.config_value("update.auto_install").as_deref(),
        Some("true")
    );
}
