//! Settings > General and Display: config keys edited in place and saved
//! through `Config::set`, which checks each.

use crossterm::event::KeyCode;
use enowx_tui::testing::TestApp;

fn screen(app: &mut TestApp) -> String {
    app.render_to_text(150, 50).join("\n")
}

#[test]
fn general_lists_how_the_agents_work_and_saves_a_switch() {
    let mut app = TestApp::new();
    app.press(KeyCode::Char('p'), true).unwrap(); // Settings, on its list
    let text = screen(&mut app);
    assert!(
        text.contains("GENERAL · HOW THE AGENTS WORK"),
        "General comes first: {text}"
    );
    for label in [
        "Look at pages in a browser (preview)",
        "Check edits with the language server",
        "Delegations run in the background",
        "Compact when the context is this full",
    ] {
        assert!(text.contains(label), "{label}: {text}");
    }
    assert!(
        text.contains("◂ 85% ▸"),
        "the threshold as a percentage: {text}"
    );
    let before = enowx_core::Config::load().unwrap().agent.preview;
    app.press_key(KeyCode::Enter).unwrap(); // into the section
    app.press_key(KeyCode::Right).unwrap(); // flip preview
    app.press_key(KeyCode::Enter).unwrap(); // save
    assert_eq!(enowx_core::Config::load().unwrap().agent.preview, !before);
}

#[test]
fn a_bad_value_is_refused_and_nothing_is_saved() {
    let mut app = TestApp::new();
    app.press(KeyCode::Char('p'), true).unwrap();
    app.press_key(KeyCode::Enter).unwrap();
    // Down to the shell timeout and make it 0.
    for _ in 0..8 {
        app.press_key(KeyCode::Down).unwrap();
    }
    app.press(KeyCode::Char('u'), true).unwrap(); // clear the field
    app.type_keys("0");
    app.press_key(KeyCode::Enter).unwrap();
    let text = screen(&mut app);
    assert!(
        text.contains("shell_timeout_secs must be 1..=86400"),
        "{text}"
    );
    assert_eq!(
        enowx_core::Config::load().unwrap().agent.shell_timeout_secs,
        120
    );
}

#[test]
fn display_and_typesafe_are_sections_of_settings() {
    let mut app = TestApp::new();
    app.press(KeyCode::Char('p'), true).unwrap();
    let text = screen(&mut app);
    assert!(
        text.contains(" Display ") && text.contains(" TypeSafe "),
        "{text}"
    );
}
