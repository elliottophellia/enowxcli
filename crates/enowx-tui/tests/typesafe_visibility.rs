//! Seeing whether TypeSafe is doing anything.
//!
//! Trimming removes text from what the model is sent. A feature that does
//! that and shows nothing for it cannot be told apart from one that is
//! broken, or from one whose key was rejected — every failure path falls back
//! to the old behaviour, so nothing looks wrong either way.

use enowx_tui::testing::TestApp;

#[test]
fn a_trim_is_counted() {
    let mut app = TestApp::new();
    assert_eq!(app.trimmed_count(), 0);
    app.deliver_trimmed("bash", 4876, 620);
    app.deliver_trimmed("glob", 3818, 615);
    assert_eq!(app.trimmed_count(), 2);
    assert_eq!(
        app.trimmed_saved(),
        (4876 - 620) + (3818 - 615),
        "the saving is what was removed, not what was kept"
    );
}

/// The status line names the tool and the sizes, so a trim that looks wrong
/// can be traced to the call that caused it.
#[test]
fn a_trim_says_what_it_did() {
    let mut app = TestApp::new();
    app.deliver_trimmed("bash", 4876, 620);
    let status = app.status_line();
    assert!(status.contains("bash"), "name the tool: {status}");
    assert!(status.contains("4,876"), "and the size before: {status}");
    assert!(status.contains("620"), "and after: {status}");
}

#[test]
fn the_sidebar_carries_the_running_total() {
    let mut app = TestApp::new();
    app.deliver_trimmed("bash", 4876, 620);
    app.select_sidebar_tab(0);
    let row = app
        .render_to_text(120, 34)
        .into_iter()
        .find(|r| r.contains("dipangkas"))
        .expect("a trimmed row");
    assert!(row.contains('1'), "the count: {row}");
    assert!(row.contains("4,256"), "and what it saved: {row}");
}

/// An install that never configured TypeSafe should not carry a row reading
/// zero forever.
#[test]
fn the_sidebar_row_is_absent_until_something_happens() {
    let mut app = TestApp::new();
    app.select_sidebar_tab(0);
    assert!(
        !app.render_to_text(120, 34)
            .iter()
            .any(|r| r.contains("dipangkas")),
        "nothing has been trimmed, so there is nothing to report"
    );
}

/// The window says what happened this session, which is the question someone
/// opens it to answer.
#[test]
fn the_window_reports_the_session() {
    let mut app = TestApp::new();
    app.deliver_trimmed("bash", 4876, 620);
    app.deliver_trimmed("glob", 3818, 615);
    app.run_command("/typesafe").expect("/typesafe");
    let text = app.render_to_text(100, 24).join("\n");
    assert!(text.contains("This session"), "a report row: {text}");
    assert!(text.contains("2 result(s) trimmed"), "the count: {text}");
    assert!(text.contains("7,459"), "and the saving: {text}");
}

#[test]
fn the_window_distinguishes_no_key_from_nothing_yet() {
    let mut app = TestApp::new();
    app.run_command("/typesafe").expect("/typesafe");
    let text = app.render_to_text(100, 24).join("\n");
    assert!(
        text.contains("no key set"),
        "with no key, say that rather than implying it ran and found nothing: {text}"
    );
}

/// A count already made is a fact about the session, not a function of
/// whether the key is still there.
#[test]
fn a_cleared_key_does_not_erase_what_happened() {
    let mut app = TestApp::new();
    app.deliver_trimmed("bash", 4876, 620);
    app.run_command("/typesafe").expect("/typesafe");
    let text = app.render_to_text(100, 24).join("\n");
    assert!(
        text.contains("1 result(s) trimmed"),
        "what happened should still be reported: {text}"
    );
}
