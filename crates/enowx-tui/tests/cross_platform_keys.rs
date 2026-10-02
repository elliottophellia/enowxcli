//! Every shortcut has a form that reaches enx from Linux, macOS and Windows
//! terminals, including the ones that cannot tell Ctrl+Enter from Enter.

use crossterm::event::{KeyCode, KeyModifiers};
use enowx_tui::testing::TestApp;

fn busy() -> TestApp {
    let mut app = TestApp::in_conversation();
    app.start_fake_turn();
    app
}

#[test]
fn shift_enter_is_a_newline_and_does_not_send() {
    let mut app = TestApp::in_conversation();
    app.type_keys("first");
    app.press_chord(KeyCode::Enter, KeyModifiers::SHIFT)
        .unwrap();
    app.type_keys("second");
    assert_eq!(app.composer_text(), "first\nsecond");
    assert!(!app.is_busy());
}

#[tokio::test]
async fn ctrl_s_sends_now_while_a_turn_runs() {
    let mut app = busy();
    app.type_keys("stop and do this");
    app.press_chord(KeyCode::Char('s'), KeyModifiers::CONTROL)
        .unwrap();
    assert_eq!(app.composer_text(), "");
    assert!(app.queued_texts().is_empty());
    assert!(
        app.block_texts().iter().any(|t| t == "stop and do this"),
        "{:?}",
        app.block_texts()
    );
}

#[test]
fn alt_enter_is_a_newline_while_a_turn_runs() {
    let mut app = busy();
    app.type_keys("a");
    app.press_chord(KeyCode::Enter, KeyModifiers::ALT).unwrap();
    app.type_keys("b");
    assert_eq!(app.composer_text(), "a\nb");
    assert!(app.queued_texts().is_empty());
}

#[test]
fn ctrl_d_quits_only_from_an_empty_composer() {
    let mut app = TestApp::in_conversation();
    app.type_keys("abc");
    app.press(KeyCode::Home, false).unwrap();
    app.press_chord(KeyCode::Char('d'), KeyModifiers::CONTROL)
        .unwrap();
    assert_eq!(app.composer_text(), "bc");
    assert!(!app.wants_to_quit());
    app.press(KeyCode::Char('c'), true).unwrap();
    app.press_chord(KeyCode::Char('d'), KeyModifiers::CONTROL)
        .unwrap();
    assert!(app.wants_to_quit());
}

#[test]
fn ctrl_and_alt_backspace_erase_a_word() {
    let mut app = TestApp::in_conversation();
    app.type_keys("fix the footer  ");
    app.press_chord(KeyCode::Backspace, KeyModifiers::CONTROL)
        .unwrap();
    assert_eq!(app.composer_text(), "fix the ");
    app.press_chord(KeyCode::Backspace, KeyModifiers::ALT)
        .unwrap();
    assert_eq!(app.composer_text(), "fix ");
    // Ctrl+Backspace as a raw DEL with Ctrl, the way some terminals send it.
    app.press_chord(KeyCode::Char('\x7f'), KeyModifiers::CONTROL)
        .unwrap();
    assert_eq!(app.composer_text(), "");
}

#[test]
fn altgr_characters_are_typed() {
    let mut app = TestApp::in_conversation();
    // Windows reports AltGr+Q on a German layout as Ctrl+Alt+@.
    for c in ['@', '{', '€'] {
        app.press_chord(KeyCode::Char(c), KeyModifiers::CONTROL | KeyModifiers::ALT)
            .unwrap();
    }
    assert_eq!(app.composer_text(), "@{€");
}

#[test]
fn an_alt_letter_types_nothing() {
    let mut app = TestApp::in_conversation();
    app.press_chord(KeyCode::Char('x'), KeyModifiers::ALT)
        .unwrap();
    assert_eq!(app.composer_text(), "");
}

#[test]
fn alt_b_and_alt_f_page_the_sidebar_without_typing() {
    let mut app = TestApp::in_conversation();
    app.press_chord(KeyCode::Char('f'), KeyModifiers::ALT)
        .unwrap();
    app.press_chord(KeyCode::Char('b'), KeyModifiers::ALT)
        .unwrap();
    assert_eq!(app.composer_text(), "");
}
