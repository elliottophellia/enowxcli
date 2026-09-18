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

/// Ctrl+P closes whatever window is open. Otherwise the key that opens the
/// palette does nothing while a form is up, and the user presses it twice
/// wondering why.
#[test]
fn it_closes_an_open_window() {
    let mut app = TestApp::new();
    app.open_settings();
    assert!(app.settings_modal_open());
    app.press(KeyCode::Char('p'), true).expect("key");
    assert!(!app.settings_modal_open(), "Ctrl+P should close the form");
}

#[test]
fn it_closes_a_picker_too() {
    let mut app = TestApp::new();
    app.open_mcp_list();
    assert!(app.is_modal_open());
    app.press(KeyCode::Char('p'), true).expect("key");
    assert!(!app.is_modal_open());
}

/// The theme picker previews as you move through it; closing without choosing
/// has to put the saved theme back.
#[test]
fn closing_the_theme_picker_restores_the_saved_theme() {
    let mut app = TestApp::new();
    let before = app.theme_name();
    app.open_themes();
    app.preview_theme(2);
    assert_ne!(app.theme_name(), before, "the preview should have applied");
    app.press(KeyCode::Char('p'), true).expect("key");
    assert_eq!(
        app.theme_name(),
        before,
        "closing should restore the saved theme, not keep the preview"
    );
}

/// Closing a window does not then open the palette in the same press — one
/// key, one effect.
#[test]
fn closing_a_window_does_not_also_open_the_palette() {
    let mut app = TestApp::new();
    app.open_settings();
    app.press(KeyCode::Char('p'), true).expect("key");
    assert!(!app.settings_modal_open());
    assert_eq!(app.input_text(), "", "the composer should be untouched");
}
