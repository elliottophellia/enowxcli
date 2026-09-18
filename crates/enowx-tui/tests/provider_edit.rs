//! Editing one provider field must not cost the user the rest of their
//! configuration. These pin the rules down, because the form writes a whole
//! `Config` back and it is easy for a field to be silently reset.

use enowx_tui::testing::TestApp;

fn seeded() -> TestApp {
    let mut app = TestApp::new();
    app.seed_provider(
        "enowx",
        "custom",
        "https://ai.enowx.id/v1",
        "https://ai.enowx.id/v1/models",
        "sk-secret",
        "cbc/deepseek-v4.1-flash",
        128_000,
    );
    app
}

#[test]
fn editing_the_base_url_keeps_every_other_field() {
    let mut app = seeded();
    app.open_settings();
    app.set_settings_field("base_url", "https://ai.enowx.id/v2");
    app.save_settings().expect("save");

    let (name, preset, base, models, key) = app.config_provider();
    assert_eq!(base, "https://ai.enowx.id/v2", "the edit should apply");
    assert_eq!(name, "enowx", "provider name must survive");
    assert_eq!(preset, "custom", "preset must survive");
    assert_eq!(models, "https://ai.enowx.id/v1/models", "models URL must survive");
    assert_eq!(key, "sk-secret", "the API key must not be cleared");
}

#[test]
fn editing_the_api_key_keeps_the_model() {
    let mut app = seeded();
    app.open_settings();
    app.set_settings_field("api_key", "sk-rotated");
    app.save_settings().expect("save");

    let (_, _, _, _, key) = app.config_provider();
    let (model, window) = app.config_model();
    assert_eq!(key, "sk-rotated");
    assert_eq!(model, "cbc/deepseek-v4.1-flash", "model must not be cleared");
    assert_eq!(window, 128_000, "context window must not be reset");
}

/// The form covers provider and model settings only. Everything else in the
/// file — agent limits, MCP entries, disabled skills — must pass through.
#[test]
fn saving_the_form_leaves_unrelated_config_alone() {
    let mut app = seeded();
    let before = app.config_agent_max_steps();
    app.open_settings();
    app.set_settings_field("base_url", "https://ai.enowx.id/v3");
    app.save_settings().expect("save");
    assert_eq!(
        app.config_agent_max_steps(),
        before,
        "agent settings are not part of this form and must be untouched"
    );
}

/// Opening the form should show what is already configured, so a user can
/// change one field and save without retyping the others.
#[test]
fn the_form_opens_prefilled_from_the_current_config() {
    let mut app = seeded();
    app.open_settings();
    assert_eq!(app.settings_field("base_url"), "https://ai.enowx.id/v1");
    assert_eq!(app.settings_field("models_url"), "https://ai.enowx.id/v1/models");
    assert_eq!(app.settings_field("api_key"), "sk-secret");
    assert_eq!(app.settings_field("model"), "cbc/deepseek-v4.1-flash");
}

/// "Edit current" is the entry a user picks to adjust the provider they are
/// already on. It must not hand them a blank form.
#[test]
fn edit_current_keeps_the_existing_values() {
    let mut app = seeded();
    app.open_providers();
    // The "Edit current · <name>" row sits after the presets.
    app.select_provider_at(usize::MAX);
    assert_eq!(
        app.settings_field("base_url"),
        "https://ai.enowx.id/v1",
        "editing the current provider must start from its own settings"
    );
    assert_eq!(app.settings_field("api_key"), "sk-secret");
    assert_eq!(app.settings_field("model"), "cbc/deepseek-v4.1-flash");
}

/// Re-picking the preset already in use is an edit, not a reset.
#[test]
fn reselecting_the_same_preset_preserves_settings() {
    let mut app = TestApp::new();
    app.seed_provider(
        "OpenAI",
        "openai",
        "https://api.openai.com/v1",
        "https://api.openai.com/v1/models",
        "sk-openai",
        "gpt-4o",
        128_000,
    );
    app.open_providers();
    app.select_provider_at(1); // OpenAI's row
    assert_eq!(
        app.settings_field("api_key"),
        "sk-openai",
        "choosing the provider already in use must not blank its key"
    );
    assert_eq!(app.settings_field("model"), "gpt-4o");
}

/// Rotating a key through the short "enter your API key" screen must not
/// discard the model the user already chose.
#[test]
fn rotating_a_key_via_the_key_screen_keeps_the_model() {
    let mut app = TestApp::new();
    app.seed_provider(
        "OpenAI",
        "openai",
        "https://api.openai.com/v1",
        "https://api.openai.com/v1/models",
        "sk-old",
        "gpt-4o",
        64_000,
    );
    app.open_providers();
    app.select_provider_at(1); // OpenAI: already the active preset
    app.set_settings_field("api_key", "sk-new");
    app.connect_preset().expect("connect");

    let (_, _, _, _, key) = app.config_provider();
    let (model, window) = app.config_model();
    assert_eq!(key, "sk-new", "the new key should be stored");
    assert_eq!(
        model, "gpt-4o",
        "changing only the key must not clear the chosen model"
    );
    assert_eq!(window, 64_000, "nor reset the context window");
}

/// Switching to a genuinely different provider SHOULD clear the model — a
/// model id from one provider is meaningless at another.
#[test]
fn switching_providers_clears_the_model_on_purpose() {
    let mut app = TestApp::new();
    app.seed_provider(
        "OpenAI",
        "openai",
        "https://api.openai.com/v1",
        "https://api.openai.com/v1/models",
        "sk-old",
        "gpt-4o",
        64_000,
    );
    app.open_providers();
    app.select_provider_at(2); // a different preset
    app.set_settings_field("api_key", "sk-other");
    app.connect_preset().expect("connect");
    let (model, _) = app.config_model();
    assert!(
        model.is_empty(),
        "a model id does not carry across providers, got {model:?}"
    );
}

/// A custom-endpoint user opening /provider and choosing "Custom" again is
/// editing what they already have, not starting over.
#[test]
fn reselecting_custom_keeps_the_endpoint_and_key() {
    let mut app = seeded(); // preset "custom"
    app.open_providers();
    app.select_provider_at(5); // the Custom row
    assert_eq!(
        app.settings_field("base_url"),
        "https://ai.enowx.id/v1",
        "picking Custom again must not blank the endpoint the user configured"
    );
    assert_eq!(app.settings_field("api_key"), "sk-secret");
    assert_eq!(app.settings_field("models_url"), "https://ai.enowx.id/v1/models");
    assert_eq!(app.settings_field("provider"), "enowx");
}

/// Typing in Base URL must not wipe the rest of the form. Correcting a typo
/// in the endpoint is the single most common edit, and it should not cost the
/// user their key, their model-list URL, and their chosen model.
#[test]
fn editing_the_base_url_does_not_clear_the_other_fields() {
    let mut app = seeded();
    app.open_settings();
    app.edit_settings_field("base_url", "https://ai.enowx.id/v2");

    assert_eq!(
        app.settings_field("api_key"),
        "sk-secret",
        "the API key belongs to the host, which has not changed"
    );
    assert_eq!(
        app.settings_field("models_url"),
        "https://ai.enowx.id/v1/models",
        "the model-list URL must not be blanked while typing"
    );
    assert_eq!(
        app.settings_field("model"),
        "cbc/deepseek-v4.1-flash",
        "the chosen model must survive an endpoint correction"
    );
}

/// Saving a form that already has a model should just save. Being thrown into
/// the model picker afterwards reads as "your model is gone, choose again".
#[test]
fn saving_with_a_model_already_set_closes_the_form() {
    let mut app = seeded();
    app.open_settings();
    app.edit_settings_field("base_url", "https://ai.enowx.id/v2");
    app.save_settings().expect("save");
    assert!(
        app.modal_closed(),
        "a complete configuration should save and close, not ask for a model"
    );
    let (model, _) = app.config_model();
    assert_eq!(model, "cbc/deepseek-v4.1-flash");
}

/// Moving to a different host is the case where the old key genuinely stops
/// applying — but that is decided at SAVE, against the finished URL, not
/// while the hostname is still being typed.
#[test]
fn moving_to_a_different_host_clears_the_key_on_save() {
    let mut app = seeded();
    app.open_settings();
    app.edit_settings_field("base_url", "https://someone-else.example/v1");
    assert_eq!(
        app.settings_field("api_key"),
        "sk-secret",
        "nothing is cleared while typing"
    );

    // Saving without a key for the new host is refused, which is the point:
    // the user is told, rather than silently pointed at a new service with
    // someone else's credentials.
    let result = app.save_settings();
    assert!(
        result.is_err() || app.config_provider().4.is_empty(),
        "a key from the previous host must not follow the user to a new one"
    );
}

/// A model-list URL that lives on the same host as the base URL should move
/// with it, rather than being thrown away and retyped.
#[test]
fn the_models_url_follows_a_host_change() {
    let mut app = seeded();
    app.open_settings();
    app.edit_settings_field("base_url", "https://new-host.example/v1");
    app.set_settings_field("api_key", "sk-for-new-host");
    app.save_settings().expect("save");
    let (_, _, _, models, _) = app.config_provider();
    assert_eq!(
        models, "https://new-host.example/v1/models",
        "the catalogue path should be re-pointed at the new host, not blanked"
    );
}

/// Every prefix of a URL is typed on the way to the full one. None of those
/// intermediate states may trigger the clearing cascade.
#[test]
fn typing_a_url_character_by_character_never_clears_the_key() {
    let mut app = seeded();
    app.open_settings();
    let target = "https://ai.enowx.id/v2/chat";
    for end in 1..=target.len() {
        app.edit_settings_field("base_url", &target[..end]);
        assert_eq!(
            app.settings_field("api_key"),
            "sk-secret",
            "the key was cleared while typing {:?}",
            &target[..end]
        );
    }
}
