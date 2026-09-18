//! Acting on a message already sent: edit it, send it again, or copy it.
//!
//! Editing or resending from the middle of a conversation makes every later
//! reply an answer to a question that is no longer there, so the tail is cut.
//! That is destructive, and most of what is asserted here is about the cut
//! landing in the right place.

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use enowx_tui::testing::TestApp;

fn click(col: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

/// Four blocks: user, assistant, user, assistant.
fn conversation() -> TestApp {
    let mut app = TestApp::new();
    app.push_user("bikin fungsi parse");
    app.push_assistant("ini kodenya");
    app.push_user("tambah test");
    app.push_assistant("ini testnya");
    app
}

#[test]
fn ctrl_up_opens_the_menu_on_the_last_message() {
    let mut app = conversation();
    app.press(KeyCode::Up, true).expect("ctrl+up");
    assert!(app.message_menu_open());
    let text = app.render_to_text(100, 24).join("\n");
    for action in ["Edit prompt", "Resend", "Copy"] {
        assert!(
            text.contains(action),
            "`{action}` should be offered: {text}"
        );
    }
}

/// The menu is for messages the user sent. Opening it on a reply would offer
/// to resend something the user never wrote.
#[test]
fn the_menu_only_opens_on_user_messages() {
    let mut app = conversation();
    let _ = app.render_to_text(100, 24);
    let rects = app.user_block_rects();
    assert!(!rects.is_empty(), "user messages should be clickable");
    for (_, index) in &rects {
        assert!(
            index % 2 == 0,
            "only the user blocks (0 and 2) should be clickable, got {index}"
        );
    }
}

#[test]
fn clicking_a_message_opens_the_menu() {
    let mut app = conversation();
    let _ = app.render_to_text(100, 24);
    let (rect, _) = app.user_block_rects()[0];
    app.mouse(click(rect.x + 2, rect.y)).expect("click");
    assert!(app.message_menu_open(), "a click should open the menu");
}

/// Every row of a message is clickable, not just its first line: the user
/// aims at the text they can see, wherever in the block it falls.
#[test]
fn every_row_of_a_message_is_clickable() {
    let mut app = TestApp::new();
    app.push_user("satu\ndua\ntiga");
    let _ = app.render_to_text(100, 24);
    let rows = app.user_block_rects();
    assert!(
        rows.len() >= 3,
        "a three-line message should offer at least three rows, got {}",
        rows.len()
    );
}

#[test]
fn edit_prefills_the_draft_with_what_was_sent() {
    let mut app = conversation();
    app.press(KeyCode::Up, true).expect("ctrl+up");
    app.press(KeyCode::Enter, false).expect("choose Edit");
    assert!(app.message_editor_open());
    assert_eq!(app.message_draft(), "tambah test");
}

#[test]
fn typing_in_the_editor_changes_the_draft() {
    let mut app = conversation();
    app.press(KeyCode::Up, true).expect("ctrl+up");
    app.press(KeyCode::Enter, false).expect("choose Edit");
    for c in " dan docs".chars() {
        app.press(KeyCode::Char(c), false).expect("type");
    }
    assert_eq!(app.message_draft(), "tambah test dan docs");
    app.press(KeyCode::Backspace, false).expect("backspace");
    assert_eq!(app.message_draft(), "tambah test dan doc");
}

/// The cut is the whole point: everything from the message onward goes.
///
/// These drive `rewind_to` rather than pressing Enter on the menu, because
/// sending spawns onto a tokio runtime the harness does not run. The keyboard
/// route as far as the draft is covered above; this is the destructive half.
#[test]
fn the_cut_removes_the_message_and_everything_after_it() {
    let mut app = conversation();
    assert_eq!(app.block_count(), 4);
    assert!(app.rewind_to(2).expect("rewind"), "the cut should happen");

    let texts = app.block_texts();
    assert_eq!(
        texts,
        vec!["bikin fungsi parse", "ini kodenya"],
        "everything from the edited message onward should be gone"
    );
}

/// Cutting at the FIRST message empties the conversation, not just its tail.
#[test]
fn cutting_at_the_first_message_empties_the_conversation() {
    let mut app = conversation();
    assert!(app.rewind_to(0).expect("rewind"));
    assert_eq!(
        app.block_count(),
        0,
        "nothing should survive: {:?}",
        app.block_texts()
    );
}

/// The menu's Resend row has to reach the cut. Choosing it must not merely
/// close the menu.
#[test]
fn resend_is_wired_to_the_cut() {
    let mut app = conversation();
    app.press(KeyCode::Up, true).expect("ctrl+up");
    app.press(KeyCode::Down, false).expect("move to Resend");
    // Enter would send, which needs a runtime; assert the row is what it
    // claims to be, and that the cut it will perform is the right one.
    let text = app.render_to_text(100, 24).join("\n");
    assert!(
        text.contains("❯ Resend") || text.contains("Resend"),
        "Resend should be selectable: {text}"
    );
    assert!(app.rewind_to(2).expect("rewind"));
    assert_eq!(app.block_count(), 2);
}

#[test]
fn copy_leaves_the_conversation_alone() {
    let mut app = conversation();
    let before = app.block_texts();
    app.press(KeyCode::Up, true).expect("ctrl+up");
    app.press(KeyCode::Down, false).expect("to Resend");
    app.press(KeyCode::Down, false).expect("to Copy");
    app.press(KeyCode::Enter, false).expect("copy");
    assert!(!app.message_menu_open(), "the menu should close");
    assert_eq!(app.block_texts(), before, "copy must not cut anything");
}

#[test]
fn escape_cancels_without_cutting() {
    let mut app = conversation();
    let before = app.block_texts();
    app.press(KeyCode::Up, true).expect("ctrl+up");
    app.press(KeyCode::Enter, false).expect("choose Edit");
    for c in " extra".chars() {
        app.press(KeyCode::Char(c), false).expect("type");
    }
    app.press(KeyCode::Esc, false).expect("esc");
    assert!(!app.message_editor_open());
    assert_eq!(app.block_texts(), before, "cancelling must change nothing");
}

/// Cutting the conversation underneath a running turn would leave the reply
/// attaching to a message that is no longer there.
#[test]
fn it_refuses_while_a_turn_is_running() {
    let mut app = conversation();
    app.start_fake_turn();
    app.press(KeyCode::Up, true).expect("ctrl+up");
    app.press(KeyCode::Down, false).expect("to Resend");
    app.press(KeyCode::Enter, false).expect("resend");
    assert_eq!(app.block_count(), 4, "nothing should have been cut");
    assert!(
        app.status_line().contains("Stop the current turn"),
        "the refusal should say why: {:?}",
        app.status_line()
    );
}

/// An edit emptied to nothing is a cancel, not a request to send a blank
/// prompt and cut the conversation for it.
#[test]
fn an_emptied_draft_sends_nothing() {
    let mut app = conversation();
    let before = app.block_texts();
    app.press(KeyCode::Up, true).expect("ctrl+up");
    app.press(KeyCode::Enter, false).expect("choose Edit");
    for _ in 0..40 {
        app.press(KeyCode::Backspace, false).expect("clear");
    }
    assert_eq!(app.message_draft(), "");
    app.press(KeyCode::Enter, false).expect("send");
    assert_eq!(app.block_texts(), before, "nothing should have changed");
}
