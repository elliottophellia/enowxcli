//! What sub-agents use reaches the interface: the log is every agent's, the
//! session's totals count them, and with a sub-agent's transcript open the
//! context shown is its own.

use enowx_tui::testing::TestApp;

const W: u16 = 160;
const H: u16 = 44;

#[test]
fn a_sub_agents_model_calls_are_in_the_log() {
    let mut app = TestApp::in_conversation();
    app.deliver_branch_usage("fe", "s-fe", (12_000, 900, 15_000, 200_000));
    app.select_sidebar_tab(3);
    let side = app.side_column(W, H).join("\n");
    assert!(side.contains("Frontend · model call"), "{side}");
}

#[test]
fn the_session_counts_what_its_sub_agents_used() {
    let mut app = TestApp::in_conversation();
    app.deliver_usage(1_000, 200, 3_000, 200_000);
    app.deliver_branch_usage("fe", "s-fe", (10_000, 1_000, 12_000, 200_000));
    app.deliver_branch_usage("be", "s-be", (5_000, 500, 6_000, 200_000));
    let side = app.side_column(W, H).join("\n");
    assert!(side.contains("in 16.0k · out 1,700"), "{side}");
    // The conversation's own context, not a sub-agent's.
    assert!(side.contains("3,000 / 200,000"), "{side}");
}

/// Once a delegation reported and the conversation took its tokens into its
/// own, it is not counted twice.
#[test]
fn a_reported_delegation_is_not_counted_twice() {
    let mut app = TestApp::in_conversation();
    let id = app.add_delegation("fe", "build", &[("user", "build")]);
    app.deliver_branch_usage("fe", &id, (10_000, 1_000, 12_000, 200_000));
    app.deliver_delegation_finished("fe", &id, false);
    // The conversation's totals now include fe's.
    app.deliver_usage(11_000, 1_200, 3_000, 200_000);
    let side = app.side_column(W, H).join("\n");
    assert!(side.contains("in 11.0k · out 1,200"), "{side}");
}

#[test]
fn a_sub_agents_transcript_shows_its_own_context() {
    let mut app = TestApp::in_conversation();
    app.deliver_usage(1_000, 200, 3_000, 200_000);
    let id = app.add_delegation("fe", "build", &[("user", "build"), ("assistant", "on it")]);
    app.deliver_branch_usage("fe", &id, (10_000, 1_000, 42_000, 200_000));
    app.open_delegation(0).expect("open it");
    let side = app.side_column(W, H).join("\n");
    assert!(side.contains("42,000 / 200,000"), "{side}");
    assert!(
        side.contains("agent") && side.contains("Frontend"),
        "{side}"
    );
}
