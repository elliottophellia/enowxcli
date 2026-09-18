//! Ctrl+P opens the settings form.
//!
//! It was otherwise only reachable through `/model`, which is not where
//! anyone looks for an API key or a base URL.

use crossterm::event::KeyCode;
use enowx_tui::testing::TestApp;

#[test]
fn it_opens_the_settings_form() {
    let mut app = TestApp::new();
    assert!(!app.settings_modal_open());
    app.press(KeyCode::Char('p'), true).expect("key");
    assert!(app.settings_modal_open(), "Ctrl+P should open settings");
}

/// Without the modifier it is an ordinary character, or typing "p" would be
/// impossible.
#[test]
fn a_bare_p_still_types() {
    let mut app = TestApp::new();
    app.press(KeyCode::Char('p'), false).expect("key");
    assert!(!app.settings_modal_open());
    assert_eq!(app.input_text(), "p");
}

/// Saving settings rebuilds the provider, so editing mid-turn would swap the
/// model underneath a running request.
#[test]
fn it_is_refused_while_a_turn_is_running() {
    let mut app = TestApp::new();
    app.set_busy(true);
    app.press(KeyCode::Char('p'), true).expect("key");
    assert!(
        !app.settings_modal_open(),
        "settings should not open during a turn"
    );
    assert!(
        app.status_line().contains("stop the current turn"),
        "the refusal should say why, got {:?}",
        app.status_line()
    );
}

/// The form arrives filled in from the current config, so one field can be
/// corrected without retyping the rest.
#[test]
fn the_form_opens_prefilled() {
    let mut app = TestApp::new();
    app.seed_provider(
        "enowx",
        "custom",
        "https://ai.example.id/v1",
        "https://ai.example.id/v1/models",
        "sk-secret",
        "some/model",
        128_000,
    );
    app.press(KeyCode::Char('p'), true).expect("key");
    assert!(app.settings_modal_open());
    assert_eq!(app.settings_field("base_url"), "https://ai.example.id/v1");
    assert_eq!(app.settings_field("api_key"), "sk-secret");
}

/// A shortcut nobody can discover is no shortcut.
#[test]
fn the_binding_is_documented() {
    let mut app = TestApp::new();
    app.run_command("/help").expect("help");
    let text = app.render_to_text(110, 60).join("\n");
    assert!(
        text.contains("Ctrl+P"),
        "help should list the binding: {text}"
    );
}
