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
    app.run_command("/commands").expect("open palette");
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
    app.run_command("/commands").expect("open palette");
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

/// The point of the key is that the app stops. Waiting for the backend to
/// finish unwinding kept the composer locked and the spinner turning, which
/// reads as the key not having worked.
#[test]
fn stopping_frees_the_app_at_the_keypress() {
    let mut app = TestApp::new();
    let cancel = app.start_fake_turn();
    assert!(app.is_busy());
    app.press(KeyCode::Char('c'), true).expect("ctrl+c");
    assert!(cancel.is_cancelled());
    assert!(
        !app.is_busy(),
        "the app should be free immediately, not once the turn reports back"
    );
}

/// The stopped turn still reports `Done` once it has unwound. That must not
/// relabel the status the user is looking at.
#[test]
fn a_late_done_from_a_stopped_turn_is_ignored() {
    let mut app = TestApp::new();
    app.start_fake_turn();
    app.press(KeyCode::Char('c'), true).expect("ctrl+c");
    let after_stop = app.status_line();
    app.deliver_done("aborted");
    assert_eq!(
        app.status_line(),
        after_stop,
        "the late Done should not change what is shown"
    );
}

/// A stopped turn's `Done` cannot reach a later turn: starting one replaces
/// the channel it would have arrived on. So the only `Done` the new turn can
/// see is its own, and it must be honoured — anything else leaves the app
/// busy with no way out.
#[test]
fn the_next_turn_owns_its_own_completion() {
    let mut app = TestApp::new();
    app.start_fake_turn();
    app.press(KeyCode::Char('c'), true).expect("ctrl+c");
    let second = app.start_fake_turn();
    assert!(app.is_busy(), "the second turn is running");
    app.deliver_done("stop");
    assert!(
        !app.is_busy(),
        "its own Done must end it rather than being eaten by the earlier stop"
    );
    assert!(!second.is_cancelled(), "finishing is not cancelling");
}

/// A stop must not poison the turns that follow it. The abandoned flag was
/// set on interrupt and only cleared by the `Done` it was waiting for — so if
/// that never arrived, the NEXT turn's `Done` was swallowed instead, leaving
/// the app busy until the channel closed and reported "Agent stopped without
/// a terminal event".
#[test]
fn a_stop_does_not_swallow_the_next_turns_completion() {
    let mut app = TestApp::new();
    app.start_fake_turn();
    app.press(KeyCode::Esc, false).expect("stop it");
    assert!(!app.is_busy());

    // A fresh turn, whose Done is its own.
    app.start_fake_turn();
    assert!(app.is_busy());
    app.deliver_done("stop");
    assert!(
        !app.is_busy(),
        "the new turn's Done must be honoured, not eaten by the old stop"
    );
}
