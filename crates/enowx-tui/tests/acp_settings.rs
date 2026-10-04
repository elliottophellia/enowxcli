//! Settings > ACP agents and the roster's `e`: engines listed with what they
//! need, permissions safe by default, bypass only when chosen, and which
//! agent runs where saved to `[acp]`.

use crossterm::event::KeyCode;
use enowx_tui::testing::TestApp;

fn screen(app: &mut TestApp) -> String {
    app.render_to_text(150, 70).join("\n")
}

#[test]
fn it_lists_the_built_in_engines_and_a_custom_slot() {
    let mut app = TestApp::new();
    app.run_command("/acp").unwrap();
    let text = screen(&mut app);
    assert!(text.contains("ACP AGENTS"), "{text}");
    for name in ["Claude Code", "Codex", "Gemini CLI", "Custom ACP agent"] {
        assert!(text.contains(name), "{name}: {text}");
    }
    assert!(text.contains("◂ ask me ▸"), "asking is the default: {text}");
}

#[test]
fn bypass_is_only_set_when_chosen_and_says_so() {
    let mut app = TestApp::new();
    app.run_command("/acp").unwrap();
    for _ in 0..3 {
        app.press_key(KeyCode::Down).unwrap(); // Claude Code's permissions
    }
    app.press_key(KeyCode::Right).unwrap(); // enowx's rules
    app.press_key(KeyCode::Right).unwrap(); // bypass
    assert!(screen(&mut app).contains("bypass: it asks nothing"));
    app.press_key(KeyCode::Enter).unwrap();
    let config = enowx_core::Config::load().unwrap();
    assert_eq!(config.acp.engine("claude").permission, "bypass");
    assert_eq!(config.acp.engine("codex").permission, "ask");
    assert!(
        app.status_line().contains("Careful"),
        "{}",
        app.status_line()
    );
}

#[test]
fn a_custom_agent_is_saved_with_its_args_and_env() {
    let mut app = TestApp::new();
    app.run_command("/acp").unwrap();
    for _ in 0..12 {
        app.press_key(KeyCode::Down).unwrap(); // the custom agent's name
    }
    app.type_keys("mine");
    app.press_key(KeyCode::Down).unwrap();
    app.type_keys("my-agent");
    app.press_key(KeyCode::Down).unwrap();
    app.type_keys("--acp --quiet");
    app.press_key(KeyCode::Down).unwrap();
    app.type_keys("A=1; B=two");
    app.press_key(KeyCode::Enter).unwrap();
    let custom = enowx_core::Config::load().unwrap().acp.custom;
    let mine = custom.get("mine").expect("saved");
    assert_eq!(mine.command, "my-agent");
    assert_eq!(mine.args, vec!["--acp", "--quiet"]);
    assert_eq!(mine.env.get("B").map(String::as_str), Some("two"));
}

#[test]
fn a_built_in_name_is_refused_for_a_custom_agent() {
    let mut app = TestApp::new();
    app.run_command("/acp").unwrap();
    for _ in 0..12 {
        app.press_key(KeyCode::Down).unwrap();
    }
    app.type_keys("claude");
    app.press_key(KeyCode::Enter).unwrap();
    assert!(screen(&mut app).contains("taken by a built-in engine"));
}

#[test]
fn e_in_the_roster_moves_an_agent_between_engines() {
    let mut app = TestApp::new();
    app.run_command("/agent").unwrap();
    app.press_key(KeyCode::Char('e')).unwrap();
    let agents = enowx_core::Config::load().unwrap().acp.agents;
    let (agent, engine) = agents.iter().next().expect("one agent moved");
    assert_eq!(engine, "claude", "the first engine after enowx");
    assert!(screen(&mut app).contains("Claude Code (ACP)"));
    // Round the list: codex, gemini, then back to the configured model.
    for _ in 0..3 {
        app.press_key(KeyCode::Char('e')).unwrap();
    }
    let agents = enowx_core::Config::load().unwrap().acp.agents;
    assert!(!agents.contains_key(agent), "{agents:?}");
}

#[test]
fn it_is_a_section_of_settings() {
    let mut app = TestApp::new();
    app.press(KeyCode::Char('p'), true).unwrap();
    assert!(screen(&mut app).contains(" ACP agents "));
}
