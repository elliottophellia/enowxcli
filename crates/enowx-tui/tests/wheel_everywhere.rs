//! The mouse wheel works wherever the pointer is, and scrolls what is under
//! it a row a step: a list's view (not its selection), the side card, the
//! transcript.
//!
//! It used to scroll the transcript whenever it was not over the skill or
//! MCP list: with the command palette, a picker or the settings form open,
//! the wheel moved the conversation hidden behind the window, and over the
//! side column it did nothing at all.

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use enowx_tui::testing::TestApp;

const W: u16 = 120;
const H: u16 = 32;

fn wheel(down: bool, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: if down {
            MouseEventKind::ScrollDown
        } else {
            MouseEventKind::ScrollUp
        },
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

fn click(column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

/// One wheel step, past the throttle that keeps a trackpad burst from
/// flying through a list.
fn step(app: &mut TestApp, down: bool, column: u16, row: u16) {
    app.let_the_wheel_settle();
    app.mouse(wheel(down, column, row)).expect("wheel");
}

/// The wheel scrolls the open window's list a row a step and leaves the
/// selection where it is; the transcript behind does not move.
#[test]
fn the_wheel_scrolls_the_command_palette_not_the_transcript() {
    let mut app = TestApp::new();
    app.set_max_scroll(20);
    app.run_command("/commands").expect("/commands");
    let _ = app.render_to_text(W, H);
    let first = app.palette_selection();
    // Anywhere on screen: the open window has the wheel.
    step(&mut app, true, 5, 5);
    let _ = app.render_to_text(W, H);
    assert_eq!(app.scroll(), 0, "the transcript behind must not move");
    assert_eq!(app.palette_selection(), first, "the selection stays");
    assert_eq!(app.modal_offset(), 1, "the list moved a row");
    step(&mut app, false, 5, 5);
    let _ = app.render_to_text(W, H);
    assert_eq!(app.modal_offset(), 0, "and back");
}

/// Every event of a trackpad flick is a row: the list scrolls smoothly,
/// a row at a time, never a page.
#[test]
fn a_burst_scrolls_a_list_row_by_row() {
    let mut app = TestApp::new();
    app.run_command("/commands").expect("/commands");
    let _ = app.render_to_text(W, H);
    for _ in 0..5 {
        app.mouse(wheel(true, 5, 5)).expect("wheel");
    }
    let _ = app.render_to_text(W, H);
    assert_eq!(app.modal_offset(), 5);
    assert_eq!(app.palette_selection().as_deref(), Some("new"), "not moved");
}

/// After the wheel, a key moves the selection and the list follows it back
/// into view.
#[test]
fn a_key_brings_the_selection_back_into_view() {
    let mut app = TestApp::new();
    app.run_command("/commands").expect("/commands");
    let _ = app.render_to_text(W, H);
    for _ in 0..8 {
        app.mouse(wheel(true, 5, 5)).expect("wheel");
    }
    let _ = app.render_to_text(W, H);
    assert!(app.modal_offset() > 0);
    app.press_key(KeyCode::Down).expect("down");
    let _ = app.render_to_text(W, H);
    assert_eq!(app.palette_selection().as_deref(), Some("resume"));
    assert!(
        app.modal_offset() <= 2,
        "back at the selection: {}",
        app.modal_offset()
    );
}

/// A list that fits does not scroll, and the selection stays.
#[test]
fn the_wheel_leaves_a_short_list_and_its_selection() {
    let mut app = TestApp::new();
    app.run_command("/theme").expect("/theme");
    let _ = app.render_to_text(W, H);
    let first = app.modal_selection();
    step(&mut app, true, 60, 10);
    let _ = app.render_to_text(W, H);
    assert_eq!(app.modal_selection(), first);
    assert_eq!(app.modal_offset(), 0);
}

#[test]
fn the_wheel_does_not_move_between_settings_fields() {
    let mut app = TestApp::new();
    app.open_custom_provider_form();
    let first = app.modal_cursor();
    step(&mut app, true, 60, 10);
    assert_eq!(app.modal_cursor(), first);
}

#[test]
fn the_wheel_scrolls_the_side_column_a_line_a_step() {
    let mut app = TestApp::in_conversation();
    for n in 0..80 {
        app.deliver_trimmed(&format!("tool-{n}"), 9000, 800);
    }
    app.select_sidebar_tab(3);
    let _ = app.render_to_text(W, H);
    let (page, pages) = app.sidebar_page();
    assert!(pages > 1, "the log should need scrolling");
    let (x, y, w, h) = app.side_area().expect("a side column");
    step(&mut app, true, x + w / 2, y + h - 3);
    let _ = app.render_to_text(W, H);
    assert_eq!(app.sidebar_page().0, page + 1, "down one line");
    step(&mut app, false, x + w / 2, y + h - 3);
    let _ = app.render_to_text(W, H);
    assert_eq!(app.sidebar_page().0, page, "up one line");
}

#[test]
fn the_wheel_moves_the_inline_command_list() {
    let mut app = TestApp::new();
    app.set_max_scroll(20);
    app.type_input("/");
    let _ = app.render_to_text(W, H);
    let (x, y, w, h) = app.inline_palette_area().expect("the list is drawn");
    step(&mut app, true, x + w / 2, y + h / 2);
    assert_eq!(app.inline_palette_cursor(), 1);
    assert_eq!(app.scroll(), 0, "the transcript must not move");
}

#[test]
fn a_click_on_the_inline_command_list_runs_that_command() {
    let mut app = TestApp::new();
    app.push_assistant("something to clear");
    app.type_input("/cle");
    let rows = app.render_to_text(W, H);
    let y = rows
        .iter()
        .position(|row| row.contains("/clear"))
        .expect("`/clear` is listed") as u16;
    app.mouse(click(20, y)).expect("click");
    assert_eq!(app.block_count(), 0, "/clear should have run");
    assert_eq!(app.input_text(), "", "and the composer is empty again");
}

/// A long picker scrolls to keep its selection in view. Its click targets
/// have to scroll with it: counted from the top, a click on the highlighted
/// row picked another one.
#[test]
fn a_click_on_a_scrolled_picker_takes_the_row_under_it() {
    let mut app = TestApp::new();
    app.run_command("/agent").expect("/agent");
    for _ in 0..12 {
        app.press_key(KeyCode::Down).expect("down");
    }
    let wanted = app.modal_selection().expect("a selection");
    let rows = app.render_to_text(W, 24);
    let y = rows
        .iter()
        .position(|row| {
            row.contains(&format!(
                "› {}",
                enowx_core::agent_def::display_name(&wanted)
            ))
        })
        .unwrap_or_else(|| panic!("`{wanted}` is on screen: {rows:#?}")) as u16;
    app.mouse(click(60, y)).expect("click");
    assert_eq!(app.active_agent(), wanted);
}
