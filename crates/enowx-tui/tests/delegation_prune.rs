//! A delegation that reported has its transcript cleared; it leaves the
//! sidebar then, since a click on it went nowhere. One at work, or failed
//! (its transcript kept), stays.

use enowx_tui::testing::TestApp;

#[test]
fn a_finished_delegation_with_no_transcript_leaves_the_sidebar() {
    let mut app = TestApp::in_conversation();
    let done = app.add_delegation("fe", "build the page", &[("user", "build the page")]);
    let failed = app.add_delegation("be", "build the api", &[("user", "build the api")]);
    let working = app.add_delegation("db", "add the table", &[("user", "add the table")]);
    app.deliver_delegation_finished("fe", &done, false);
    app.deliver_delegation_finished("be", &failed, true);
    assert_eq!(
        app.prune_delegations(),
        3,
        "transcripts still on disk: all stay"
    );
    app.clear_branch(&done);
    assert_eq!(
        app.prune_delegations(),
        2,
        "the finished one, cleared, goes"
    );
    let _ = working;
}
