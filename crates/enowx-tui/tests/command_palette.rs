//! Ctrl+P opens the command palette: a floating, searchable list.
//!
//! Typing `/` still shows the inline list above the composer. That serves
//! someone who knows the name they want; this is for looking.

use crossterm::event::KeyCode;
use enowx_tui::testing::TestApp;

fn open(app: &mut TestApp) {
    app.press(KeyCode::Char('p'), true).expect("ctrl+p");
    // Every test below asserts against an OPEN palette. Without this, a
    // binding that does nothing leaves the ones asserting `!palette_open()`
    // passing for the wrong reason.
    assert!(app.palette_open(), "ctrl+p should have opened the palette");
}

#[test]
fn it_opens_a_modal() {
    let mut app = TestApp::new();
    assert!(!app.palette_open());
    open(&mut app);
    assert!(app.palette_open());
}

#[test]
fn every_command_is_listed() {
    let mut app = TestApp::new();
    open(&mut app);
    let text = app.render_to_text(110, 40).join("\n");
    for command in ["/help", "/agent", "/model", "/provider", "/quit"] {
        assert!(
            text.contains(command),
            "`{command}` should be listed: {text}"
        );
    }
}

/// Searching the summary as well as the name is the reason to open a palette
/// rather than type: you remember what it does, not what it is called.
#[test]
fn it_searches_summaries_not_just_names() {
    let mut app = TestApp::new();
    open(&mut app);
    let all = app.palette_row_count();
    for c in "palette".chars() {
        app.press_key(KeyCode::Char(c)).expect("type");
    }
    assert!(
        app.palette_row_count() < all,
        "typing should narrow the list"
    );

    let mut app = TestApp::new();
    open(&mut app);
    for c in "reasoning".chars() {
        app.press_key(KeyCode::Char(c)).expect("type");
    }
    assert!(
        app.palette_row_count() >= 1,
        "a word from a summary should still find its command"
    );
}

#[test]
fn arrows_move_the_selection() {
    let mut app = TestApp::new();
    open(&mut app);
    let first = app.palette_selection();
    app.press_key(KeyCode::Down).expect("down");
    let second = app.palette_selection();
    assert_ne!(first, second, "Down should move");
    app.press_key(KeyCode::Up).expect("up");
    assert_eq!(app.palette_selection(), first, "Up should come back");
}

/// The selection must not sit past the end after a search narrows the list.
#[test]
fn narrowing_the_search_resets_the_selection() {
    let mut app = TestApp::new();
    open(&mut app);
    for _ in 0..8 {
        app.press_key(KeyCode::Down).expect("down");
    }
    app.press_key(KeyCode::Char('q')).expect("type");
    let selected = app.palette_selection();
    assert!(selected.is_some(), "something should still be selected");
}

#[test]
fn enter_runs_the_highlighted_command() {
    let mut app = TestApp::new();
    open(&mut app);
    for c in "clear".chars() {
        app.press_key(KeyCode::Char(c)).expect("type");
    }
    app.push_assistant("some content");
    app.press_key(KeyCode::Enter).expect("enter");
    assert!(!app.palette_open(), "running a command closes the palette");
    assert_eq!(app.block_count(), 0, "/clear should have run");
}

/// A command needing an argument lands in the composer instead of running
/// with nothing.
#[test]
fn a_command_taking_an_argument_is_staged_not_run() {
    let mut app = TestApp::new();
    open(&mut app);
    for c in "agent".chars() {
        app.press_key(KeyCode::Char(c)).expect("type");
    }
    app.press_key(KeyCode::Enter).expect("enter");
    assert!(!app.palette_open());
    assert_eq!(app.input_text(), "/agent ", "ready for the name");
}

#[test]
fn escape_closes_it() {
    let mut app = TestApp::new();
    open(&mut app);
    app.press_key(KeyCode::Esc).expect("esc");
    assert!(!app.palette_open());
}

/// Ctrl+P while something else is open closes that, rather than doing nothing.
#[test]
fn it_closes_another_open_window() {
    let mut app = TestApp::new();
    app.open_settings();
    assert!(app.settings_modal_open(), "settings should be open first");
    app.press(KeyCode::Char('p'), true).expect("ctrl+p");
    assert!(!app.settings_modal_open());
    assert!(!app.palette_open(), "one key, one effect");
}

/// The theme picker previews as you move; closing must restore the saved one.
#[test]
fn closing_the_theme_picker_restores_the_saved_theme() {
    let mut app = TestApp::new();
    let before = app.theme_name();
    app.open_themes();
    app.preview_theme(2);
    assert_ne!(app.theme_name(), before);
    app.press(KeyCode::Char('p'), true).expect("ctrl+p");
    assert_eq!(app.theme_name(), before);
}

/// Typing `/` still works — the two routes are independent.
#[test]
fn the_inline_list_still_works() {
    let mut app = TestApp::new();
    app.type_input("/ne");
    let text = app.render_to_text(110, 40).join("\n");
    assert!(text.contains("/new"), "the inline list should still show");
    assert!(!app.palette_open(), "typing does not open the modal");
}

#[test]
fn the_binding_is_documented() {
    let mut app = TestApp::new();
    app.run_command("/help").expect("help");
    let text = app.render_to_text(110, 60).join("\n");
    assert!(text.contains("Ctrl+P"), "help should list it: {text}");
}
