//! Seeing what a sub-agent did.
//!
//! A delegation runs in its own session and reports one line back. That line
//! is all the caller sees, so "fe finished" is indistinguishable from "fe
//! wrote nothing" — and the work it describes is real: files written, commands
//! run. The branch is already on disk; these cover the way in and back.

use crossterm::event::KeyCode;
use enowx_tui::testing::TestApp;

/// The sidebar half of each row. The transcript names the same agents
/// ("fe started"), so searching a whole row finds the wrong one.
fn sidebar_rows(app: &mut TestApp) -> Vec<String> {
    app.render_to_text(120, 34)
        .into_iter()
        .filter_map(|row| row.rsplit('│').nth(1).map(str::to_owned))
        .collect()
}

/// The tab lists what has been delegated, with its state.
#[test]
fn the_agent_tab_lists_delegations() {
    let mut app = TestApp::new();
    app.deliver_delegation_started("fe", "write the HTML", "id-1");
    app.deliver_delegation_started("review", "check it", "id-2");
    app.select_sidebar_tab(3);
    let text = app.render_to_text(120, 34).join("\n");
    assert!(text.contains("DELEGATED"), "a section for them: {text}");
    assert!(text.contains("fe"), "the first: {text}");
    assert!(text.contains("write the HTML"), "and its task: {text}");
    assert!(text.contains("review"), "the second: {text}");
}

/// Running and finished have to look different, or a hung delegation looks
/// like a completed one.
#[test]
fn a_running_delegation_is_marked_apart_from_a_finished_one() {
    let mut app = TestApp::new();
    app.deliver_delegation_started("fe", "write the HTML", "id-1");
    app.deliver_delegation_started("review", "check it", "id-2");
    app.deliver_delegation_finished("fe", "id-1", false);
    app.select_sidebar_tab(3);
    let rows = sidebar_rows(&mut app);
    let fe = rows
        .iter()
        .find(|r| r.contains("fe") && (r.contains('✓') || r.contains('◆') || r.contains('✗')))
        .expect("an fe row in the sidebar");
    let review = rows
        .iter()
        .find(|r| r.contains("review") && (r.contains('✓') || r.contains('◆') || r.contains('✗')))
        .expect("a review row in the sidebar");
    assert!(fe.contains('✓'), "finished: {fe}");
    assert!(review.contains('◆'), "still running: {review}");
}

/// The same agent can be delegated to twice. Finishing one must not mark the
/// other.
#[test]
fn two_runs_of_one_agent_are_tracked_separately() {
    let mut app = TestApp::new();
    app.deliver_delegation_started("fe", "first job", "id-1");
    app.deliver_delegation_started("fe", "second job", "id-2");
    app.deliver_delegation_finished("fe", "id-1", false);
    app.select_sidebar_tab(3);
    let marks: Vec<char> = sidebar_rows(&mut app)
        .iter()
        .filter(|r| r.contains("fe"))
        .filter_map(|r| r.chars().find(|c| *c == '✓' || *c == '◆'))
        .collect();
    assert_eq!(marks, vec!['✓', '◆'], "one finished, one still running");
}

#[test]
fn a_failed_delegation_is_shown_as_failed() {
    let mut app = TestApp::new();
    app.deliver_delegation_started("fe", "write the HTML", "id-1");
    app.deliver_delegation_finished("fe", "id-1", true);
    app.select_sidebar_tab(3);
    let row = sidebar_rows(&mut app)
        .into_iter()
        .find(|r| r.contains("fe") && (r.contains('✓') || r.contains('◆') || r.contains('✗')))
        .expect("an fe row in the sidebar");
    assert!(row.contains('✗'), "a failure should read as one: {row}");
}

/// Opening one shows what it actually did.
#[test]
fn opening_a_delegation_shows_its_transcript() {
    let mut app = TestApp::new();
    app.push_user("MAIN-CONVERSATION");
    app.add_delegation(
        "fe",
        "write the HTML",
        &[
            ("user", "write the HTML"),
            ("assistant", "WROTE-INDEX-HTML"),
        ],
    );
    app.open_delegation(0).expect("open it");
    let text = app.render_to_text(110, 30).join("\n");
    assert!(
        text.contains("WROTE-INDEX-HTML"),
        "the sub-agent's work should be on screen: {text}"
    );
    assert!(
        !text.contains("MAIN-CONVERSATION"),
        "and the main conversation set aside: {text}"
    );
}

/// The way back is the whole reason this is safe to offer.
#[test]
fn escape_returns_to_the_main_conversation() {
    let mut app = TestApp::new();
    app.push_user("MAIN-CONVERSATION");
    app.add_delegation("fe", "write the HTML", &[("assistant", "WROTE-INDEX-HTML")]);
    app.open_delegation(0).expect("open it");
    assert_eq!(app.viewing_agent().as_deref(), Some("fe"));

    app.press(KeyCode::Esc, false).expect("esc");
    assert!(app.viewing_agent().is_none(), "no longer viewing");
    let text = app.render_to_text(110, 30).join("\n");
    assert!(
        text.contains("MAIN-CONVERSATION"),
        "the conversation should come back: {text}"
    );
    assert!(
        !text.contains("WROTE-INDEX-HTML"),
        "and the branch be gone: {text}"
    );
}

/// The footer has to say the transcript is not the conversation, or a
/// scrollback of someone else's tool calls reads as the session gone strange.
#[test]
fn the_footer_says_which_agent_is_being_viewed() {
    let mut app = TestApp::new();
    app.add_delegation("fe", "write the HTML", &[("assistant", "done")]);
    app.open_delegation(0).expect("open it");
    let rows = app.render_to_text(110, 30);
    let footer = rows
        .iter()
        .rev()
        .take(3)
        .find(|r| r.contains("viewing"))
        .expect("the footer should say so");
    assert!(footer.contains("fe"), "and name the agent: {footer}");
    assert!(footer.contains("Esc"), "and the way back: {footer}");
}

/// Opening a second one must not lose the main conversation behind the first.
#[test]
fn opening_another_does_not_lose_the_conversation() {
    let mut app = TestApp::new();
    app.push_user("MAIN-CONVERSATION");
    app.add_delegation("fe", "first", &[("assistant", "FIRST-BRANCH")]);
    app.add_delegation("review", "second", &[("assistant", "SECOND-BRANCH")]);
    app.open_delegation(0).expect("open the first");
    app.open_delegation(1).expect("then the second");
    let text = app.render_to_text(110, 30).join("\n");
    assert!(text.contains("SECOND-BRANCH"), "the second: {text}");
    assert!(!text.contains("FIRST-BRANCH"), "not the first: {text}");

    app.press(KeyCode::Esc, false).expect("esc");
    let text = app.render_to_text(110, 30).join("\n");
    assert!(
        text.contains("MAIN-CONVERSATION"),
        "one Esc returns to the conversation, not to the first branch: {text}"
    );
}

/// Sending while viewing belongs to the main conversation, not the finished
/// branch on screen.
#[test]
fn sending_while_viewing_returns_first() {
    let mut app = TestApp::new();
    app.push_user("MAIN-CONVERSATION");
    app.add_delegation("fe", "write it", &[("assistant", "BRANCH-WORK")]);
    app.open_delegation(0).expect("open it");
    app.type_input("a follow-up question");
    // `start_turn` spawns, which the harness has no runtime for; the guard
    // under test runs before that, so check it directly.
    app.leave_viewing_as_send_would();
    assert!(app.viewing_agent().is_none(), "sending leaves the branch");
    let text = app.render_to_text(110, 30).join("\n");
    assert!(
        text.contains("MAIN-CONVERSATION"),
        "back on the main: {text}"
    );
}

/// Rows have to be clickable, or the list is a display with no way in.
#[test]
fn the_rows_are_clickable() {
    let mut app = TestApp::new();
    app.deliver_delegation_started("fe", "write the HTML", "id-1");
    app.select_sidebar_tab(3);
    let _ = app.render_to_text(120, 34);
    assert!(
        app.delegation_rows() > 0,
        "the delegation rows should be registered as clickable"
    );
}
