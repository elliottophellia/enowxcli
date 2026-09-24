//! The LOGS tab: what the harness did, as opposed to what was said.
//!
//! The transcript carries the conversation. It does not say that a turn was
//! handed to another agent, that a request was retried, or how long a model
//! call took — and those are what answer "why did it stop there?" once the
//! status line, which showed them for a moment, has moved on.

use crossterm::event::KeyCode;
use enowx_tui::testing::TestApp;

const LOGS_TAB: usize = 4;

/// The sidebar's half of each row.
///
/// Taken by column rather than by splitting on `│`: the sidebar is a box with
/// borders of its own, so counting rules in from the right lands inside it or
/// past it depending on the row. The column is found from the box's titled top
/// edge, counted in chars — `find` gives a byte offset, and the frame's
/// multi-byte glyphs put the two well apart by this point in the row.
fn logs(app: &mut TestApp) -> String {
    app.select_sidebar_tab(LOGS_TAB);
    let rows = app.render_to_text(120, 34);
    let start = rows
        .iter()
        .find_map(|row| {
            let at = row.find("╭─ ")?;
            Some(row[..at].chars().count())
        })
        .filter(|at| *at > 4)
        .expect("the sidebar's titled top edge");
    rows.into_iter()
        .map(|row| row.chars().skip(start).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_fifth_tab_is_logs() {
    let mut app = TestApp::new();
    app.select_sidebar_tab(LOGS_TAB);
    let text = app.render_to_text(120, 34).join("\n");
    assert!(text.contains("LOGS"), "the tab should be named: {text}");
    assert!(
        !text.contains("BOUNDARIES"),
        "the config tab it replaced should be gone: {text}"
    );
}

#[test]
fn an_empty_session_says_so() {
    let mut app = TestApp::new();
    assert!(
        logs(&mut app).contains("belum ada yang tercatat"),
        "an empty log should say it is empty rather than looking broken"
    );
}

/// A handoff is the event most worth recording: it is one line in the
/// transcript and it changes who is answering.
#[test]
fn a_handoff_is_logged() {
    let mut app = TestApp::new();
    app.switch_agent("fe", "this is frontend work");
    let text = logs(&mut app);
    assert!(text.contains("fe"), "the agent taken over by: {text}");
    assert!(text.contains('→'), "marked as routing: {text}");
}

#[test]
fn delegations_are_logged_from_start_to_finish() {
    let mut app = TestApp::new();
    app.deliver_delegation_started("review", "check the page", "id-1");
    let text = logs(&mut app);
    assert!(text.contains("delegate"), "the start: {text}");
    assert!(text.contains("review"), "naming the agent: {text}");
    // The task is on the line too, but the sidebar is narrow enough to clip
    // it; what has to survive is which agent and that it was a delegation.

    app.deliver_delegation_finished("review", "id-1", false);
    let text = logs(&mut app);
    assert!(text.contains("review finished"), "and the end: {text}");
}

#[test]
fn a_failed_delegation_is_logged_as_failed() {
    let mut app = TestApp::new();
    app.deliver_delegation_started("fe", "write it", "id-1");
    app.deliver_delegation_finished("fe", "id-1", true);
    assert!(
        logs(&mut app).contains("fe failed"),
        "a failure should read as one"
    );
}

#[test]
fn a_trim_is_logged() {
    let mut app = TestApp::new();
    app.deliver_trimmed("bash", 4876, 620);
    assert!(logs(&mut app).contains("trimmed bash"));
}

/// The sizes belong under the line, not in it: the summary answers "what
/// happened" and the detail answers "by how much".
#[test]
fn detail_is_hidden_until_asked_for() {
    let mut app = TestApp::new();
    app.deliver_trimmed("bash", 4876, 620);
    assert!(
        !logs(&mut app).contains("4,876"),
        "the numbers are noise by default"
    );

    app.press(KeyCode::F(7), false).expect("F7");
    assert!(
        logs(&mut app).contains("4,876"),
        "and available when asked for"
    );

    app.press(KeyCode::F(7), false).expect("F7 again");
    assert!(
        !logs(&mut app).contains("4,876"),
        "F7 toggles rather than only turning on"
    );
}

/// Filtering is what makes a long log usable: a session with two hundred
/// model calls in it has the one handoff buried.
#[test]
fn the_filter_narrows_to_one_kind() {
    let mut app = TestApp::new();
    app.switch_agent("fe", "frontend work");
    app.deliver_trimmed("bash", 4876, 620);
    let all = logs(&mut app);
    assert!(all.contains("fe") && all.contains("trimmed bash"), "{all}");

    // F6 steps to the first filter: agents only.
    app.press(KeyCode::F(6), false).expect("F6");
    let agents = logs(&mut app);
    assert!(
        agents.contains("agents"),
        "it should name the filter: {agents}"
    );
    assert!(agents.contains("fe"), "the handoff stays: {agents}");
    assert!(
        !agents.contains("trimmed bash"),
        "and the trim goes: {agents}"
    );
}

/// Cycling has to come back round, or a filter is a trap.
#[test]
fn the_filter_cycles_back_to_everything() {
    let mut app = TestApp::new();
    app.deliver_trimmed("bash", 4876, 620);
    for _ in 0..5 {
        app.press(KeyCode::F(6), false).expect("F6");
    }
    let text = logs(&mut app);
    // The unfiltered heading names no filter: "N baris log" rather than
    // "N baris · saring <kind>".
    assert!(text.contains("baris log"), "back to everything: {text}");
    assert!(text.contains("trimmed bash"), "and showing it: {text}");
}

/// F6 and F7 are useless if the user is looking at another tab.
#[test]
fn the_filter_keys_bring_the_tab_forward() {
    let mut app = TestApp::new();
    app.select_sidebar_tab(0);
    app.press(KeyCode::F(6), false).expect("F6");
    let text = app.render_to_text(120, 34).join("\n");
    assert!(
        text.contains("F6 saring"),
        "F6 should show the log it just filtered: {text}"
    );
}

/// An error is the line someone opens this tab to find.
#[test]
fn errors_are_logged() {
    let mut app = TestApp::new();
    app.deliver_error("every model tier failed");
    assert!(logs(&mut app).contains("every model tier failed"));
}
