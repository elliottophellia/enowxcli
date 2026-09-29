//! The home screen: a new conversation opens on the mark and the
//! composer in the middle, with no sidebar, and the first message brings the
//! chat layout.

use enowx_tui::testing::TestApp;

const W: u16 = 100;
const H: u16 = 30;
/// A full row of the mark's grid: five cells two columns wide. Its top row
/// is the first one found.
const MARK: &str = "██ ██ ██ ██ ██";
/// The brand's orange, the mark's centre cell.
const BRAND: ratatui::style::Color = ratatui::style::Color::Rgb(255, 90, 54);

fn ready() -> TestApp {
    let mut app = TestApp::new();
    app.seed_provider(
        "fixture",
        "http://127.0.0.1:1/v1",
        "",
        "test-key",
        "deepseek-flash",
    );
    app
}

fn settled(mut app: TestApp) -> TestApp {
    app.home_at(10.0);
    app
}

/// Where `needle` first appears: (column, row), counted in characters.
fn find(rows: &[String], needle: &str) -> Option<(usize, usize)> {
    rows.iter().enumerate().find_map(|(y, row)| {
        row.find(needle)
            .map(|byte| (row[..byte].chars().count(), y))
    })
}

#[test]
fn a_new_conversation_opens_on_the_home_screen() {
    let mut app = settled(ready());
    let rows = app.render_to_text(W, H);
    assert!(app.is_home());
    assert!(app.side_area().is_none(), "no side column: {rows:#?}");
    assert!(find(&rows, MARK).is_some(), "the mark: {rows:#?}");
    let keys = rows.last().expect("the status bar");
    assert!(
        keys.contains("Ctrl+P"),
        "the keys stay where they are: {keys}"
    );
    assert!(
        !keys.contains("READY"),
        "the state moved to the middle: {keys}"
    );
}

/// The status bar's state, agent and model sit in the middle with the
/// composer, starting under its `❯`, rather than alone in the corner.
#[test]
fn the_state_agent_and_model_sit_under_the_composer() {
    let mut app = settled(ready());
    let rows = app.render_to_text(W, H);
    let (prompt_x, _) = find(&rows, "❯").expect("the composer's prompt");
    let (_, box_bottom) = find(&rows, "╰").expect("the composer's bottom edge");
    let under = &rows[box_bottom + 1];
    for part in ["READY", "Orchestrator", "deepseek-flash"] {
        assert!(under.contains(part), "{part}: {under}");
    }
    // The badge's own padding is a coloured space before the word.
    let (ready_x, _) = find(&rows[box_bottom + 1..box_bottom + 2], "READY").unwrap();
    assert_eq!(ready_x, prompt_x + 1, "{under}");
}

/// The mark and the composer share a centre, and the composer sits under
/// the mark, not at the bottom of the window.
#[test]
fn the_composer_sits_in_the_middle_under_the_mark() {
    let mut app = settled(ready());
    let rows = app.render_to_text(W, H);
    let (logo_x, logo_y) = find(&rows, MARK).expect("the mark");
    let (box_x, box_y) = find(&rows, "╭").expect("the composer");
    let box_right = rows[box_y].chars().count() - 1;
    let left = box_x;
    let right = W as usize - 1 - box_right;
    assert!(
        left.abs_diff(right) <= 1,
        "centred: {left} and {right} free"
    );
    assert!(box_y > logo_y, "under the mark");
    assert!(
        box_y < H as usize / 2 + 4,
        "in the middle, not at the bottom"
    );
    // The wordmark centred over the composer, to half a column: `enow` and
    // a gap, 42 columns, then the 14 of the mark.
    let logo_centre = (logo_x - 42) * 2 + 56;
    let box_centre = box_x + box_right;
    assert!(logo_centre.abs_diff(box_centre) <= 1, "{rows:#?}");
}

#[test]
fn the_first_message_brings_the_chat_layout() {
    let mut app = ready();
    app.push_user("build me a landing page");
    let _ = app.render_to_text(120, 34);
    assert!(!app.is_home());
    assert!(app.side_area().is_some(), "the side column is back");
}

#[test]
fn new_goes_back_home() {
    let mut app = ready();
    app.push_user("hi");
    app.push_assistant("hello");
    let _ = app.render_to_text(W, H);
    app.new_session();
    let rows = app.render_to_text(W, H);
    assert!(app.is_home());
    assert!(find(&rows, "❯").is_some());
}

#[test]
fn a_resumed_session_opens_in_the_chat_layout() {
    let mut app = ready();
    app.resume_with_switches(&[("user", "hi"), ("assistant", "hello")], &[])
        .expect("resume");
    let _ = app.render_to_text(120, 34);
    assert!(!app.is_home());
    assert!(app.side_area().is_some());
}

/// Without a model nothing can be sent, so the line under the composer
/// says what to set up instead of where the work will happen.
#[test]
fn what_to_set_up_is_said_under_the_composer() {
    let mut app = settled(TestApp::new());
    let rows = app.render_to_text(W, H);
    let (_, box_y) = find(&rows, "╰").expect("the composer's bottom edge");
    let under = rows[box_y + 1..box_y + 3].join("\n");
    assert!(under.contains("Choose a model · /model"), "{under}");
}

#[test]
fn a_ready_app_names_the_workspace_and_version() {
    let mut app = settled(ready());
    let rows = app.render_to_text(W, H);
    let (_, box_y) = find(&rows, "╰").expect("the composer's bottom edge");
    let under = rows[box_y + 1..box_y + 3].join("\n");
    assert!(under.contains("/ws"), "the workspace, by its end: {under}");
    assert!(
        under
            .trim_end()
            .ends_with(&format!("v{}", env!("CARGO_PKG_VERSION"))),
        "{under}"
    );
}

/// The command list opens below the composer, where there is room, rather
/// than over the mark.
#[test]
fn commands_open_under_the_composer() {
    let mut app = settled(ready());
    app.type_input("/m");
    let rows = app.render_to_text(W, H);
    let (_, box_bottom) = find(&rows, "╰").expect("the composer");
    let (_, list) = find(&rows, "COMMANDS").expect("the command list");
    assert_eq!(list, box_bottom + 1, "{rows:#?}");
    assert!(find(&rows, MARK).is_some(), "the mark stays");
}

/// A short window gives up the mark, never the composer.
#[test]
fn a_short_window_keeps_the_composer() {
    for (w, h) in [(60, 12), (30, 10), (20, 8)] {
        let mut app = settled(ready());
        let rows = app.render_to_text(w, h);
        assert!(find(&rows, MARK).is_none(), "{w}x{h}: {rows:#?}");
        assert!(find(&rows, "❯").is_some(), "{w}x{h}: {rows:#?}");
    }
}

/// The centre cell lights first, in the brand's orange, and stays lit.
#[test]
fn the_centre_lights_first_and_stays() {
    let mut app = settled(ready());
    let rows = app.render_to_text(W, H);
    let (x, y) = find(&rows, MARK).expect("the mark");
    let centre = (x as u16 + 6, y as u16 + 3);
    app.home_at(0.02);
    assert_ne!(
        app.cell_colours(W, H, centre.0, centre.1).1,
        BRAND,
        "not yet"
    );
    for at in [2.0, 3.8, 6.0] {
        app.home_at(at);
        assert_eq!(
            app.cell_colours(W, H, centre.0, centre.1).1,
            BRAND,
            "at {at}s"
        );
    }
}

/// Around it the X lights outward, holds, goes out and lights again: the
/// screen keeps a slow loop while it waits.
#[test]
fn the_x_lights_and_goes_out_in_a_loop() {
    let mut app = settled(ready());
    let rows = app.render_to_text(W, H);
    let (x, y) = find(&rows, MARK).expect("the mark");
    let corner = (x as u16, y as u16);
    let grid_cell = (x as u16 + 3, y as u16);
    let text = app.theme_colour("text");
    app.home_at(2.0);
    assert_eq!(app.cell_colours(W, H, corner.0, corner.1).1, text, "lit");
    let grid = app.cell_colours(W, H, grid_cell.0, grid_cell.1).1;
    assert_ne!(grid, text, "the rest of the grid stays faint");
    app.home_at(3.8);
    assert_eq!(app.cell_colours(W, H, corner.0, corner.1).1, grid, "out");
    app.home_at(6.0);
    assert_eq!(
        app.cell_colours(W, H, corner.0, corner.1).1,
        text,
        "lit again"
    );
}

/// Rows of nothing above the block, below it (to the keys), and columns of
/// nothing to its left and right.
fn gaps(rows: &[String], height: u16) -> (usize, usize, usize, usize) {
    let first = rows.iter().position(|row| !row.trim().is_empty()).unwrap();
    let keys = height as usize - 1;
    let last = (0..keys)
        .rev()
        .find(|&y| !rows[y].trim().is_empty())
        .unwrap();
    let (left, box_y) = find(rows, "╭").expect("the composer");
    let right = rows[box_y].chars().count() - 1;
    (first, keys - 1 - last, left, right)
}

/// The block sits a little above the middle, with the same columns on
/// either side, and the composer takes about three quarters of the width.
#[test]
fn the_block_sits_a_little_high_with_a_wide_composer() {
    for (w, h) in [(142, 27), (100, 30), (120, 36)] {
        let mut app = settled(ready());
        let rows = app.render_to_text(w, h);
        let (top, bottom, left, right_edge) = gaps(&rows, h);
        let right = w as usize - 1 - right_edge;
        assert!(
            top < bottom && bottom <= 2 * top + 2,
            "{w}x{h}: {top} above, {bottom} below"
        );
        assert_eq!(left, right, "{w}x{h}: {left} left, {right} right");
        let width = right_edge + 1 - left;
        assert!(
            width.abs_diff(w as usize * 3 / 4) <= 1,
            "{w}x{h}: {width} columns wide"
        );
    }
}

/// On a wide window the composer is wide too, and what sits under it fits
/// on one line: the state on the left, the version on the right.
#[test]
fn a_wide_window_gets_a_wide_composer_and_one_line_under_it() {
    let mut app = settled(ready());
    let rows = app.render_to_text(142, 27);
    let (left, box_y) = find(&rows, "╭").unwrap();
    let width = rows[box_y].chars().count() - left;
    assert!(width >= 100, "{width} columns: {rows:#?}");
    let (_, bottom) = find(&rows, "╰").unwrap();
    let under = &rows[bottom + 1];
    assert!(
        under.contains("READY") && under.contains("v0.1.0"),
        "{under}"
    );
    assert!(rows[bottom + 2].trim().is_empty(), "one line: {rows:#?}");
}

/// Floating on the window, the composer has no fill: a fill drew a square
/// block behind its rounded edge. In the chat layout it keeps the grid's.
#[test]
fn the_home_composer_has_no_fill() {
    let mut app = settled(ready());
    let rows = app.render_to_text(W, H);
    let (x, y) = find(&rows, "╭").unwrap();
    let inside = (x as u16 + 2, y as u16 + 1);
    let canvas = app.theme_colour("canvas");
    assert_eq!(app.cell_colours(W, H, inside.0, inside.1).2, canvas);
    assert_eq!(
        app.cell_colours(W, H, x as u16, y as u16).2,
        canvas,
        "the corner"
    );

    let mut chat = TestApp::in_conversation();
    let rows = chat.render_to_text(W, H);
    let (_, y) = find(&rows, "❯").unwrap();
    let subtle = chat.theme_colour("subtle");
    assert_eq!(chat.cell_colours(W, H, 1, y as u16).2, subtle, "{rows:#?}");
}

/// Empty, the composer says what it is for; typing replaces that.
#[test]
fn the_empty_composer_says_what_it_is_for() {
    let mut app = settled(ready());
    let hint = "Ask, or type / for commands";
    assert!(find(&app.render_to_text(W, H), hint).is_some());
    app.type_input("fix the login bug");
    let rows = app.render_to_text(W, H);
    assert!(find(&rows, hint).is_none(), "{rows:#?}");
    assert!(find(&rows, "fix the login bug").is_some());
}

/// While a turn runs, the mark's middle row turns under the composer, with
/// what the turn is doing; the line goes when the turn ends.
#[test]
fn a_running_turn_shows_the_mark_under_the_composer() {
    let mut app = TestApp::in_conversation();
    let rows = app.render_to_text(W, H);
    assert!(find(&rows, "Esc to stop").is_none(), "{rows:#?}");
    let _cancel = app.start_fake_turn();
    let rows = app.render_to_text(W, H);
    let (_, y) = find(&rows, "Esc to stop").expect("the working line");
    let (_, composer) = find(&rows, "❯").expect("the composer");
    assert_eq!(y, composer + 2, "right under the composer: {rows:#?}");
    let line = &rows[y];
    assert!(line.contains("■ ■ ■ ■ ■") && line.contains("0s"), "{line}");
    app.deliver_done("stop");
    let rows = app.render_to_text(W, H);
    assert!(find(&rows, "Esc to stop").is_none(), "{rows:#?}");
}
