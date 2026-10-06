//! `/model` lists the models of every connected provider in one place, the
//! favourite and recent ones first, and remembers a pick in `model.json`
//! rather than rewriting `config.toml`.

use crossterm::event::KeyCode;
use enowx_tui::testing::TestApp;

/// Two connected providers with known lists, a model on the gateway in use.
fn two_providers() -> TestApp {
    let mut app = TestApp::new();
    app.seed_key("deepseek", "sk-deepseek");
    app.seed_provider(
        "enowx",
        "https://ai.enowx.id/v1",
        "https://ai.enowx.id/v1/models",
        "sk-gateway",
        "cbc/glm-5",
    );
    app.seed_listing(
        "enowx",
        &[("cbc/glm-5", Some(200_000)), ("cbc/kimi-k2", None)],
    );
    app.seed_listing(
        "deepseek",
        &[("deepseek-flash", None), ("deepseek-v4-pro", None)],
    );
    app
}

#[test]
fn every_connected_provider_has_its_models_listed() {
    let mut app = two_providers();
    app.open_model_picker();
    let rows = app.picker_rows();
    assert!(rows.contains(&"# DeepSeek".to_owned()), "{rows:#?}");
    assert!(rows.contains(&"# enowx".to_owned()), "{rows:#?}");
    assert!(rows
        .iter()
        .any(|row| row.starts_with("deepseek/deepseek-v4-pro |")));
    assert!(rows
        .iter()
        .any(|row| row == "enowx/cbc/glm-5 | cbc/glm-5 | 200k context · in use"));
    assert!(
        !rows.iter().any(|row| row.starts_with("# OpenAI")),
        "a provider with no key is not listed"
    );
    assert_eq!(
        app.selected_model().as_deref(),
        Some("enowx/cbc/glm-5"),
        "the list opens on the model in use"
    );
}

#[test]
fn a_pick_is_remembered_not_written_to_the_config() {
    let mut app = two_providers();
    app.open_model_picker();
    app.type_keys("v4-pro");
    assert_eq!(
        app.selected_model().as_deref(),
        Some("deepseek/deepseek-v4-pro")
    );
    app.press_key(KeyCode::Enter).expect("choose");
    assert_eq!(app.active_model(), "deepseek/deepseek-v4-pro");
    assert!(app.modal_closed());
    assert_eq!(
        app.model_state().recent.first().map(String::as_str),
        Some("deepseek/deepseek-v4-pro")
    );

    // Opened again, the pick heads the recent list.
    app.open_model_picker();
    let rows = app.picker_rows();
    let recent = rows
        .iter()
        .position(|row| row == "# Recent")
        .expect("recent");
    assert!(rows[recent + 1].starts_with("deepseek/deepseek-v4-pro |"));
}

#[test]
fn the_search_narrows_the_list_and_esc_clears_it() {
    let mut app = two_providers();
    app.open_model_picker();
    app.type_keys("kimi");
    let models: Vec<String> = app
        .picker_rows()
        .into_iter()
        .filter(|row| !row.starts_with('#') && !row.starts_with('-'))
        .collect();
    assert_eq!(models.len(), 1, "{models:?}");
    assert!(models[0].starts_with("enowx/cbc/kimi-k2 |"));

    app.press_key(KeyCode::Esc).expect("clear the search");
    assert!(!app.modal_closed(), "the first Esc clears the search");
    assert!(app.picker_rows().len() > 3);
    // Chat-launched pickers close directly after their search is cleared.
    app.press_key(KeyCode::Esc).expect("close the picker");
    assert!(app.modal_closed());
}

#[test]
fn a_favourite_is_listed_first() {
    let mut app = two_providers();
    app.open_model_picker();
    app.type_keys("flash");
    app.press(KeyCode::Char('f'), true).expect("ctrl+f");
    assert_eq!(app.model_state().favorite, ["deepseek/deepseek-flash"]);
    app.press_key(KeyCode::Esc).expect("clear the search");
    let rows = app.picker_rows();
    assert_eq!(rows[0], "# Favourites");
    assert!(rows[1].starts_with("deepseek/deepseek-flash | ★ deepseek-flash"));
}

#[test]
fn the_arrows_pass_over_headings() {
    let mut app = two_providers();
    app.open_model_picker();
    for _ in 0..10 {
        app.press_key(KeyCode::Down).expect("down");
        assert!(app.selected_model().is_some(), "always on a model");
    }
    for _ in 0..10 {
        app.press_key(KeyCode::Up).expect("up");
        assert!(app.selected_model().is_some(), "always on a model");
    }
}

#[test]
fn a_model_added_by_hand_is_saved_and_used() {
    let mut app = two_providers();
    app.open_model_picker();
    app.type_keys("glm");
    app.press(KeyCode::Char('n'), true).expect("add by hand");
    app.set_settings_field("model", "cbc/new-one");
    app.set_settings_field("context_window", "256,000");
    app.submit_form().expect("add");
    assert_eq!(app.active_model(), "enowx/cbc/new-one");
    assert_eq!(app.context_window(), 256_000);
    let entry = app.provider_entry("enowx").expect("entry");
    assert_eq!(entry.models["cbc/new-one"].context_window, Some(256_000));
}

#[test]
fn a_model_on_a_provider_with_no_key_is_refused() {
    let mut app = two_providers();
    let error = app
        .run_command("/model openai/gpt-4o")
        .expect_err("no OpenAI key")
        .to_string();
    assert!(error.contains("OpenAI is not connected"), "{error}");
    assert_eq!(app.active_model(), "enowx/cbc/glm-5");
}

/// A provider with no model list (the test app's own, on this machine)
/// says how to add a model rather than showing an empty section.
#[test]
fn a_provider_with_no_list_offers_to_add_a_model_by_hand() {
    let mut app = TestApp::new();
    app.open_model_picker();
    let rows = app.picker_rows();
    assert!(
        rows.iter()
            .any(|row| row.contains("Ctrl+N adds one by hand")),
        "{rows:#?}"
    );
}

/// In `/agent`, `m` chooses the selected agent's own model and `d` drops
/// it. The conversation's model stays as it was.
#[test]
fn an_agent_gets_a_model_of_its_own_from_the_roster() {
    let mut app = two_providers();
    app.run_command("/agent").expect("roster");
    while app.modal_cursor() > 0 {
        app.press_key(KeyCode::Up).expect("up");
    }
    while app.selected_agent_row().as_deref() != Some("fe") {
        let before = app.modal_cursor();
        app.press_key(KeyCode::Down).expect("down");
        assert_ne!(app.modal_cursor(), before, "fe is in the roster");
    }
    app.press_key(KeyCode::Char('m')).expect("m");
    let screen = app.render_to_text(120, 40).join("\n");
    assert!(screen.contains("MODEL FOR FRONTEND"), "{screen}");
    app.type_keys("v4-pro");
    app.press_key(KeyCode::Enter).expect("pick");
    assert_eq!(
        app.agent_own_model("fe").as_deref(),
        Some("deepseek/deepseek-v4-pro")
    );
    assert_eq!(
        app.active_model(),
        "enowx/cbc/glm-5",
        "the conversation keeps its model"
    );
    assert_eq!(
        app.selected_agent_row().as_deref(),
        Some("fe"),
        "back on the roster, on fe"
    );
    let screen = app.render_to_text(120, 60).join("\n");
    assert!(screen.contains("deepseek/deepseek-v4-pro"), "{screen}");

    app.press_key(KeyCode::Char('d')).expect("d");
    assert_eq!(app.agent_own_model("fe"), None);
}

/// Esc from the model list while choosing for an agent goes back to the
/// roster and changes nothing.
#[test]
fn leaving_an_agents_model_list_changes_nothing() {
    let mut app = two_providers();
    app.run_command("/agent").expect("roster");
    let agent = app.selected_agent_row().expect("a row");
    app.press_key(KeyCode::Char('m')).expect("m");
    app.press_key(KeyCode::Esc).expect("esc");
    assert_eq!(app.selected_agent_row(), Some(agent.clone()));
    assert_eq!(app.agent_own_model(&agent), None);
    assert_eq!(app.active_model(), "enowx/cbc/glm-5");
}

#[test]
fn settings_model_picker_cancel_restores_origin_query_and_row() {
    let mut app = two_providers();
    app.run_command("/settings").expect("settings");
    app.type_keys("model:choose");
    assert_eq!(app.settings_row_id().as_deref(), Some("model:choose"));
    assert_eq!(app.settings_query(), "model:choose");
    app.press_key(KeyCode::Enter).expect("open model picker");
    app.press_key(KeyCode::Esc).expect("cancel model picker");
    assert_eq!(app.active_tab(), "Settings");
    assert_eq!(app.settings_row_id().as_deref(), Some("model:choose"));
    assert_eq!(app.settings_query(), "model:choose");
}

#[test]
fn settings_agent_model_pick_restores_origin_query_and_row() {
    let mut app = two_providers();
    app.run_command("/settings").expect("settings");
    app.type_keys("frontend");
    assert_eq!(app.settings_row_id().as_deref(), Some("agent:fe"));
    app.press_key(KeyCode::Esc)
        .expect("clear filter, retain row");
    assert_eq!(app.settings_query(), "");
    app.press_key(KeyCode::Char('m'))
        .expect("choose agent model");
    app.type_keys("v4-pro");
    app.press_key(KeyCode::Enter).expect("pick agent model");
    assert_eq!(
        app.agent_own_model("fe").as_deref(),
        Some("deepseek/deepseek-v4-pro")
    );
    assert_eq!(app.active_tab(), "Settings");
    assert_eq!(app.settings_row_id().as_deref(), Some("agent:fe"));
    assert_eq!(app.settings_query(), "");
}
