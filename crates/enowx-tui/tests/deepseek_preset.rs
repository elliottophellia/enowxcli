//! Connecting DeepSeek's own API: pick it, paste a key, choose a model.

use enowx_tui::testing::TestApp;

fn connected() -> TestApp {
    let mut app = TestApp::new();
    app.open_providers();
    app.select_provider("DeepSeek");
    app.set_settings_field("api_key", "sk-test-key");
    app.submit_form().expect("connect");
    app
}

#[test]
fn a_key_is_all_it_takes_to_connect() {
    let app = connected();
    assert_eq!(app.stored_key("deepseek").as_deref(), Some("sk-test-key"));
    assert!(
        app.provider_entry("deepseek").is_none(),
        "a built-in provider needs no entry in config.toml"
    );
}

/// The model then takes DeepSeek's published limits and prices.
#[test]
fn choosing_a_model_takes_deepseeks_prices() {
    let mut app = connected();
    app.run_command("/model deepseek/deepseek-v4-pro")
        .expect("/model");
    assert_eq!(app.active_model(), "deepseek/deepseek-v4-pro");
    assert_eq!(app.context_window(), 1_000_000);
    assert_eq!(app.config_model_pricing(), (0.66, 1.98, 0.022, false));

    // A bare id is a model on the provider in use.
    app.run_command("/model deepseek-flash").expect("/model");
    assert_eq!(app.active_model(), "deepseek/deepseek-flash");
    assert_eq!(app.config_model_pricing(), (0.15, 0.6, 0.003, true));
}
