//! Stopping a turn. A key that is documented as interrupting has to reach the
//! cancellation token, not just change the status line.

use crossterm::event::KeyCode;
use enowx_tui::testing::TestApp;

#[test]
fn ctrl_c_cancels_a_running_turn() {
    let mut app = TestApp::new();
    let cancel = app.start_fake_turn();
    assert!(!cancel.is_cancelled());
    app.press(KeyCode::Char('c'), true).expect("ctrl+c");
    assert!(cancel.is_cancelled(), "Ctrl+C during a turn must cancel it");
}

/// The composer having text in it must not turn Ctrl+C into "clear the input"
/// while a turn is running: stopping the model is the more urgent of the two.
#[test]
fn ctrl_c_cancels_even_with_text_in_the_composer() {
    let mut app = TestApp::new();
    let cancel = app.start_fake_turn();
    app.type_input("something I typed while waiting");
    app.press(KeyCode::Char('c'), true).expect("ctrl+c");
    assert!(cancel.is_cancelled(), "the turn should still be cancelled");
    assert_eq!(
        app.input_text(),
        "something I typed while waiting",
        "and what was typed should survive"
    );
}

/// Ctrl+C with a window open should still stop the turn. The window is in
/// front of the transcript, but the model is what is running.
#[test]
fn ctrl_c_cancels_while_a_window_is_open() {
    let mut app = TestApp::new();
    let cancel = app.start_fake_turn();
    app.press(KeyCode::Char('p'), true).expect("open palette");
    assert!(app.palette_open());
    app.press(KeyCode::Char('c'), true).expect("ctrl+c");
    assert!(
        cancel.is_cancelled(),
        "a turn should be interruptible with a window open"
    );
}

#[test]
fn esc_cancels_a_running_turn() {
    let mut app = TestApp::new();
    let cancel = app.start_fake_turn();
    app.press(KeyCode::Esc, false).expect("esc");
    assert!(cancel.is_cancelled(), "Esc during a turn must cancel it");
}

/// Esc with nothing running keeps its old job: clear the composer.
#[test]
fn esc_still_clears_the_composer_when_idle() {
    let mut app = TestApp::new();
    app.type_input("draft text");
    app.press(KeyCode::Esc, false).expect("esc");
    assert_eq!(app.input_text(), "", "Esc should clear an idle composer");
}

/// Esc during a turn stops the model rather than throwing away the draft.
#[test]
fn esc_during_a_turn_keeps_the_draft() {
    let mut app = TestApp::new();
    let cancel = app.start_fake_turn();
    app.type_input("my next question");
    app.press(KeyCode::Esc, false).expect("esc");
    assert!(cancel.is_cancelled());
    assert_eq!(
        app.input_text(),
        "my next question",
        "interrupting should not discard what was typed"
    );
}

/// Esc has a third job: closing an open window. That must survive, and it
/// must not also stop the turn — the window was what the key was aimed at.
#[test]
fn esc_closes_a_window_without_stopping_the_turn() {
    let mut app = TestApp::new();
    let cancel = app.start_fake_turn();
    app.press(KeyCode::Char('p'), true).expect("open palette");
    assert!(app.palette_open());
    app.press(KeyCode::Esc, false).expect("esc");
    assert!(!app.palette_open(), "Esc should close the window");
    assert!(
        !cancel.is_cancelled(),
        "closing a window should not stop the model as well"
    );
    // A second Esc, with nothing in front any more, does stop it.
    app.press(KeyCode::Esc, false).expect("esc");
    assert!(cancel.is_cancelled(), "now Esc should stop the turn");
}
