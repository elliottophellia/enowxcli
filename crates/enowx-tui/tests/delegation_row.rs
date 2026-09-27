//! A delegation is one row, start to finish.
//!
//! It used to be three blocks: the `delegate` tool call ("✓ delegate  1
//! line"), a brief row ("◆ delegate fe  24 line brief") and, at the end, a
//! notice with the sub-agent's report as raw text. Two rows said the same
//! thing and the report read as a paste.

use enowx_tui::testing::TestApp;

const W: u16 = 110;
const H: u16 = 36;

const REPORT: &str = "DONE: rebuilt the portfolio page\n\
                      CHANGED: index.html, style.css\n\
                      VERIFIED: opened it at 375 and 1280 wide\n\
                      NEXT: nothing";

fn delegate_rows(app: &mut TestApp) -> Vec<String> {
    app.main_column(W, H)
        .into_iter()
        .filter(|row| row.contains("delegate"))
        .collect()
}

/// The whole path a live delegation takes: the orchestrator's call, the start,
/// the report.
fn delegated(app: &mut TestApp) {
    app.push_assistant("Murni kerja frontend.");
    app.push_tool(
        "call-1",
        "delegate",
        r#"{"agent":"fe","task":"write the page"}"#,
        "delegating to fe",
    );
    app.deliver_delegation_started("fe", "write the page", "branch-1");
}

#[test]
fn one_delegation_is_one_row() {
    let mut app = TestApp::new();
    delegated(&mut app);
    assert_eq!(delegate_rows(&mut app).len(), 1, "while it runs");
    app.deliver_delegation_report("fe", "branch-1", REPORT, false);
    assert_eq!(delegate_rows(&mut app).len(), 1, "and once it reports");
}

#[test]
fn the_row_shows_how_it_went() {
    let mut app = TestApp::new();
    delegated(&mut app);
    let running = delegate_rows(&mut app).remove(0);
    // `◆`, as the Agents card marks the same delegation.
    assert!(
        running.contains('◆') && running.contains("working"),
        "{running}"
    );

    app.deliver_delegation_report("fe", "branch-1", REPORT, false);
    let done = delegate_rows(&mut app).remove(0);
    assert!(done.contains('✓') && !done.contains("working"), "{done}");

    let mut app = TestApp::new();
    delegated(&mut app);
    app.deliver_delegation_report("fe", "branch-1", "failed before changing anything: x", true);
    let failed = delegate_rows(&mut app).remove(0);
    assert!(failed.contains('✗'), "{failed}");
}

/// The report's fields line up: labels in one column, values in the next.
#[test]
fn the_report_is_laid_out_as_fields() {
    let mut app = TestApp::new();
    delegated(&mut app);
    app.deliver_delegation_report("fe", "branch-1", REPORT, false);
    let rows = app.main_column(W, H);
    let done = rows
        .iter()
        .find(|row| row.contains("DONE"))
        .expect("a DONE row");
    let changed = rows
        .iter()
        .find(|row| row.contains("CHANGED"))
        .expect("a CHANGED row");
    assert!(!done.contains("DONE:"), "the colon goes: {done}");
    let value = |row: &str, text: &str| row.find(text).expect("value on its label's row");
    assert_eq!(
        value(done, "rebuilt"),
        value(changed, "index.html"),
        "values should start on one column:\n{done}\n{changed}"
    );
    // Right under the row, not after a paragraph gap.
    let row = rows.iter().position(|r| r.contains("delegate")).unwrap();
    assert!(rows[row + 1].contains("DONE"), "{rows:#?}");
}

/// A report that ignores the contract is still shown, as text.
#[test]
fn a_report_without_fields_is_shown_as_text() {
    let mut app = TestApp::new();
    delegated(&mut app);
    app.deliver_delegation_report("fe", "branch-1", "Rewrote the page and checked it.", false);
    assert!(app
        .main_column(W, H)
        .iter()
        .any(|row| row.contains("Rewrote the page and checked it.")));
}

/// A routing call that failed is the only record of the failure, so it
/// stays.
#[test]
fn a_failed_delegate_call_is_still_shown() {
    let mut app = TestApp::new();
    app.push_tool_with_status(
        "call-1",
        "delegate",
        r#"{"agent":"nobody"}"#,
        "unknown agent nobody",
        true,
    );
    assert_eq!(delegate_rows(&mut app).len(), 1);
}

/// A handover already has its marker; the tool row repeated it.
#[test]
fn a_handoff_call_is_not_drawn_twice() {
    let mut app = TestApp::new();
    app.push_assistant("Handing this over.");
    app.push_tool(
        "call-1",
        "handoff",
        r#"{"agent":"fe","reason":"frontend work"}"#,
        "handing over to fe",
    );
    let text = app.main_column(W, H).join("\n");
    assert!(!text.contains("handoff"), "{text}");
}

/// Resuming shows what the live session showed: the row and its report,
/// not the report message the agent loop stores as a user turn.
#[test]
fn a_resumed_session_shows_its_delegation_the_same_way() {
    let mut app = TestApp::new();
    app.resume_with_delegation("fe", "write the page", REPORT, false)
        .expect("resume");
    let rows = app.main_column(W, H);
    let text = rows.join("\n");
    assert!(
        !text.contains("[delegation to"),
        "no raw report message: {text}"
    );
    assert_eq!(delegate_rows(&mut app).len(), 1, "{text}");
    let row = delegate_rows(&mut app).remove(0);
    assert!(row.contains('✓') && row.contains("fe"), "{row}");
    assert!(text.contains("rebuilt the portfolio page"), "{text}");
}

/// And the sub-agent is listed again, so its branch can be opened.
#[test]
fn a_resumed_session_lists_its_sub_agents() {
    let mut app = TestApp::new();
    app.resume_with_delegation("fe", "write the page", REPORT, false)
        .expect("resume");
    app.select_sidebar_tab(0);
    let _ = app.render_to_text(W, H);
    // Two clickable rows per sub-agent: its name and its task.
    assert_eq!(app.delegation_rows(), 2);
    app.open_delegation(0).expect("the branch opens");
    assert_eq!(app.viewing_agent().as_deref(), Some("fe"));
}

/// A new session starts with no sub-agents listed.
#[test]
fn a_new_session_forgets_the_last_ones_sub_agents() {
    let mut app = TestApp::new();
    app.resume_with_delegation("fe", "write the page", REPORT, false)
        .expect("resume");
    app.run_command("/new").expect("/new");
    app.select_sidebar_tab(0);
    let _ = app.render_to_text(W, H);
    assert_eq!(app.delegation_rows(), 0);
}

/// A session saved before delegations were recorded on it still finds its
/// sub-agents: each branch names its parent.
#[test]
fn an_older_session_finds_its_sub_agents_through_their_branches() {
    let mut app = TestApp::new();
    let branch = app
        .resume_with_unrecorded_delegation("fe", "write the page", REPORT)
        .expect("resume");
    assert_eq!(delegate_rows(&mut app).len(), 1);
    app.select_sidebar_tab(0);
    let _ = app.render_to_text(W, H);
    assert_eq!(app.delegation_rows(), 2, "listed again");
    app.open_delegation(0).expect("and its branch opens");
    assert_eq!(app.viewing_agent().as_deref(), Some("fe"));
    let _ = branch;
}
