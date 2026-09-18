//! Errors collapse into a single block instead of stacking. A provider that
//! is down, a rejected key, or a retry loop otherwise repeats the same line
//! until the real conversation is pushed off screen.

use enowx_tui::testing::TestApp;

const W: u16 = 80;
const H: u16 = 24;

#[test]
fn repeated_identical_errors_do_not_stack() {
    let mut app = TestApp::new();
    for _ in 0..5 {
        app.push_error("Request failed: connection refused");
    }
    assert_eq!(
        app.error_block_count(),
        1,
        "the same error repeated should occupy one block, not five"
    );
}

/// Collapsing must not hide that it is still happening.
#[test]
fn a_repeat_count_is_shown() {
    let mut app = TestApp::new();
    for _ in 0..4 {
        app.push_error("Request failed: connection refused");
    }
    let rows = app.render_to_text(W, H);
    assert!(
        rows.iter().any(|r| r.contains("error ×4")),
        "the transcript should say the failure repeated, got {rows:?}"
    );
}

/// A single failure reads as "error", not "error ×1".
#[test]
fn one_error_has_no_counter() {
    let mut app = TestApp::new();
    app.push_error("Something went wrong");
    let rows = app.render_to_text(W, H);
    assert!(rows.iter().any(|r| r.contains("error")));
    assert!(
        !rows.iter().any(|r| r.contains('×')),
        "a lone error needs no multiplier, got {rows:?}"
    );
}

/// The newest failure is the one worth reading; the previous one is usually
/// its cause rather than separate news.
#[test]
fn a_different_error_replaces_rather_than_appends() {
    let mut app = TestApp::new();
    app.push_error("connection refused");
    app.push_error("401 unauthorized");
    assert_eq!(app.error_block_count(), 1, "still one error block");
    let rows = app.render_to_text(W, H);
    assert!(
        rows.iter().any(|r| r.contains("401 unauthorized")),
        "the newest message should be the visible one, got {rows:?}"
    );
    assert!(
        !rows.iter().any(|r| r.contains("connection refused")),
        "the superseded message should be gone, got {rows:?}"
    );
}

/// An error belongs to the moment it happened. Once other output follows it,
/// a later failure is a separate event and gets its own block.
#[test]
fn an_error_after_other_output_starts_a_new_block() {
    let mut app = TestApp::new();
    app.push_error("first failure");
    app.push_assistant("here is an answer");
    app.push_error("second failure");
    assert_eq!(
        app.error_block_count(),
        2,
        "errors separated by real output are separate events"
    );
    let rows = app.render_to_text(W, H);
    assert!(rows.iter().any(|r| r.contains("first failure")));
    assert!(rows.iter().any(|r| r.contains("second failure")));
}

/// The counter tracks the run in progress, not the lifetime total.
#[test]
fn the_counter_resets_after_intervening_output() {
    let mut app = TestApp::new();
    for _ in 0..3 {
        app.push_error("flaky");
    }
    app.push_assistant("recovered");
    app.push_error("flaky");
    let rows = app.render_to_text(W, H);
    assert!(
        !rows.iter().any(|r| r.contains("error ×4")),
        "the new run starts from one, got {rows:?}"
    );
}

#[test]
fn errors_stay_red() {
    let mut app = TestApp::new();
    app.push_error("connection refused");
    let styled = app.render_to_styled(W, H);
    assert!(
        styled.iter().any(|(text, red)| text.contains("error") && *red),
        "the error label must render in the theme's red"
    );
}

/// Collapsing changes block count, which the render cache keys on; the count
/// shown must follow the state rather than serving a stale frame.
#[test]
fn the_rendered_count_tracks_further_repeats() {
    let mut app = TestApp::new();
    app.push_error("boom");
    let _ = app.render_to_text(W, H);
    app.push_error("boom");
    let rows = app.render_to_text(W, H);
    assert!(
        rows.iter().any(|r| r.contains("error ×2")),
        "a further repeat must re-render the block, got {rows:?}"
    );
}

/// The real spam source: the provider's backoff loop emits a near-identical
/// message per attempt. Ten attempts must not become ten transcript blocks.
#[test]
fn a_backoff_sequence_occupies_one_block() {
    let mut app = TestApp::new();
    let msg = "upstream error: provider stream ended without a finish reason; no tools executed";
    for attempt in 2..=10 {
        app.push_retry(msg, attempt, 10);
    }
    assert_eq!(
        app.retry_block_count(),
        1,
        "nine retry notices should collapse into one line"
    );
}

#[test]
fn the_retry_line_shows_progress_through_the_budget() {
    let mut app = TestApp::new();
    for attempt in 2..=6 {
        app.push_retry("connection refused", attempt, 10);
    }
    let rows = app.render_to_text(W, H);
    assert!(
        rows.iter().any(|r| r.contains("retry 6/10")),
        "the latest attempt should be visible, got {rows:?}"
    );
    assert!(
        !rows.iter().any(|r| r.contains("retry 5/10")),
        "earlier attempts should have been replaced, got {rows:?}"
    );
}

#[test]
fn retries_are_red_not_plain_text() {
    let mut app = TestApp::new();
    app.push_retry("connection refused", 2, 10);
    let styled = app.render_to_styled(W, H);
    assert!(
        styled.iter().any(|(text, red)| text.contains("retry") && *red),
        "a retry must read as a failure, not as ordinary output"
    );
}

/// When the retries run out, the terminal error should take over the block
/// rather than leaving a stale retry line above it.
#[test]
fn a_final_error_replaces_the_retry_line() {
    let mut app = TestApp::new();
    for attempt in 2..=10 {
        app.push_retry("connection refused", attempt, 10);
    }
    app.push_error("Request failed after 10 attempts");
    assert_eq!(app.retry_block_count(), 0, "the retry line is superseded");
    assert_eq!(app.error_block_count(), 1, "one error remains");
    let rows = app.render_to_text(W, H);
    assert!(rows.iter().any(|r| r.contains("Request failed after 10")));
    assert!(
        !rows.iter().any(|r| r.contains("retry")),
        "no stale retry line should be left behind, got {rows:?}"
    );
}

/// A retry that eventually succeeds still leaves its line; the next real
/// output must not be swallowed by the collapse.
#[test]
fn output_after_a_retry_is_not_absorbed() {
    let mut app = TestApp::new();
    app.push_retry("connection refused", 2, 10);
    app.push_assistant("Recovered, here is the answer.");
    let rows = app.render_to_text(W, H);
    assert!(rows.iter().any(|r| r.contains("Recovered")));
    assert!(rows.iter().any(|r| r.contains("retry")));
}
