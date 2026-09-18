//! Direct tests that `App::mouse` dispatches clicks and wheel events into the
//! right handler. These bypass the terminal so the assertions are exact.

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use enowx_tui::testing::TestApp;

fn click(col: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

fn wheel(down: bool, col: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: if down {
            MouseEventKind::ScrollDown
        } else {
            MouseEventKind::ScrollUp
        },
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

/// Transcript wheel scrolling is deliberately NOT throttled — the throttle
/// exists only for popup selectors, where a trackpad burst would otherwise
/// skip past every row (see `wheel_inside_popup_moves_cursor_not_transcript`).
/// Each event moves three lines, matching the file-wide scale where PgUp/PgDn
/// move ten.
///
/// This test previously asserted one line per event with an 80 ms throttle,
/// which the transcript path has never done; it failed from the first commit
/// and forced a 200 ms sleep into the suite to test a throttle that was not
/// on this path.
#[test]
fn scroll_wheel_moves_transcript() {
    let mut app = TestApp::new();
    app.set_max_scroll(20);

    app.mouse(wheel(true, 10, 5)).unwrap();
    assert_eq!(app.scroll(), 3, "one wheel step scrolls three lines");

    app.mouse(wheel(true, 10, 5)).unwrap();
    assert_eq!(app.scroll(), 6, "a second step scrolls three more");

    app.mouse(wheel(false, 10, 5)).unwrap();
    assert_eq!(app.scroll(), 3, "scroll-up rewinds by the same step");
}

/// Back-to-back events must all land: throttling the transcript would drop
/// events from a trackpad flick and make scrolling feel stuck.
#[test]
fn rapid_wheel_events_are_not_throttled_on_the_transcript() {
    let mut app = TestApp::new();
    app.set_max_scroll(100);
    for _ in 0..5 {
        app.mouse(wheel(true, 10, 5)).unwrap();
    }
    assert_eq!(
        app.scroll(),
        15,
        "five immediate wheel events should all take effect"
    );
}

/// Scrolling stops at the ends rather than wrapping or saturating past them.
#[test]
fn wheel_scrolling_clamps_at_both_ends() {
    let mut app = TestApp::new();
    app.set_max_scroll(4);
    for _ in 0..10 {
        app.mouse(wheel(true, 10, 5)).unwrap();
    }
    assert_eq!(app.scroll(), 4, "cannot scroll past the last line");

    for _ in 0..10 {
        app.mouse(wheel(false, 10, 5)).unwrap();
    }
    assert_eq!(app.scroll(), 0, "cannot scroll above the first line");
}

/// Reaching the bottom re-arms follow-the-tail, so a new reply keeps the view
/// pinned; scrolling up again releases it.
#[test]
fn scrolling_to_the_bottom_resumes_auto_follow() {
    let mut app = TestApp::new();
    app.set_max_scroll(6);

    app.mouse(wheel(false, 10, 5)).unwrap();
    assert!(!app.auto_scroll(), "scrolling up detaches from the tail");

    for _ in 0..5 {
        app.mouse(wheel(true, 10, 5)).unwrap();
    }
    assert_eq!(app.scroll(), 6);
    assert!(
        app.auto_scroll(),
        "landing on the last line should follow new output again"
    );
}

#[test]
fn click_on_skill_mark_toggles_persisted() {
    let mut app = TestApp::new_with_skills(&["alpha", "beta"]);
    let idx = app.skill_index("alpha").expect("alpha discovered");
    app.set_popup_rows(vec![(Rect::new(10, 5, 70, 1), Rect::new(10, 5, 5, 1), idx)]);
    app.enter_skills_modal();
    app.mouse(click(11, 5)).unwrap();
    assert!(
        app.config_disabled_skills().contains(&"alpha".to_string()),
        "mark click should have added `alpha`; got {:?}",
        app.config_disabled_skills()
    );
}

#[test]
fn click_on_skill_body_selects_only() {
    let mut app = TestApp::new_with_skills(&["alpha"]);
    let idx = app.skill_index("alpha").expect("alpha discovered");
    app.set_popup_rows(vec![(Rect::new(10, 5, 70, 1), Rect::new(10, 5, 5, 1), idx)]);
    app.enter_skills_modal();
    app.mouse(click(40, 5)).unwrap();
    // Popup must stay open (select-only); nothing should have been dumped
    // into the transcript.
    assert!(app.is_modal_open(), "body click must keep popup open");
    assert!(
        !app.transcript_contains("alpha"),
        "body click must not push anything to transcript"
    );
    assert_eq!(app.modal_cursor(), idx, "cursor should move to clicked row");
}

#[test]
fn click_on_mcp_form_field_focuses_it() {
    let mut app = TestApp::new();
    app.enter_mcp_form();
    app.set_mcp_field_rows(vec![
        (Rect::new(10, 5, 60, 1), 0),
        (Rect::new(10, 6, 60, 1), 1),
        (Rect::new(10, 7, 60, 1), 2),
    ]);
    app.mouse(click(30, 7)).unwrap();
    assert_eq!(
        app.mcp_field(),
        2,
        "clicking row 2 must focus field index 2"
    );
}

#[test]
fn wheel_inside_popup_moves_cursor_not_transcript() {
    let mut app = TestApp::new_with_skills(&["a", "b", "c"]);
    app.set_popup_rows(vec![
        (Rect::new(10, 5, 70, 1), Rect::new(10, 5, 5, 1), 0),
        (Rect::new(10, 6, 70, 1), Rect::new(10, 6, 5, 1), 1),
        (Rect::new(10, 7, 70, 1), Rect::new(10, 7, 5, 1), 2),
    ]);
    app.set_popup_body(Rect::new(10, 5, 70, 5));
    app.enter_skills_modal();
    app.set_max_scroll(20);
    let before = app.scroll();
    app.mouse(wheel(true, 30, 6)).unwrap();
    assert_eq!(
        app.scroll(),
        before,
        "popup wheel must not scroll transcript"
    );
    assert_eq!(
        app.modal_cursor(),
        1,
        "popup wheel-down advances the cursor"
    );
}
