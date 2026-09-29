//! Providers are kept side by side, each with its own key: connecting, editing or disconnecting one never costs the
//! others, or the model in use.

use enowx_tui::testing::TestApp;

/// A custom gateway with its key, and a model on it in use.
fn seeded() -> TestApp {
    let mut app = TestApp::new();
    app.seed_provider(
        "enowx",
        "https://ai.enowx.id/v1",
        "https://ai.enowx.id/v1/models",
        "sk-secret",
        "cbc/deepseek-v4.1-flash",
    );
    app
}

#[test]
fn connecting_a_second_provider_keeps_the_first() {
    let mut app = seeded();
    app.open_providers();
    app.select_provider("DeepSeek");
    app.set_settings_field("api_key", "sk-deepseek");
    app.submit_form().expect("connect");

    assert_eq!(app.stored_key("deepseek").as_deref(), Some("sk-deepseek"));
    assert_eq!(
        app.stored_key("enowx").as_deref(),
        Some("sk-secret"),
        "the first provider's key must survive"
    );
    assert_eq!(
        app.active_model(),
        "enowx/cbc/deepseek-v4.1-flash",
        "connecting another provider does not switch the model"
    );
}

#[test]
fn the_list_says_which_providers_are_connected() {
    let mut app = seeded();
    app.seed_key("deepseek", "sk-deepseek");
    app.open_providers();
    let rows = app.provider_rows();
    let row = |name: &str| {
        rows.iter()
            .find(|(label, _)| label == name)
            .map(|(_, state)| state.clone())
            .unwrap_or_else(|| panic!("no row {name}: {rows:?}"))
    };
    assert_eq!(row("DeepSeek"), "Connected");
    assert_eq!(row("OpenAI"), "Enter adds your API key");
    assert_eq!(row("enowx"), "Connected · ai.enowx.id · in use");
    assert_eq!(
        rows.last().map(|(label, _)| label.as_str()),
        Some("Add a custom provider")
    );
}

/// Opening a custom provider shows its settings, with the stored key kept
/// out of sight: an empty key field keeps it.
#[test]
fn a_custom_provider_opens_with_its_settings() {
    let mut app = seeded();
    app.open_providers();
    app.select_provider("enowx");
    assert_eq!(app.settings_field("name"), "enowx");
    assert_eq!(app.settings_field("base_url"), "https://ai.enowx.id/v1");
    assert_eq!(
        app.settings_field("models_url"),
        "https://ai.enowx.id/v1/models"
    );
    assert_eq!(app.settings_field("api_key"), "", "a key is never shown");
}

#[test]
fn editing_the_endpoint_keeps_the_key_and_the_model() {
    let mut app = seeded();
    app.open_providers();
    app.select_provider("enowx");
    app.edit_settings_field("base_url", "https://ai.enowx.id/v2");
    app.submit_form().expect("save");

    let entry = app.provider_entry("enowx").expect("entry");
    assert_eq!(entry.base_url, "https://ai.enowx.id/v2");
    assert_eq!(app.stored_key("enowx").as_deref(), Some("sk-secret"));
    assert_eq!(app.active_model(), "enowx/cbc/deepseek-v4.1-flash");
}

/// Typing in one field never clears another: correcting the endpoint is the
/// commonest edit there is.
#[test]
fn typing_in_one_field_leaves_the_others() {
    let mut app = seeded();
    app.open_providers();
    app.select_provider("enowx");
    app.edit_settings_field("base_url", "https://someone-else.example/v1");
    assert_eq!(
        app.settings_field("models_url"),
        "https://ai.enowx.id/v1/models"
    );
    assert_eq!(app.settings_field("name"), "enowx");
}

/// A key issued by one service means nothing at another. Moving to a
/// different host without typing a new key drops the stored one, decided
/// at save, and a model-list URL on the old host moves with the endpoint.
#[test]
fn moving_to_a_different_host_drops_the_stored_key() {
    let mut app = seeded();
    app.open_providers();
    app.select_provider("enowx");
    app.edit_settings_field("base_url", "https://someone-else.example/v1");
    app.submit_form().expect("save");

    assert_eq!(app.stored_key("enowx"), None);
    let entry = app.provider_entry("enowx").expect("entry");
    assert_eq!(entry.models_url, "https://someone-else.example/v1/models");
}

#[test]
fn a_key_typed_in_the_form_replaces_the_stored_one() {
    let mut app = seeded();
    app.open_providers();
    app.select_provider("enowx");
    app.set_settings_field("api_key", "sk-rotated");
    app.submit_form().expect("save");
    assert_eq!(app.stored_key("enowx").as_deref(), Some("sk-rotated"));
    assert_eq!(app.active_model(), "enowx/cbc/deepseek-v4.1-flash");
}

#[test]
fn a_new_custom_provider_takes_its_id_from_its_name() {
    let mut app = TestApp::new();
    app.open_custom_provider_form();
    app.set_settings_field("name", "My Gateway");
    app.set_settings_field("base_url", "https://gw.example/v1/");
    app.set_settings_field("api_key", "sk-gw");
    app.set_settings_field("models_url", "https://gw.example/v1/models");
    app.submit_form().expect("save");

    let entry = app
        .provider_entry("my-gateway")
        .expect("saved under its id");
    assert_eq!(entry.name, "My Gateway");
    assert_eq!(entry.base_url, "https://gw.example/v1");
    assert_eq!(app.stored_key("my-gateway").as_deref(), Some("sk-gw"));
}

#[test]
fn a_custom_provider_needs_a_name_an_endpoint_and_a_free_id() {
    let mut app = TestApp::new();
    app.open_custom_provider_form();
    app.set_settings_field("base_url", "https://gw.example/v1");
    assert!(app.submit_form().is_err(), "no name");

    app.set_settings_field("name", "gw");
    app.set_settings_field("base_url", "");
    assert!(app.submit_form().is_err(), "no endpoint");

    app.set_settings_field("name", "OpenAI");
    app.set_settings_field("base_url", "https://gw.example/v1");
    let error = app.submit_form().unwrap_err().to_string();
    assert!(error.contains("openai"), "{error}");
}

#[test]
fn disconnecting_removes_only_that_providers_key() {
    let mut app = seeded();
    app.seed_key("deepseek", "sk-deepseek");
    app.open_providers();
    app.disconnect_provider("DeepSeek").expect("disconnect");
    assert_eq!(app.stored_key("deepseek"), None);
    assert_eq!(app.stored_key("enowx").as_deref(), Some("sk-secret"));

    // A custom provider goes in two presses: its key, then itself.
    app.disconnect_provider("enowx").expect("disconnect");
    assert_eq!(app.stored_key("enowx"), None);
    assert!(app.provider_entry("enowx").is_some());
    app.disconnect_provider("enowx").expect("remove");
    assert!(app.provider_entry("enowx").is_none());
}

/// Disconnecting the provider the model in use is on moves to the latest
/// pick that can still run.
#[test]
fn disconnecting_the_provider_in_use_falls_back_to_a_recent_pick() {
    let mut app = seeded();
    app.seed_key("deepseek", "sk-deepseek");
    app.run_command("/model deepseek/deepseek-flash")
        .expect("pick deepseek");
    app.run_command("/model enowx/cbc/deepseek-v4.1-flash")
        .expect("pick enowx");
    app.open_providers();
    app.disconnect_provider("enowx").expect("disconnect");
    assert_eq!(app.active_model(), "deepseek/deepseek-flash");
}

#[test]
fn saving_a_provider_leaves_unrelated_settings_alone() {
    let mut app = seeded();
    let before = app.config_agent_max_steps();
    app.open_providers();
    app.select_provider("enowx");
    app.edit_settings_field("base_url", "https://ai.enowx.id/v3");
    app.submit_form().expect("save");
    assert_eq!(app.config_agent_max_steps(), before);
}
