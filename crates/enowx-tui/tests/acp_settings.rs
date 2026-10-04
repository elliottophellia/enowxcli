//! ACP agents in the interface: `/acp` to pick one for the session (with
//! its name completed), `/acp off` and `/native` to go back, the status bar
//! saying which is in use, Settings > ACP agents as one card per agent, and
//! the roster's `e` for mixing engines. Nothing here installs or starts an
//! agent: the test app has no runtime to run one on.

use crossterm::event::KeyCode;
use enowx_tui::testing::TestApp;

fn screen(app: &mut TestApp) -> String {
    app.render_to_text(150, 60).join("\n")
}

#[test]
fn acp_alone_lists_the_agents_and_enowx_itself() {
    let mut app = TestApp::new();
    app.run_command("/acp").unwrap();
    let text = screen(&mut app);
    assert!(text.contains("RUN THIS SESSION ON"), "{text}");
    for name in ["enowx", "Claude Code", "Codex", "Gemini CLI"] {
        assert!(text.contains(name), "{name}: {text}");
    }
    assert!(text.contains("enowx's own model · in use"), "{text}");
}

#[test]
fn the_agent_name_is_completed() {
    let mut app = TestApp::new();
    app.type_input("/acp c");
    let text = screen(&mut app);
    assert!(text.contains("/acp claude"), "{text}");
    assert!(text.contains("/acp codex"), "{text}");
    assert!(!text.contains("/acp gemini"), "{text}");
    app.type_input("o");
    let text = screen(&mut app);
    assert!(
        text.contains("/acp codex") && !text.contains("/acp claude"),
        "{text}"
    );
}

#[test]
fn an_unknown_agent_is_refused_with_the_names_there_are() {
    let mut app = TestApp::new();
    let error = app.run_command("/acp nope").unwrap_err().to_string();
    assert!(error.contains("no ACP agent named `nope`"), "{error}");
    assert!(error.contains("claude, codex, gemini"), "{error}");
}

#[test]
fn the_status_bar_says_native_and_acp_off_keeps_it_so() {
    let mut app = TestApp::new();
    assert!(screen(&mut app).contains("enowx native"));
    app.run_command("/acp off").unwrap();
    assert!(!enowx_core::Config::load().unwrap().acp.any_assigned());
    app.run_command("/native").unwrap();
    assert!(screen(&mut app).contains("enowx native"));
}

#[test]
fn settings_has_a_card_per_agent_and_a_row_to_add_one() {
    let mut app = TestApp::new();
    app.press(KeyCode::Char('p'), true).unwrap();
    assert!(screen(&mut app).contains(" ACP agents "));
    app.run_command("/acp").unwrap();
    app.press_key(KeyCode::Esc).unwrap();
    let mut app = TestApp::new();
    app.press(KeyCode::Char('p'), true).unwrap();
    // To the ACP agents section, then into it.
    for _ in 0..4 {
        app.press_key(KeyCode::Down).unwrap();
    }
    app.press_key(KeyCode::Enter).unwrap();
    let text = screen(&mut app);
    for name in [
        "Claude Code",
        "Codex",
        "Gemini CLI",
        "Add a custom ACP agent",
    ] {
        assert!(text.contains(name), "{name}: {text}");
    }
    assert!(
        text.contains("default model · default effort · ask me"),
        "{text}"
    );
}

#[test]
fn a_card_sets_permissions_and_bypass_warns() {
    let mut app = TestApp::new();
    app.press(KeyCode::Char('p'), true).unwrap();
    for _ in 0..4 {
        app.press_key(KeyCode::Down).unwrap();
    }
    app.press_key(KeyCode::Enter).unwrap(); // the list
    app.press_key(KeyCode::Enter).unwrap(); // Claude Code's card
    let text = screen(&mut app);
    for label in [
        "Status",
        "Default model",
        "Thinking effort",
        "Permissions",
        "Run this session on it",
    ] {
        assert!(text.contains(label), "{label}: {text}");
    }
    for _ in 0..3 {
        app.press_key(KeyCode::Down).unwrap(); // Permissions
    }
    app.press_key(KeyCode::Right).unwrap(); // enowx's rules
    app.press_key(KeyCode::Right).unwrap(); // bypass
    app.press_key(KeyCode::Enter).unwrap();
    assert_eq!(
        enowx_core::Config::load()
            .unwrap()
            .acp
            .engine("claude")
            .permission,
        "bypass"
    );
    assert!(
        app.status_line().contains("Careful"),
        "{}",
        app.status_line()
    );
}

#[test]
fn a_custom_agent_is_added_with_its_args_and_env() {
    let mut app = TestApp::new();
    app.press(KeyCode::Char('p'), true).unwrap();
    for _ in 0..4 {
        app.press_key(KeyCode::Down).unwrap();
    }
    app.press_key(KeyCode::Enter).unwrap();
    for _ in 0..3 {
        app.press_key(KeyCode::Down).unwrap(); // the row that adds one
    }
    app.press_key(KeyCode::Enter).unwrap();
    app.type_keys("mine");
    app.press_key(KeyCode::Down).unwrap();
    app.type_keys("my-agent");
    app.press_key(KeyCode::Down).unwrap();
    app.type_keys("--acp --quiet");
    app.press_key(KeyCode::Down).unwrap();
    app.type_keys("A=1; B=two");
    app.press_key(KeyCode::Enter).unwrap();
    let config = enowx_core::Config::load().unwrap();
    let mine = config.acp.custom.get("mine").expect("saved");
    assert_eq!(mine.args, vec!["--acp", "--quiet"]);
    assert_eq!(mine.env.get("B").map(String::as_str), Some("two"));
    // And it completes like the built-ins.
    let mut app = TestApp::new();
    app.type_input("/acp m");
    assert!(screen(&mut app).contains("/acp mine"));
}

#[test]
fn a_taken_name_is_refused_for_a_custom_agent() {
    let mut app = TestApp::new();
    app.press(KeyCode::Char('p'), true).unwrap();
    for _ in 0..4 {
        app.press_key(KeyCode::Down).unwrap();
    }
    app.press_key(KeyCode::Enter).unwrap();
    for _ in 0..3 {
        app.press_key(KeyCode::Down).unwrap();
    }
    app.press_key(KeyCode::Enter).unwrap();
    app.type_keys("claude");
    app.press_key(KeyCode::Down).unwrap();
    app.type_keys("x");
    app.press_key(KeyCode::Enter).unwrap();
    assert!(screen(&mut app).contains("is taken"));
}

#[test]
fn e_in_the_roster_still_moves_one_agent() {
    let mut app = TestApp::new();
    app.run_command("/agent").unwrap();
    app.press_key(KeyCode::Char('e')).unwrap();
    let agents = enowx_core::Config::load().unwrap().acp.agents;
    let (agent, engine) = agents.iter().next().expect("one agent moved");
    assert_eq!(engine, "claude");
    assert!(screen(&mut app).contains("Claude Code (ACP)"));
    for _ in 0..3 {
        app.press_key(KeyCode::Char('e')).unwrap();
    }
    let agents = enowx_core::Config::load().unwrap().acp.agents;
    assert!(!agents.contains_key(agent), "{agents:?}");
}
