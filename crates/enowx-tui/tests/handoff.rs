//! `/handoff` asks whether to keep this session's history, and the SESSION
//! card shows what the session costs the machine.

use crossterm::event::KeyCode;
use enowx_tui::testing::TestApp;

#[test]
fn handoff_asks_whether_to_keep_the_history() {
    let mut app = TestApp::in_conversation();
    app.run_command("/handoff").expect("handoff");
    let screen = app.render_to_text(120, 40).join("\n");
    assert!(screen.contains("HAND OFF TO A NEW SESSION"), "{screen}");
    assert!(screen.contains("Keep this session's history"), "{screen}");
    assert!(screen.contains("Delete this session's history"), "{screen}");
    // Cancel changes nothing.
    app.press_key(KeyCode::Down).expect("down");
    app.press_key(KeyCode::Down).expect("down");
    app.press_key(KeyCode::Enter).expect("cancel");
    assert!(!app.is_modal_open());
    assert!(!app.is_busy());
}

#[test]
fn there_is_nothing_to_hand_off_before_a_message() {
    let mut app = TestApp::new();
    app.run_command("/handoff").expect("handoff");
    assert!(!app.is_modal_open());
    let text = app.block_texts().join("\n");
    assert!(text.contains("send a message first"), "{text}");
}

#[test]
fn the_session_card_shows_memory_and_cpu() {
    let mut app = TestApp::in_conversation();
    app.tick_resources();
    let screen = app.render_to_text(140, 40).join("\n");
    assert!(screen.contains("memory"), "{screen}");
    assert!(screen.contains("cpu"), "{screen}");
}
