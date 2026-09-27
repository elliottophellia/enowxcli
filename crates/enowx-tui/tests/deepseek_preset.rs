//! Connecting DeepSeek's own API: pick it, paste a key, choose a model.

use enowx_tui::testing::TestApp;

/// DeepSeek's row in the provider list.
fn deepseek_row() -> usize {
    enowx_core::PROVIDER_PRESETS
        .iter()
        .position(|preset| preset.id == "deepseek")
        .expect("DeepSeek ships as a preset")
}

#[test]
fn a_key_is_all_it_takes_to_connect() {
    let mut app = TestApp::new();
    app.open_providers();
    app.select_provider_at(deepseek_row());
    app.set_settings_field("api_key", "sk-test-key");
    app.connect_preset().expect("connect");
    let (name, preset, base_url, models_url, key) = app.config_provider();
    assert_eq!(name, "DeepSeek");
    assert_eq!(preset, "deepseek");
    assert_eq!(base_url, "https://api.deepseek.com");
    assert_eq!(models_url, "https://api.deepseek.com/models");
    assert_eq!(key, "sk-test-key");
}

/// The model then takes DeepSeek's published limits and prices.
#[test]
fn choosing_a_model_takes_deepseeks_prices() {
    let mut app = TestApp::new();
    app.open_providers();
    app.select_provider_at(deepseek_row());
    app.set_settings_field("api_key", "sk-test-key");
    app.connect_preset().expect("connect");
    app.run_command("/model deepseek-v4-pro").expect("/model");
    assert_eq!(
        app.config_model(),
        ("deepseek-v4-pro".to_owned(), 1_000_000)
    );
    assert_eq!(app.config_model_pricing(), (0.66, 1.98, 0.022, false));

    app.run_command("/model deepseek-flash").expect("/model");
    assert_eq!(app.config_model_pricing(), (0.15, 0.6, 0.003, true));
}
