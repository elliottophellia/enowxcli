//! `/agent` — the manual override. The orchestrator decides on its own, but a user
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
    assert_eq!(
        app.active_agent(),
        before,
        "a refused switch changes nothing"
    );
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
    let rows = app.render_to_text(100, 60);
    let text = rows.join("\n");
    // By full name, with the id `/agent` takes where it differs.
    for name in ["Frontend  fe", "Backend  be", "Librarian"] {
        assert!(text.contains(name), "`{name}` should be listed: {text}");
    }
    assert!(
        !text.contains("Compactor"),
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

/// Picked before the first message, the agent has no session to be saved in.
/// The message that creates one carries the pick, or the session would start
/// with the orchestrator while the status bar names the pick.
#[test]
fn a_pick_before_the_first_message_goes_with_it() {
    let mut app = TestApp::new();
    assert_eq!(app.agent_for_new_session(), None, "nothing picked");
    app.run_command("/agent fe").expect("switch");
    assert_eq!(app.agent_for_new_session().as_deref(), Some("fe"));
    app.run_command("/agent orchestrator").expect("back");
    assert_eq!(
        app.agent_for_new_session(),
        None,
        "the default needs no pick"
    );
}

/// Once a session exists it holds its own agent; the message names none.
#[test]
fn a_session_keeps_its_own_agent() {
    let mut app = TestApp::new();
    app.resume_with_switches(&[("user", "hi"), ("assistant", "hello")], &[])
        .expect("resume");
    app.run_command("/agent fe").expect("switch");
    assert_eq!(app.agent_for_new_session(), None);
}

#[test]
fn an_agent_on_its_own_model_says_which_in_the_list() {
    let mut app = TestApp::in_conversation();
    app.set_agent_model("fe", "vendor/design-model");
    app.run_command("/agent").unwrap();
    assert!(app.is_modal_open());
    let screen = app.render_to_text(120, 60).join("\n");
    assert!(screen.contains("vendor/design-model"), "{screen}");
    let marked = screen
        .lines()
        .filter(|line| line.contains("vendor/design-model"))
        .count();
    assert_eq!(marked, 1, "only fe runs on it: {screen}");
}

/// Under every agent in the list, the model it runs on: its own, or the
/// shared default, marked as such.
#[test]
fn each_agent_shows_its_model_under_it() {
    let mut app = TestApp::new();
    app.set_agent_model("orchestrator", "deepseek/deepseek-flash");
    app.run_command("/agent").expect("/agent");
    let screen = app.render_to_text(150, 60).join("\n");
    assert!(
        screen.contains("model  deepseek/deepseek-flash"),
        "{screen}"
    );
    assert!(
        screen.contains("· default") || screen.contains("default (none"),
        "{screen}"
    );
}
