//! Configuring TypeSafe.
//!
//! It is deliberately not a provider: Jev answers typed questions, not
//! prompts, so listing it beside the chat providers would offer a model that
//! cannot hold a conversation. And with no key, every feature it powers stays
//! off — a default install must behave exactly as it did before.

use crossterm::event::KeyCode;
use enowx_tui::testing::TestApp;

#[test]
fn the_command_opens_its_own_window() {
    let mut app = TestApp::new();
    app.run_command("/typesafe").expect("/typesafe");
    assert_eq!(app.modal_title(), " TYPESAFE ");
}

/// The three things this window is for, all visible at once rather than one
/// per page.
#[test]
fn it_offers_the_key_and_both_features() {
    let mut app = TestApp::new();
    app.run_command("/typesafe").expect("/typesafe");
    let text = app.render_to_text(100, 24).join("\n");
    for row in [
        "API key",
        "Trim spent tool results",
        "Keep live turns through compaction",
    ] {
        assert!(text.contains(row), "`{row}` should be listed: {text}");
    }
}

/// Without a key nothing reaches the network, whatever the toggles say.
#[test]
fn a_fresh_install_is_off() {
    let app = TestApp::new();
    assert!(
        !app.typesafe_active(),
        "TypeSafe must be off until a key is set"
    );
}

#[test]
fn the_window_says_when_no_key_is_set() {
    let mut app = TestApp::new();
    app.run_command("/typesafe").expect("/typesafe");
    let text = app.render_to_text(100, 24).join("\n");
    assert!(
        text.contains("not set"),
        "the state should be plain: {text}"
    );
}

/// Jev cannot answer a prompt, so it must never appear where a chat model is
/// being chosen.
#[test]
fn it_is_not_offered_as_a_chat_provider() {
    let mut app = TestApp::new();
    app.run_command("/provider").expect("/provider");
    let text = app.render_to_text(100, 30).join("\n").to_lowercase();
    assert!(
        !text.contains("typesafe") && !text.contains("jev"),
        "TypeSafe is not a chat provider: {text}"
    );
}

#[test]
fn the_toggles_flip_and_persist() {
    let mut app = TestApp::new();
    app.run_command("/typesafe").expect("/typesafe");
    let before = app.typesafe_gating();
    app.press(KeyCode::Down, false).expect("to the toggle");
    app.press(KeyCode::Enter, false).expect("flip it");
    assert_ne!(app.typesafe_gating(), before, "the toggle should flip");
    let text = app.render_to_text(100, 24).join("\n");
    assert!(
        text.contains(if before { "off" } else { "on" }),
        "the window should show the new state: {text}"
    );
}

/// Entering the key opens a masked field rather than echoing it into the
/// transcript.
#[test]
fn the_key_is_entered_in_a_form() {
    let mut app = TestApp::new();
    app.run_command("/typesafe").expect("/typesafe");
    app.press(KeyCode::Enter, false).expect("choose API key");
    let text = app.render_to_text(100, 24).join("\n");
    assert!(text.contains("TypeSafe"), "the form should name it: {text}");
    for c in "secret-key-123".chars() {
        app.press(KeyCode::Char(c), false).expect("type");
    }
    let text = app.render_to_text(100, 24).join("\n");
    assert!(
        !text.contains("secret-key-123"),
        "the key must be masked as it is typed: {text}"
    );
}

#[test]
fn it_is_listed_in_the_command_palette() {
    let mut app = TestApp::new();
    app.press(KeyCode::Char('p'), true).expect("ctrl+p");
    assert!(
        app.select_palette("typesafe"),
        "the palette should offer /typesafe"
    );
}
