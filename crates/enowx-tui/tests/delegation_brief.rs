//! The brief handed to a sub-agent is closed by default.
//!
//! A real one runs to forty lines of instructions, exclusions and acceptance
//! criteria. It is written for the sub-agent, and printing it in full pushes
//! the conversation it belongs to off the screen.

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use enowx_tui::testing::TestApp;

const W: u16 = 100;
const H: u16 = 30;

fn brief() -> String {
    format!(
        "ARAH VISUAL SUDAH DITETAPKAN\n{}",
        "• a long instruction line the sub-agent must follow\n".repeat(20)
    )
}

fn with_a_brief() -> TestApp {
    let mut app = TestApp::new();
    app.push_assistant("Murni kerja frontend.");
    app.deliver_delegation_started("fe", &brief(), "id-1");
    app
}

fn click_the_row(app: &mut TestApp) {
    let rows = app.render_to_text(W, H);
    let y = rows
        .iter()
        .position(|row| row.contains("delegate"))
        .expect("a delegate row") as u16;
    app.mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 6,
        row: y,
        modifiers: KeyModifiers::NONE,
    })
    .expect("click");
}

#[test]
fn the_brief_is_closed_by_default() {
    let mut app = with_a_brief();
    let text = app.render_to_text(W, H).join("\n");
    assert!(
        !text.contains("a long instruction line"),
        "the brief should not be printed in full: {text}"
    );
}

/// The row still has to say what happened, or a delegation becomes invisible.
#[test]
fn the_row_says_who_was_sent_what() {
    let mut app = with_a_brief();
    let row = app
        .render_to_text(W, H)
        .into_iter()
        .find(|row| row.contains("delegate"))
        .expect("a delegate row");
    assert!(row.contains("fe"), "the agent: {row}");
    assert!(row.contains("21"), "and the size of the brief: {row}");
    assert!(row.contains('▸'), "marked as openable: {row}");
}

#[test]
fn clicking_opens_it() {
    let mut app = with_a_brief();
    click_the_row(&mut app);
    let text = app.render_to_text(W, H).join("\n");
    assert!(
        text.contains("a long instruction line"),
        "a click should show the brief: {text}"
    );
}

#[test]
fn clicking_again_closes_it() {
    // A short brief, so the opened body does not scroll its own row off the
    // top: the row has to still be on screen to be clicked shut. A long one
    // scrolls like any other block, which is the transcript working, not a
    // fault in the row.
    let mut app = TestApp::new();
    app.deliver_delegation_started("fe", "line one\nline two\nline three", "id-1");
    click_the_row(&mut app);
    assert!(
        app.render_to_text(W, H).join("\n").contains("line two"),
        "open first"
    );
    click_the_row(&mut app);
    let text = app.render_to_text(W, H).join("\n");
    assert!(
        !text.contains("line two"),
        "a second click should close it again: {text}"
    );
    assert!(
        text.contains('▸'),
        "and the arrow should point back to closed: {text}"
    );
}

/// `/tools` opens tool bodies by default. A brief is not a tool result and
/// must stay closed — the bug that made the first click do nothing was
/// reading that toggle for the brief's default state.
#[test]
fn the_tool_output_toggle_does_not_open_briefs() {
    let mut app = TestApp::new();
    app.run_command("/tools").expect("/tools");
    app.deliver_delegation_started("fe", &brief(), "id-1");
    let text = app.render_to_text(W, H).join("\n");
    assert!(
        !text.contains("a long instruction line"),
        "expanding tool output should not expand briefs: {text}"
    );

    // And the first click must still open it.
    click_the_row(&mut app);
    assert!(
        app.render_to_text(W, H)
            .join("\n")
            .contains("a long instruction line"),
        "the first click has to do something"
    );
}

/// The render cache keys on what is drawn; a click that is not in the key
/// serves the closed row again and looks ignored.
#[test]
fn opening_it_survives_the_render_cache() {
    let mut app = with_a_brief();
    let _ = app.render_to_text(W, H);
    click_the_row(&mut app);
    let warm = app.render_to_text(W, H);
    assert!(
        warm.iter().any(|r| r.contains("a long instruction line")),
        "the cached closed row must not be served again"
    );
    app.clear_render_cache();
    let cold = app.render_to_text(W, H);
    assert_eq!(warm, cold, "warm and cold renders should agree");
}
