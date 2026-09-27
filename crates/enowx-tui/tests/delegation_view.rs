//! Seeing what a sub-agent did.
//!
//! A delegation runs in its own session and reports one line back. That line
//! is all the caller sees, so "fe finished" is indistinguishable from "fe
//! wrote nothing" — and the work it describes is real: files written, commands
//! run. The branch is already on disk; these cover the way in and back.

use crossterm::event::KeyCode;
use enowx_tui::testing::TestApp;

/// The Agents tab, where delegations are listed.
const AGENTS_TAB: usize = 0;

/// The side column's rows. The transcript names the same agents ("fe
/// started"), so searching a whole row finds the wrong one.
fn sidebar_rows(app: &mut TestApp) -> Vec<String> {
    app.side_column(120, 34)
}

/// The tab lists what has been delegated, with its state.
#[test]
fn the_agent_tab_lists_delegations() {
    let mut app = TestApp::new();
    app.deliver_delegation_started("fe", "write the HTML", "id-1");
    app.deliver_delegation_started("review", "check it", "id-2");
    app.select_sidebar_tab(AGENTS_TAB);
    let text = sidebar_rows(&mut app).join("\n");
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
    app.select_sidebar_tab(AGENTS_TAB);
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
    app.select_sidebar_tab(AGENTS_TAB);
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
    app.select_sidebar_tab(AGENTS_TAB);
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
        .find(|r| r.contains("Esc back"))
        .expect("the footer should offer the way back");
    assert!(footer.contains("fe"), "and name the agent: {footer}");
    assert!(
        footer.contains("sub-agent"),
        "and say the transcript is not the conversation: {footer}"
    );
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
    app.select_sidebar_tab(AGENTS_TAB);
    let _ = app.render_to_text(120, 34);
    assert!(
        app.delegation_rows() > 0,
        "the delegation rows should be registered as clickable"
    );
}

/// The footer has to say whether the sub-agent is still working. A transcript
/// on its own cannot: a branch that stopped halfway looks the same as one
/// that finished.
#[test]
fn the_footer_says_whether_it_is_still_working() {
    let mut app = TestApp::new();
    let id = app.add_delegation("fe", "write it", &[("assistant", "some work")]);
    app.open_delegation(0).expect("open it");
    let footer = footer_of(&mut app);
    assert!(footer.contains("working"), "still running: {footer}");

    app.deliver_delegation_finished("fe", &id, false);
    let footer = footer_of(&mut app);
    assert!(footer.contains("finished"), "now done: {footer}");
    assert!(
        !footer.contains("working"),
        "and no longer working: {footer}"
    );
}

#[test]
fn the_footer_says_when_it_failed() {
    let mut app = TestApp::new();
    let id = app.add_delegation("fe", "write it", &[("assistant", "some work")]);
    app.deliver_delegation_finished("fe", &id, true);
    app.open_delegation(0).expect("open it");
    let footer = footer_of(&mut app);
    assert!(
        footer.contains("failed"),
        "a failure should say so: {footer}"
    );
}

/// A transcript frozen at the moment it was opened is what makes a working
/// sub-agent look stopped.
#[test]
fn a_running_branch_keeps_up_with_the_sub_agent() {
    let mut app = TestApp::new();
    let id = app.add_delegation("fe", "write it", &[("assistant", "FIRST-STEP")]);
    app.open_delegation(0).expect("open it");
    assert!(
        app.render_to_text(110, 30)
            .join("\n")
            .contains("FIRST-STEP"),
        "the work so far"
    );

    // The sub-agent keeps working.
    app.grow_branch(&id, &[("assistant", "SECOND-STEP")]);
    app.expire_refresh_timer();
    app.refresh_viewed_delegation();
    assert!(
        app.render_to_text(110, 30)
            .join("\n")
            .contains("SECOND-STEP"),
        "what it did next should appear without reopening it"
    );
}

/// A finished branch cannot change, so re-reading it is pure waste.
#[test]
fn a_finished_branch_is_not_re_read() {
    let mut app = TestApp::new();
    let id = app.add_delegation("fe", "write it", &[("assistant", "FIRST-STEP")]);
    app.deliver_delegation_finished("fe", &id, false);
    app.open_delegation(0).expect("open it");

    // Something else writes to the file; a finished branch should not follow.
    app.grow_branch(&id, &[("assistant", "LATE-WRITE")]);
    app.expire_refresh_timer();
    app.refresh_viewed_delegation();
    assert!(
        !app.render_to_text(110, 30)
            .join("\n")
            .contains("LATE-WRITE"),
        "a finished branch should not be re-read"
    );
}

/// The refresh must not run on every frame: a branch re-read at 25Hz is a
/// file read per frame for as long as someone is watching.
#[test]
fn the_refresh_is_rate_limited() {
    let mut app = TestApp::new();
    let id = app.add_delegation("fe", "write it", &[("assistant", "FIRST-STEP")]);
    app.open_delegation(0).expect("open it");
    app.grow_branch(&id, &[("assistant", "SECOND-STEP")]);
    // No timer expiry: the refresh should decline to run.
    app.refresh_viewed_delegation();
    assert!(
        !app.render_to_text(110, 30)
            .join("\n")
            .contains("SECOND-STEP"),
        "a refresh straight after opening should be skipped"
    );
}

fn footer_of(app: &mut TestApp) -> String {
    app.status_bar(110, 30)
}

/// Events belong to the main conversation even while a branch is on screen.
/// `push` writes to `self.blocks`, which during viewing holds the sub-agent's
/// transcript — so a reply arriving then was written into the branch and
/// thrown away on the way back.
#[test]
fn text_arriving_while_viewing_is_not_lost() {
    let mut app = TestApp::new();
    app.push_user("MAIN-CONVERSATION");
    let id = app.add_delegation("fe", "write it", &[("assistant", "branch work")]);
    app.open_delegation(0).expect("open it");

    // The main agent carries on while the user is looking at the branch.
    app.deliver_delegation_finished("fe", &id, false);
    app.deliver_assistant_text("REPLY-WHILE-VIEWING");

    app.press(KeyCode::Esc, false).expect("esc");
    let text = app.render_to_text(110, 40).join("\n");
    assert!(
        text.contains("MAIN-CONVERSATION"),
        "the conversation should come back: {text}"
    );
    assert!(
        text.contains("REPLY-WHILE-VIEWING"),
        "and what arrived while viewing should be in it: {text}"
    );
    assert!(
        text.contains("fe finished"),
        "including the delegation's own report: {text}"
    );
}

/// Staying in a finished branch is the right default — you opened it to check
/// the work — but only if it does not read as being stuck. The footer says
/// the conversation has moved on.
#[test]
fn the_footer_says_when_the_main_chat_moves_on() {
    let mut app = TestApp::new();
    app.push_user("MAIN-CONVERSATION");
    let id = app.add_delegation("review", "check it", &[("assistant", "checking")]);
    app.open_delegation(0).expect("open it");
    let footer = footer_of(&mut app);
    assert!(
        !footer.contains("main chat"),
        "nothing has happened yet: {footer}"
    );

    app.deliver_delegation_finished("review", &id, false);
    let footer = footer_of(&mut app);
    assert!(
        footer.contains("main chat +1"),
        "the delegation's report landed in the conversation: {footer}"
    );

    app.deliver_assistant_text("and the main agent carried on");
    let footer = footer_of(&mut app);
    assert!(footer.contains("main chat +2"), "and so did that: {footer}");
}

/// Finishing must not move the user out of the branch: the work is what they
/// opened it for, and the screen changing under them while they read is
/// worse than staying.
#[test]
fn finishing_does_not_eject_the_viewer() {
    let mut app = TestApp::new();
    let id = app.add_delegation("review", "check it", &[("assistant", "BRANCH-WORK")]);
    app.open_delegation(0).expect("open it");
    app.deliver_delegation_finished("review", &id, false);
    assert_eq!(
        app.viewing_agent().as_deref(),
        Some("review"),
        "still viewing the branch"
    );
    assert!(
        app.render_to_text(110, 30)
            .join("\n")
            .contains("BRANCH-WORK"),
        "and its work still on screen"
    );
}

/// The count is what the conversation gained, not its total length.
#[test]
fn the_count_is_what_arrived_not_the_whole_conversation() {
    let mut app = TestApp::new();
    for i in 0..5 {
        app.push_user(&format!("earlier message {i}"));
    }
    app.add_delegation("fe", "write it", &[("assistant", "work")]);
    app.open_delegation(0).expect("open it");
    app.deliver_assistant_text("one new reply");
    let footer = footer_of(&mut app);
    assert!(
        footer.contains("+1"),
        "five earlier messages are not news: {footer}"
    );
}

/// A sub-agent is not something the user drives: they cannot send it a
/// message and cannot steer it. So keys pressed while looking at one act on
/// the window, not on the work.
#[test]
fn ctrl_c_while_viewing_leaves_rather_than_stopping_the_turn() {
    let mut app = TestApp::new();
    app.push_user("MAIN-CONVERSATION");
    let cancel = app.start_fake_turn();
    app.add_delegation("fe", "write it", &[("assistant", "BRANCH-WORK")]);
    app.open_delegation(0).expect("open it");

    app.press(KeyCode::Char('c'), true).expect("ctrl+c");
    assert!(
        app.viewing_agent().is_none(),
        "it should take the user out of the branch"
    );
    assert!(
        !cancel.is_cancelled(),
        "and must not stop the conversation from inside a window that is not it"
    );

    // Back on the main conversation, it means what it always did.
    app.press(KeyCode::Char('c'), true).expect("ctrl+c again");
    assert!(
        cancel.is_cancelled(),
        "a second press, now on the conversation, stops the turn"
    );
}

/// Typing a message while viewing belongs to the conversation. There is no
/// way to send anything to a sub-agent, by design.
#[test]
fn there_is_no_way_to_message_a_sub_agent() {
    let mut app = TestApp::new();
    app.push_user("MAIN-CONVERSATION");
    app.add_delegation("fe", "write it", &[("assistant", "BRANCH-WORK")]);
    app.open_delegation(0).expect("open it");
    app.type_input("stop doing that");
    app.leave_viewing_as_send_would();
    assert!(
        app.viewing_agent().is_none(),
        "sending returns to the conversation first"
    );
    assert_eq!(
        app.input_text(),
        "stop doing that",
        "and the message is still the user's to send there"
    );
}

/// Watching a running sub-agent must not inflate the session's tool count.
///
/// The branch is re-read every half second while it runs, and each re-read
/// used to add all of its tool calls to the main session's total again. A
/// portfolio built in eight calls showed 1,175 — the count was wrong by more
/// than two orders of magnitude, and it read as the agent being wasteful.
#[test]
fn watching_a_sub_agent_does_not_inflate_the_tool_count() {
    let mut app = TestApp::new();
    let id = app.add_delegation("fe", "write the page", &[("user", "write the page")]);
    app.grow_branch_with_tool_call(&id, "write", r#"{"path":"index.html","content":"x"}"#);
    app.grow_branch_with_tool_call(&id, "write", r#"{"path":"style.css","content":"y"}"#);
    let before = app.tool_call_total();

    app.open_delegation(0).expect("open it");
    for _ in 0..20 {
        app.expire_refresh_timer();
        app.refresh_viewed_delegation();
    }
    assert_eq!(
        app.tool_call_total(),
        before,
        "viewing and re-reading the branch must leave the session's count alone"
    );
    let side = app.side_column(120, 34).join("\n");
    assert!(
        !side.contains("40 calls") && !side.contains("42 calls"),
        "and the card must not show the inflated figure: {side}"
    );
}
