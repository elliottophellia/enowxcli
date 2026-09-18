//! The render cache must be invisible: a cached frame has to match what a
//! cold render would have produced. These tests compare the two directly,
//! because a stale cache shows up as wrong pixels rather than as a panic.

use enowx_tui::testing::TestApp;

const W: u16 = 80;
const H: u16 = 24;

/// Render twice — once warm, once after dropping the cache — and require the
/// two to agree.
fn assert_cache_matches_cold(app: &mut TestApp, context: &str) {
    let warm = app.render_to_text(W, H);
    app.clear_render_cache();
    let cold = app.render_to_text(W, H);
    assert_eq!(
        warm, cold,
        "cached render diverged from cold render: {context}"
    );
}

#[test]
fn streaming_into_the_last_block_keeps_the_frame_correct() {
    let mut app = TestApp::new();
    app.push_user("refactor the parser");
    app.push_assistant("Working on it.");
    let _ = app.render_to_text(W, H);

    // Each delta is what a streamed token does to the transcript.
    for delta in [" First", " I will", " read the file."] {
        app.append_to_last(delta);
        assert_cache_matches_cold(&mut app, "mid-stream");
    }
}

#[test]
fn appending_a_block_does_not_disturb_the_ones_above() {
    let mut app = TestApp::new();
    app.push_user("one");
    app.push_assistant("first answer");
    let _ = app.render_to_text(W, H);

    app.push_user("two");
    app.push_assistant("second answer");
    assert_cache_matches_cold(&mut app, "after appending blocks");
}

#[test]
fn a_width_change_invalidates_every_block() {
    let mut app = TestApp::new();
    app.push_user("a line long enough that the wrap point depends on the width");
    app.push_assistant("| col | col |\n| --- | --- |\n| a | b |");

    let narrow = app.render_to_text(60, H);
    let wide = app.render_to_text(100, H);
    assert_ne!(
        narrow, wide,
        "a resize must actually re-lay-out the transcript"
    );

    // Back to the original width: must equal a cold render at that width.
    let narrow_again = app.render_to_text(60, H);
    app.clear_render_cache();
    let narrow_cold = app.render_to_text(60, H);
    assert_eq!(
        narrow_again, narrow_cold,
        "returning to a previous width must not serve a stale layout"
    );
}

#[test]
fn toggling_reasoning_visibility_re_renders() {
    let mut app = TestApp::new();
    app.push_user("question");
    app.push_reasoning("a private thought");
    app.push_assistant("the answer");

    app.set_show_reasoning(false);
    let hidden = app.render_to_text(W, H);
    assert!(
        !hidden.iter().any(|l| l.contains("private thought")),
        "reasoning must be hidden when the toggle is off"
    );

    app.set_show_reasoning(true);
    let shown = app.render_to_text(W, H);
    assert!(
        shown.iter().any(|l| l.contains("private thought")),
        "toggling reasoning on must reveal it rather than serve the cached frame"
    );
    assert_cache_matches_cold(&mut app, "reasoning shown");

    app.set_show_reasoning(false);
    assert_cache_matches_cold(&mut app, "reasoning hidden again");
}

#[test]
fn markdown_blocks_survive_a_round_trip_through_the_cache() {
    let mut app = TestApp::new();
    app.push_assistant(
        "Here is code:\n\n```rust\nfn main() { println!(\"hi\"); }\n```\n\nAnd a list:\n\n- one\n- two\n",
    );
    assert_cache_matches_cold(&mut app, "markdown with a fenced block");
}

#[test]
fn every_block_gets_cached() {
    let mut app = TestApp::new();
    app.push_user("one");
    app.push_assistant("two");
    app.push_user("three");
    let _ = app.render_to_text(W, H);
    assert_eq!(
        app.cached_block_count(),
        3,
        "each transcript block should hold a cached rendering after a draw"
    );
}

/// The viewport optimisation only materialises the visible rows, so scrolled
/// frames are the case most likely to drift. A cached scrolled frame must
/// still match the cold one.
#[test]
fn scrolled_frames_match_a_cold_render() {
    let mut app = TestApp::new();
    for i in 0..40 {
        app.push_user(&format!("question {i}"));
        app.push_assistant(&format!(
            "answer {i} with a bit of text to take up a row or two"
        ));
    }
    let _ = app.render_to_text(W, H);
    let max = app.max_scroll();
    assert!(max > 0, "the fixture must be taller than the viewport");

    for row in [0u16, max / 3, max / 2, max] {
        app.scroll_to(row);
        assert_cache_matches_cold(&mut app, &format!("scrolled to row {row}"));
    }
}

/// Distinct scroll positions must show distinct content — a window that
/// ignored the offset would pass the equality test above while showing the
/// same rows every time.
#[test]
fn scrolling_actually_changes_what_is_shown() {
    let mut app = TestApp::new();
    for i in 0..40 {
        app.push_assistant(&format!("unique marker line {i}"));
    }
    let _ = app.render_to_text(W, H);
    let max = app.max_scroll();

    app.scroll_to(0);
    let top = app.render_to_text(W, H);
    app.scroll_to(max);
    let bottom = app.render_to_text(W, H);

    assert_ne!(top, bottom, "scrolling must move the window");
    assert!(
        top.iter().any(|l| l.contains("marker line 0")),
        "the top of the transcript must show the first block"
    );
    assert!(
        bottom.iter().any(|l| l.contains("marker line 39")),
        "the bottom must show the last block"
    );
}

/// Tool headers are clickable, and the click handler compares against screen
/// rows. If the marker rebasing were off by the scroll offset, clicks would
/// land on the wrong block.
#[test]
fn tool_header_rows_track_the_scroll_offset() {
    let mut app = TestApp::new();
    for i in 0..30 {
        app.push_assistant(&format!("filler {i}"));
    }
    app.push_tool("tool-1", "bash", r#"{"command":"ls -la"}"#, "a\nb\nc");
    let _ = app.render_to_text(W, H);

    app.scroll_to(app.max_scroll());
    let _ = app.render_to_text(W, H);
    let rows = app.tool_header_rows();
    assert!(
        rows.iter().any(|(_, id)| id == "tool-1"),
        "a visible tool header must register a clickable row, got {rows:?}"
    );

    // Scrolled to the very top the tool is far below the fold, so it must not
    // claim a row on screen.
    app.scroll_to(0);
    let _ = app.render_to_text(W, H);
    let rows = app.tool_header_rows();
    assert!(
        !rows.iter().any(|(_, id)| id == "tool-1"),
        "an off-screen tool header must not register a clickable row, got {rows:?}"
    );
}

/// A tool result can be megabytes. The cache key must not read all of it on
/// every frame, or the cost of the cache scales with the output it exists to
/// avoid re-rendering — which is exactly what it did before the key was
/// changed to hash the result's shape.
#[test]
fn a_huge_tool_result_does_not_slow_the_frame() {
    use std::time::Instant;

    fn frame_cost(calls: usize, payload: &str) -> std::time::Duration {
        let mut app = TestApp::new();
        for i in 0..calls {
            app.push_tool(&format!("t{i}"), "grep", r#"{"pattern":"x"}"#, payload);
        }
        let _ = app.render_to_text(100, 40);
        let start = Instant::now();
        for _ in 0..20 {
            let _ = app.render_to_text(100, 40);
        }
        start.elapsed() / 20
    }

    let payload: String = (0..2_000).map(|i| format!("src/a.rs:{i}:hit\n")).collect();
    let few = frame_cost(10, &payload);
    let many = frame_cost(200, &payload);

    // Twenty times the payload must not cost anything like twenty times the
    // frame. A generous bound keeps this from flaking on a loaded machine
    // while still failing loudly if the key goes back to reading every byte.
    assert!(
        many < few * 6,
        "frame cost should stay near-flat as results pile up: {few:?} -> {many:?}"
    );
}

/// Appending to a running tool's output must still invalidate its row, or a
/// streaming result would freeze at whatever was first cached.
#[test]
fn a_growing_tool_result_still_re_renders() {
    let mut app = TestApp::new();
    app.push_tool("t1", "bash", r#"{"command":"cargo test"}"#, "exit 0\nfirst");
    let before = app.render_to_text(W, H);
    app.push_tool(
        "t1b",
        "bash",
        r#"{"command":"cargo test"}"#,
        "exit 0\nfirst\nsecond",
    );
    let after = app.render_to_text(W, H);
    assert_ne!(
        before, after,
        "a longer result must produce a different frame"
    );
}
