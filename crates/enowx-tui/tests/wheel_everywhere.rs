//! The mouse wheel works wherever the pointer is.
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

#[test]
fn the_wheel_moves_the_command_palette_not_the_transcript() {
    let mut app = TestApp::new();
    app.set_max_scroll(20);
    app.run_command("/commands").expect("/commands");
    let _ = app.render_to_text(W, H);
    let first = app.palette_selection();
    // Anywhere on screen: the open window has the focus.
    step(&mut app, true, 5, 5);
    assert_eq!(app.scroll(), 0, "the transcript behind must not move");
    assert_ne!(app.palette_selection(), first, "the selection moves");
    step(&mut app, false, 5, 5);
    assert_eq!(app.palette_selection(), first, "and back");
}

#[test]
fn the_wheel_moves_a_picker() {
    let mut app = TestApp::new();
    app.run_command("/theme").expect("/theme");
    let first = app.modal_selection();
    step(&mut app, true, 60, 10);
    assert_ne!(app.modal_selection(), first);
}

#[test]
fn the_wheel_moves_between_settings_fields() {
    let mut app = TestApp::new();
    app.open_custom_provider_form();
    let first = app.modal_cursor();
    step(&mut app, true, 60, 10);
    assert_eq!(app.modal_cursor(), first + 1);
}

/// A burst of wheel events from one trackpad flick moves a list by one row,
/// not by however many events the flick sent.
#[test]
fn a_burst_moves_a_list_one_row() {
    let mut app = TestApp::new();
    app.run_command("/commands").expect("/commands");
    app.let_the_wheel_settle();
    for _ in 0..5 {
        app.mouse(wheel(true, 5, 5)).expect("wheel");
    }
    assert_eq!(app.palette_selection().as_deref(), Some("resume"));
}

#[test]
fn the_wheel_pages_the_side_column() {
    let mut app = TestApp::in_conversation();
    for n in 0..80 {
        app.deliver_trimmed(&format!("tool-{n}"), 9000, 800);
    }
    app.select_sidebar_tab(3);
    let _ = app.render_to_text(W, H);
    let (page, pages) = app.sidebar_page();
    assert!(pages > 1, "the log should need more than one page");
    let (x, y, w, h) = app.side_area().expect("a side column");
    step(&mut app, true, x + w / 2, y + h - 3);
    let _ = app.render_to_text(W, H);
    assert_eq!(app.sidebar_page().0, page + 1, "down turns the page");
    step(&mut app, false, x + w / 2, y + h - 3);
    let _ = app.render_to_text(W, H);
    assert_eq!(app.sidebar_page().0, page, "up turns it back");
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
