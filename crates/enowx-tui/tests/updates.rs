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
fn the_updates_section_saves_its_switches() {
    let mut app = TestApp::new();
    app.run_command("/commands").unwrap();
    app.press_key(KeyCode::Esc).unwrap();
    // Reached through Settings: Ctrl+P, then up from the top wraps to it.
    app.press(KeyCode::Char('p'), true).unwrap();
    app.press_key(KeyCode::Up).unwrap();
    let screen = app.render_to_text(150, 44).join("\n");
    assert!(screen.contains("UPDATES · ENOWX"), "{screen}");
    assert!(
        screen.contains("Check for a new release at start"),
        "{screen}"
    );
    app.press_key(KeyCode::Enter).unwrap(); // into the section
    app.press_key(KeyCode::Down).unwrap(); // install by itself
    app.press_key(KeyCode::Right).unwrap(); // on
    app.press_key(KeyCode::Enter).unwrap();
    let update = enowx_core::Config::load().unwrap().update;
    assert!(update.check_on_start);
    assert!(update.auto_install);
}
