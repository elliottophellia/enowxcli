//! `/agent` — the manual override. The router decides on its own, but a user
//! naming a specialist is a stronger signal than a classification.

use enowx_tui::testing::TestApp;

#[test]
fn it_switches_to_a_named_agent() {
    let mut app = TestApp::new();
    app.run_command("/agent fe").expect("switch");
    assert_eq!(app.active_agent(), "fe");
}

/// Naming an agent that does not exist should say what does, not just fail.
#[test]
fn an_unknown_agent_is_refused_with_the_alternatives() {
    let mut app = TestApp::new();
    let before = app.active_agent();
    let err = app.run_command("/agent frontend").unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("frontend"), "{msg}");
    assert!(msg.contains("fe"), "the real name should be offered: {msg}");
    assert_eq!(app.active_agent(), before, "a refused switch changes nothing");
}

#[test]
fn switching_to_the_current_agent_is_harmless() {
    let mut app = TestApp::new();
    app.run_command("/agent fe").expect("switch");
    app.run_command("/agent fe").expect("again");
    assert_eq!(app.active_agent(), "fe");
}

/// The bare command lists the roster rather than doing nothing.
#[test]
fn the_bare_command_shows_the_roster() {
    let mut app = TestApp::new();
    app.run_command("/agent").expect("list");
    let rows = app.render_to_text(100, 40);
    let text = rows.join("\n");
    for name in ["fe", "be", "librarian"] {
        assert!(text.contains(name), "`{name}` should be listed: {text}");
    }
    assert!(
        !text.contains("compactor"),
        "the compactor is not a routing target"
    );
}

/// The switch is announced, or the next reply changes voice for no visible
/// reason.
#[test]
fn a_switch_is_marked_in_the_transcript() {
    let mut app = TestApp::new();
    app.run_command("/agent review").expect("switch");
    let text = app.render_to_text(100, 40).join("\n");
    assert!(text.contains("review"), "{text}");
}

#[test]
fn the_name_is_case_insensitive() {
    let mut app = TestApp::new();
    app.run_command("/agent FE").expect("switch");
    assert_eq!(app.active_agent(), "fe");
}
