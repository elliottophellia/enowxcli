//! A message typed while a turn runs waits in a queue above the composer,
//! goes when the turn ends, and Ctrl+Enter sends it at once.

use crossterm::event::KeyCode;
use enowx_tui::testing::TestApp;

fn busy() -> TestApp {
    let mut app = TestApp::in_conversation();
    app.start_fake_turn();
    app
}

#[test]
fn enter_while_a_turn_runs_queues_the_message() {
    let mut app = busy();
    app.type_keys("also fix the footer");
    app.press_key(KeyCode::Enter).expect("enter");
    app.type_keys("and the header");
    app.press_key(KeyCode::Enter).expect("enter");
    assert_eq!(
        app.queued_texts(),
        vec!["also fix the footer", "and the header"]
    );
    assert_eq!(app.composer_text(), "", "the composer is free again");
    assert!(app.is_busy(), "the running turn goes on");
    let screen = app.render_to_text(120, 40).join("\n");
    assert!(screen.contains("QUEUED 2"), "{screen}");
    assert!(screen.contains("[send now]"), "{screen}");
    assert!(screen.contains("1. also fix the footer"), "{screen}");
}

#[tokio::test]
async fn the_next_message_goes_when_the_turn_ends() {
    let mut app = busy();
    app.type_keys("also fix the footer");
    app.press_key(KeyCode::Enter).expect("enter");
    app.tick_queue();
    assert_eq!(
        app.queued_texts().len(),
        1,
        "nothing goes while the turn runs"
    );
    app.deliver_done("stop");
    app.tick_queue();
    assert!(app.queued_texts().is_empty());
    assert!(app.is_busy(), "the queued message started a turn");
    assert!(
        app.block_texts().iter().any(|t| t == "also fix the footer"),
        "{:?}",
        app.block_texts()
    );
}

#[test]
fn stopping_the_turn_pauses_the_queue() {
    let mut app = busy();
    app.type_keys("later");
    app.press_key(KeyCode::Enter).expect("enter");
    app.press_key(KeyCode::Esc).expect("stop");
    assert!(!app.is_busy());
    app.tick_queue();
    assert_eq!(
        app.queued_texts(),
        vec!["later"],
        "stopped on purpose: it waits"
    );
    let screen = app.render_to_text(120, 40).join("\n");
    assert!(screen.contains("paused"), "{screen}");
}

#[tokio::test]
async fn ctrl_enter_sends_now() {
    // What is typed goes at once, ahead of the queue, stopping the turn.
    let mut app = busy();
    app.type_keys("queued one");
    app.press_key(KeyCode::Enter).expect("enter");
    app.type_keys("urgent");
    app.press(KeyCode::Enter, true).expect("ctrl+enter");
    assert!(
        app.block_texts().iter().any(|t| t == "urgent"),
        "{:?}",
        app.block_texts()
    );
    assert_eq!(app.queued_texts(), vec!["queued one"]);

    // With nothing typed, the first queued message goes.
    let mut app = busy();
    app.type_keys("first");
    app.press_key(KeyCode::Enter).expect("enter");
    app.press(KeyCode::Enter, true).expect("ctrl+enter");
    assert!(
        app.block_texts().iter().any(|t| t == "first"),
        "{:?}",
        app.block_texts()
    );
    assert!(app.queued_texts().is_empty());
}

#[test]
fn up_takes_the_last_queued_message_back() {
    let mut app = busy();
    app.type_keys("first");
    app.press_key(KeyCode::Enter).expect("enter");
    app.type_keys("second, with a typo");
    app.press_key(KeyCode::Enter).expect("enter");
    app.press_key(KeyCode::Up).expect("up");
    assert_eq!(app.composer_text(), "second, with a typo");
    assert_eq!(app.queued_texts(), vec!["first"]);
}

#[test]
fn idle_ctrl_enter_is_still_a_newline() {
    let mut app = TestApp::in_conversation();
    app.type_keys("line one");
    app.press(KeyCode::Enter, true).expect("ctrl+enter");
    assert_eq!(app.composer_text(), "line one\n");
}
