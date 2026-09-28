//! The home screen: a new conversation opens on the wordmark and the
//! composer in the middle, with no sidebar, and the first message brings the
//! chat layout.

use enowx_tui::testing::TestApp;

const W: u16 = 100;
const H: u16 = 30;
/// The top row of `enow`, which only the wordmark draws.
const LETTERS: &str = "▄█▀▀█▄ ██▀▀█▄ ▄█▀▀█▄";

fn ready() -> TestApp {
    let mut app = TestApp::new();
    app.seed_provider(
        "deepseek",
        "deepseek",
        "http://127.0.0.1:1/v1",
        "",
        "test-key",
        "deepseek-flash",
        128_000,
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
    assert!(find(&rows, LETTERS).is_some(), "the wordmark: {rows:#?}");
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

/// The wordmark and the composer share a centre, and the composer sits
/// under the wordmark, not at the bottom of the window.
#[test]
fn the_composer_sits_in_the_middle_under_the_wordmark() {
    let mut app = settled(ready());
    let rows = app.render_to_text(W, H);
    let (logo_x, logo_y) = find(&rows, LETTERS).expect("the wordmark");
    let (box_x, box_y) = find(&rows, "╭").expect("the composer");
    let box_right = rows[box_y].chars().count() - 1;
    let left = box_x;
    let right = W as usize - 1 - box_right;
    assert!(
        left.abs_diff(right) <= 1,
        "centred: {left} and {right} free"
    );
    assert!(box_y > logo_y, "under the wordmark");
    assert!(
        box_y < H as usize / 2 + 4,
        "in the middle, not at the bottom"
    );
    // The wordmark centred over the composer, to half a column.
    let logo_centre = logo_x * 2 + 41;
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
/// than over the wordmark.
#[test]
fn commands_open_under_the_composer() {
    let mut app = settled(ready());
    app.type_input("/m");
    let rows = app.render_to_text(W, H);
    let (_, box_bottom) = find(&rows, "╰").expect("the composer");
    let (_, list) = find(&rows, "COMMANDS").expect("the command list");
    assert_eq!(list, box_bottom + 1, "{rows:#?}");
    assert!(find(&rows, LETTERS).is_some(), "the wordmark stays");
}

/// A short window gives up the wordmark, never the composer.
#[test]
fn a_short_window_keeps_the_composer() {
    for (w, h) in [(60, 12), (30, 10), (20, 8)] {
        let mut app = settled(ready());
        let rows = app.render_to_text(w, h);
        assert!(find(&rows, LETTERS).is_none(), "{w}x{h}: {rows:#?}");
        assert!(find(&rows, "❯").is_some(), "{w}x{h}: {rows:#?}");
    }
}

/// The letters fade in during the opening and are the text colour after it.
#[test]
fn the_letters_fade_in() {
    let mut app = settled(ready());
    let rows = app.render_to_text(W, H);
    let (x, y) = find(&rows, LETTERS).expect("the wordmark");
    // The `█` after the first `▄`: both of its pixels are the `e`.
    let (x, y) = (x as u16 + 1, y as u16);
    let text = app.theme_colour("text");
    assert_eq!(app.cell_colours(W, H, x, y).1, text, "settled");
    app.home_at(0.05);
    let (symbol, fg, _) = app.cell_colours(W, H, x, y);
    assert!(
        symbol.trim().is_empty() || fg != text,
        "still fading in at 0.05s: {symbol:?} {fg:?}"
    );
}

/// Once the opening is over, only the dot where the strokes cross moves.
#[test]
fn the_dot_breathes() {
    let mut app = settled(ready());
    let rows = app.render_to_text(W, H);
    let (_, y) = find(&rows, LETTERS).expect("the wordmark");
    let x = rows[y].chars().count() - 1;
    let (x, y) = (x as u16, y as u16);
    app.home_at(10.0);
    let (symbol, one, _) = app.cell_colours(W, H, x, y);
    assert_eq!(symbol, "█", "the dot");
    app.home_at(11.2);
    let (_, other, _) = app.cell_colours(W, H, x, y);
    assert_ne!(one, other, "half a breath later it has changed");
    let letter = find(&rows, LETTERS).map(|(x, _)| x as u16 + 1).unwrap();
    app.home_at(10.0);
    let still = app.cell_colours(W, H, letter, y).1;
    app.home_at(11.2);
    assert_eq!(
        app.cell_colours(W, H, letter, y).1,
        still,
        "the letters stay"
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

/// The block sits in the middle with the same gap on every side as it looks
/// on screen: as many rows above as below, as many columns left as right,
/// and twice as many columns as rows, a cell being twice as tall as wide.
#[test]
fn the_gap_is_the_same_on_every_side() {
    for (w, h) in [(142, 27), (100, 30), (120, 36)] {
        let mut app = settled(ready());
        let rows = app.render_to_text(w, h);
        let (top, bottom, left, right_edge) = gaps(&rows, h);
        let right = w as usize - 1 - right_edge;
        assert!(
            top.abs_diff(bottom) <= 1,
            "{w}x{h}: {top} above, {bottom} below"
        );
        assert_eq!(left, right, "{w}x{h}: {left} left, {right} right");
        assert!(
            left.abs_diff(2 * top) <= 2,
            "{w}x{h}: {left} columns at the sides against {top} rows above"
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
