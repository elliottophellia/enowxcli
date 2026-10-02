//! Settings > Team: agents working together, off by default; on, each part
//! and how cross-review runs can be set, and are saved to the config.

use crossterm::event::KeyCode;
use enowx_tui::testing::TestApp;

fn screen(app: &mut TestApp) -> String {
    app.render_to_text(150, 44).join("\n")
}

#[test]
fn team_is_off_by_default_and_shows_only_its_switch() {
    let mut app = TestApp::new();
    app.run_command("/team").unwrap();
    let text = screen(&mut app);
    assert!(text.contains("TEAM · AGENTS WORKING TOGETHER"), "{text}");
    assert!(text.contains("Agents work together"), "{text}");
    assert!(text.contains("◂ off ▸"), "{text}");
    assert!(!text.contains("Reviewer"), "the rest only when on: {text}");
}

#[test]
fn turning_it_on_shows_its_parts_and_saves_them() {
    let mut app = TestApp::new();
    app.run_command("/team").unwrap();
    app.press_key(KeyCode::Right).unwrap(); // on
    let text = screen(&mut app);
    for label in [
        "Messages between agents at work",
        "Shared board for each run",
        "Cross-review of delegated work",
        "Correction rounds at most",
        "Reviewer",
    ] {
        assert!(text.contains(label), "{label}: {text}");
    }
    // Rounds: 2 by default, up to 3.
    for _ in 0..4 {
        app.press_key(KeyCode::Down).unwrap();
    }
    app.press_key(KeyCode::Right).unwrap();
    assert!(screen(&mut app).contains("◂ 3 ▸"), "{}", screen(&mut app));
    app.press_key(KeyCode::Enter).unwrap();
    let comms = enowx_core::Config::load().unwrap().agent.comms;
    assert!(comms.enabled);
    assert_eq!(comms.review_rounds, 3);
    assert_eq!(comms.reviewer, "review");
    assert!(app.is_modal_open(), "the section stays open");
}
