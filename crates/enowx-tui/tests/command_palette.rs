//! Ctrl+P opens the command palette.
//!
//! The palette was otherwise only reachable by typing `/`, which assumes you
//! already know it is there. Ctrl+P is where most editors put the same thing.

use crossterm::event::KeyCode;
use enowx_tui::testing::TestApp;

/// The palette is driven by the composer starting with `/`, so that is what
/// "open" means here.
fn palette_open(app: &TestApp) -> bool {
    app.input_text().starts_with('/')
}

#[test]
fn it_opens_the_palette() {
    let mut app = TestApp::new();
    assert!(!palette_open(&app));
    app.press(KeyCode::Char('p'), true).expect("key");
    assert!(palette_open(&app), "Ctrl+P should open the palette");
}

#[test]
fn the_commands_are_listed() {
    let mut app = TestApp::new();
    app.press(KeyCode::Char('p'), true).expect("key");
    let text = app.render_to_text(110, 40).join("\n");
    for command in ["/help", "/agent", "/model", "/provider"] {
        assert!(text.contains(command), "`{command}` should be listed: {text}");
    }
}

/// A second press closes it rather than doing nothing — the key toggles.
#[test]
fn pressing_it_again_closes_the_palette() {
    let mut app = TestApp::new();
    app.press(KeyCode::Char('p'), true).expect("open");
    app.press(KeyCode::Char('p'), true).expect("close");
    assert!(!palette_open(&app));
    assert_eq!(app.input_text(), "");
}

/// A half-written message must survive: the shortcut prepends the slash rather
/// than replacing what is there.
#[test]
fn it_keeps_what_was_already_typed() {
    let mut app = TestApp::new();
    app.type_input("fix the parser");
    app.press(KeyCode::Char('p'), true).expect("key");
    assert_eq!(app.input_text(), "/fix the parser");
}

/// Without the modifier it is an ordinary character, or typing "p" would be
/// impossible.
#[test]
fn a_bare_p_still_types() {
    let mut app = TestApp::new();
    app.press(KeyCode::Char('p'), false).expect("key");
    assert!(!palette_open(&app));
    assert_eq!(app.input_text(), "p");
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
