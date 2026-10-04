//! Settings > Decision model: off by default, each provider asks for what it
//! needs, a provider that cannot work is refused with why, and what is saved
//! lands in `[decision]` with the key in `auth.json`, never on screen.

use crossterm::event::KeyCode;
use enowx_tui::testing::TestApp;

fn screen(app: &mut TestApp) -> String {
    app.render_to_text(150, 70).join("\n")
}

#[test]
fn it_is_off_by_default_and_lists_every_use() {
    let mut app = TestApp::new();
    app.run_command("/decision").unwrap();
    let text = screen(&mut app);
    assert!(text.contains("DECISION MODEL"), "{text}");
    assert!(text.contains("Decision model"), "{text}");
    assert!(text.contains("◂ off ▸"), "off until turned on: {text}");
    assert!(text.contains("Clef (Cloudflare Workers AI)"), "{text}");
    assert!(text.contains("Cloudflare account ID"), "{text}");
    assert!(!enowx_core::Config::load().unwrap().decision.enabled);
}

#[test]
fn a_provider_shows_only_what_it_needs() {
    let mut app = TestApp::new();
    app.run_command("/decision").unwrap();
    app.press_key(KeyCode::Down).unwrap(); // provider
    app.press_key(KeyCode::Right).unwrap(); // jev
    let text = screen(&mut app);
    assert!(text.contains("Jev (TypeSafe)"), "{text}");
    assert!(!text.contains("Cloudflare account ID"), "{text}");
    app.press_key(KeyCode::Right).unwrap(); // custom
    assert!(screen(&mut app).contains("Endpoint URL"));
    app.press_key(KeyCode::Right).unwrap(); // llm
    let text = screen(&mut app);
    assert!(text.contains("A model configured in enowx"), "{text}");
    assert!(
        !text.contains("API key or token"),
        "an enowx model needs no key: {text}"
    );
}

#[test]
fn turning_it_on_without_a_key_is_refused_with_why() {
    let mut app = TestApp::new();
    app.run_command("/decision").unwrap();
    app.press_key(KeyCode::Right).unwrap(); // on, Clef
    app.press_key(KeyCode::Enter).unwrap();
    let text = screen(&mut app);
    assert!(text.contains("account ID"), "says what is missing: {text}");
    assert!(
        !enowx_core::Config::load().unwrap().decision.enabled,
        "nothing is saved"
    );
}

#[test]
fn a_custom_endpoint_saves_with_its_key_hidden() {
    let mut app = TestApp::new();
    app.run_command("/decision").unwrap();
    app.press_key(KeyCode::Right).unwrap(); // on
    app.press_key(KeyCode::Down).unwrap(); // provider
    app.press_key(KeyCode::Right).unwrap(); // jev
    app.press_key(KeyCode::Right).unwrap(); // custom
    app.press_key(KeyCode::Down).unwrap(); // endpoint
    app.type_keys("http://127.0.0.1:9/decide");
    app.press_key(KeyCode::Down).unwrap(); // model
    app.press_key(KeyCode::Down).unwrap(); // key
    app.type_keys("local-secret-123");
    let text = screen(&mut app);
    assert!(
        !text.contains("local-secret-123"),
        "a key is masked: {text}"
    );
    app.press_key(KeyCode::Down).unwrap(); // timeout
    app.press_key(KeyCode::Down).unwrap(); // shadow
    app.press_key(KeyCode::Right).unwrap(); // shadow on
    app.press_key(KeyCode::Enter).unwrap();
    let config = enowx_core::Config::load().unwrap();
    assert!(config.decision.enabled, "{}", screen(&mut app));
    assert_eq!(config.decision.provider, "custom");
    assert_eq!(config.decision.base_url, "http://127.0.0.1:9/decide");
    assert!(config.decision.shadow);
    let stored = config
        .auth
        .key(&enowx_core::decision::secret_id("custom"), &[])
        .map(|(k, _)| k);
    assert_eq!(stored.as_deref(), Some("local-secret-123"));
    let file = std::fs::read_to_string(enowx_core::config::config_path()).unwrap();
    assert!(
        !file.contains("local-secret-123"),
        "the key is not in config.toml"
    );
    assert!(app.is_modal_open(), "the section stays open");
}

#[test]
fn a_use_can_be_turned_off_and_its_threshold_changed() {
    let mut app = TestApp::new();
    app.run_command("/decision").unwrap();
    // Clef's fields: enabled, provider, model, account, key, timeout,
    // shadow, test, then intent on / threshold.
    for _ in 0..8 {
        app.press_key(KeyCode::Down).unwrap();
    }
    app.press_key(KeyCode::Right).unwrap(); // intent off
    app.press_key(KeyCode::Down).unwrap();
    app.press_key(KeyCode::Right).unwrap(); // 0.8 -> 0.85
    app.press_key(KeyCode::Enter).unwrap();
    let uses = enowx_core::Config::load().unwrap().decision.uses;
    assert!(!uses.intent.enabled);
    assert!(
        (uses.intent.threshold - 0.85).abs() < 1e-6,
        "{}",
        uses.intent.threshold
    );
}

#[test]
fn it_is_a_section_of_settings() {
    let mut app = TestApp::new();
    app.press(KeyCode::Char('p'), true).unwrap();
    assert!(screen(&mut app).contains(" Decision model "));
}
