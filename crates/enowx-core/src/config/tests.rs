use super::*;
use crate::agent_def::Tier;
use std::sync::Mutex;

/// Tests that point `ENX_HOME` somewhere take turns: it is one variable for
/// the whole process.
static HOME: Mutex<()> = Mutex::new(());

/// A fresh `ENX_HOME` for one test, removed afterwards.
struct Home {
    path: PathBuf,
    _turn: std::sync::MutexGuard<'static, ()>,
}

impl Home {
    fn new(tag: &str) -> Self {
        let turn = HOME.lock().unwrap_or_else(|e| e.into_inner());
        let path = std::env::temp_dir().join(format!(
            "enx-config-{tag}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&path).unwrap();
        std::env::set_var("ENX_HOME", &path);
        for name in ["ENX_MODEL", "ENX_BASE_URL", "ENX_API_KEY"] {
            std::env::remove_var(name);
        }
        Self { path, _turn: turn }
    }

    fn write(&self, name: &str, text: &str) {
        std::fs::write(self.path.join(name), text).unwrap();
    }

    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.path.join(name)).unwrap_or_default()
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[test]
fn set_keeps_scalar_types() {
    let mut config = Config::default();
    config.set("agent.shell_timeout_secs", "90").unwrap();
    assert_eq!(config.agent.shell_timeout_secs, 90);
    assert!(config.set("model.nope", "x").is_err());
}

/// A table from an older version, such as one holding a key for a feature
/// since removed, loads without error, survives a save untouched, and is
/// never echoed or set.
#[test]
fn a_table_this_version_does_not_know_is_kept_and_hidden() {
    let home = Home::new("unknown-table");
    home.write(
        "config.toml",
        "[ui]\ntheme = \"dark\"\n\n[retired]\napi_key = \"secret\"\ntimeout_ms = 1500\n",
    );
    let mut config = Config::load().expect("an unknown table must not stop the load");
    assert_eq!(config.get("retired.api_key"), None);
    assert!(config.set("retired.api_key", "other").is_err());
    config.set("ui.theme", "light").unwrap();
    config.save().unwrap();
    let saved: toml::Table = toml::from_str(&home.read("config.toml")).unwrap();
    assert_eq!(saved["retired"]["api_key"].as_str(), Some("secret"));
    assert_eq!(saved["retired"]["timeout_ms"].as_integer(), Some(1500));
    assert_eq!(saved["ui"]["theme"].as_str(), Some("light"));
}

#[test]
fn a_pinned_model_is_written_with_its_provider() {
    let mut config = Config::default();
    assert!(
        config.set("model.default", "no-provider-model").is_err(),
        "a bare id with no model in use names no provider"
    );
    config
        .set("model.default", "deepseek/deepseek-flash")
        .unwrap();
    assert_eq!(config.model.default, "deepseek/deepseek-flash");
    assert_eq!(config.model.active, "deepseek/deepseek-flash");
    config.set("model.default", "deepseek-v4-pro").unwrap();
    assert_eq!(
        config.model.default, "deepseek/deepseek-v4-pro",
        "a bare id is pinned on the provider in use"
    );
    config.set("model.default", "").unwrap();
    assert!(config.model.default.is_empty(), "an empty value unpins");
    let text = toml::to_string_pretty(&config).unwrap();
    assert!(
        !text.contains("default ="),
        "nothing pinned, nothing written"
    );
}

/// The model in use and the keys are not in the file; a `set` of anything
/// else must not drop them.
#[test]
fn a_set_keeps_what_the_file_does_not_hold() {
    let mut config = Config::default();
    config.use_endpoint("gateway", "http://127.0.0.1:9", "sk-test", "first");
    config.set("agent.max_steps", "12").unwrap();
    assert_eq!(config.model.active, "gateway/first");
    assert!(config.is_ready(), "the key and the endpoint survive");
}

#[test]
fn a_provider_is_set_field_by_field() {
    let mut config = Config::default();
    config
        .set("provider.enowx.base_url", "https://ai.enowx.id/v1/")
        .unwrap();
    config
        .set("provider.enowx.models_url", "https://ai.enowx.id/v1/models")
        .unwrap();
    config
        .set(
            "provider.enowx.models.cbc/deepseek-v4.1-flash.context_window",
            "200000",
        )
        .unwrap();
    let entry = &config.provider["enowx"];
    assert_eq!(entry.base_url, "https://ai.enowx.id/v1");
    assert_eq!(
        entry.models["cbc/deepseek-v4.1-flash"].context_window,
        Some(200_000),
        "a model id with dots and slashes keeps them"
    );
    let error = config
        .set("provider.enowx.api_key", "sk-x")
        .unwrap_err()
        .to_string();
    assert!(error.contains("enx auth login enowx"), "{error}");
    assert!(config.set("provider.Bad Id.base_url", "https://x").is_err());
    assert!(config.set("provider.enowx.base_url", "ftp://x").is_err());

    config
        .set("provider.enowx.models.cbc/deepseek-v4.1-flash", "")
        .unwrap();
    config.set("provider.enowx.models_url", "").unwrap();
    config.set("provider.enowx.base_url", "").unwrap();
    assert!(
        !config.provider.contains_key("enowx"),
        "an entry left empty is removed"
    );
}

/// Set every level at once: a resolution order that reads the wrong slot
/// first still returns *a* model, so only a populated ladder catches it.
#[test]
fn the_most_specific_model_wins() {
    let mut config = Config::default();
    config.model.active = "active/model".into();
    config.agent.tiers.strong = "tier/strong".into();
    config.agent.tiers.cheap = "tier/cheap".into();
    config
        .agent
        .models
        .insert("fe".into(), "override/fe".into());

    assert_eq!(config.model_for("fe", Tier::Strong), "override/fe");
    assert_eq!(config.model_for("be", Tier::Strong), "tier/strong");
    assert_eq!(config.model_for("be", Tier::Cheap), "tier/cheap");
    assert_eq!(
        config.model_for("be", Tier::Balanced),
        "active/model",
        "an unmapped tier falls back rather than failing"
    );
}

#[test]
fn blank_entries_fall_through() {
    let mut config = Config::default();
    config.model.active = "active/model".into();
    config.agent.tiers.balanced = "   ".into();
    config.agent.models.insert("fe".into(), String::new());
    assert_eq!(config.model_for("fe", Tier::Balanced), "active/model");

    config.agent.tiers.balanced = "tier/balanced".into();
    assert_eq!(
        config.model_for("fe", Tier::Balanced),
        "tier/balanced",
        "a blank override must not mask the tier below it"
    );
}

#[test]
fn an_agent_is_given_its_own_model_by_name() {
    let mut config = Config::default();
    config.model.active = "active/model".into();
    config
        .set("agent.models.fe", "openrouter/vendor/design-model")
        .unwrap();
    assert_eq!(
        config.model_for("fe", Tier::Strong),
        "openrouter/vendor/design-model"
    );
    assert_eq!(config.model_for("be", Tier::Strong), "active/model");
    config.set("agent.models.fe", "").unwrap();
    assert!(config.agent.models.is_empty(), "an empty value clears it");
    assert!(config.set("agent.nope", "1").is_err());
    assert!(config.set("agent.tiers.huge", "x").is_err());
}

/// Configs written before the tier table existed have to keep loading.
#[test]
fn a_config_without_the_agent_tables_parses() {
    let config: Config = toml::from_str("[agent]\nmax_steps = 8\n").expect("loads");
    assert_eq!(config.agent.max_steps, 8);
    assert!(config.agent.models.is_empty());
    assert_eq!(
        config.model.context_window, DEFAULT_CONTEXT_WINDOW,
        "figures not in the file start from the defaults"
    );
    assert!(config.model.tool_call);
}

#[test]
fn switching_is_automatic_unless_turned_off() {
    assert!(Config::default().agent.auto_switch);
    let config: Config = toml::from_str("[agent]\nauto_switch = false\n").unwrap();
    assert!(!config.agent.auto_switch);
}

#[test]
fn workspace_escape_is_rejected() {
    let root = PathBuf::from("/tmp/enx-root");
    assert!(resolve_in_workspace(&root, "src/main.rs").is_ok());
    assert!(resolve_in_workspace(&root, "../etc/passwd").is_err());
    assert!(resolve_in_workspace(&root, "/etc/passwd").is_err());
}

#[test]
fn a_provider_id_is_made_from_a_name() {
    assert_eq!(provider_id_from("My Gateway!"), "my-gateway");
    assert_eq!(provider_id_from("enowx"), "enowx");
    assert!(valid_provider_id("enowx"));
    assert!(valid_provider_id("my_gw-2"));
    assert!(!valid_provider_id("Enowx"));
    assert!(!valid_provider_id("-x"));
    assert!(!valid_provider_id(ENV_PROVIDER));
}

/// The configuration enx wrote before providers had ids: one provider, its
/// key in the file, a bare model id. It moves to the new layout on load,
/// losing nothing, with the old file kept.
#[test]
fn an_old_configuration_moves_to_the_new_layout() {
    let home = Home::new("legacy");
    home.write(
        "config.toml",
        r#"
[model]
default = "cbc/deepseek-v4.1-flash"
context_window = 128000
price_input = 0.15

[provider]
name = "enowx"
preset = "custom"
base_url = "https://ai.enowx.id/v1"
models_url = "https://ai.enowx.id/v1/models"
api_key = "sk-legacy"

[agent]
max_steps = 7

[agent.models]
fe = "cbc/glm-5"

[agent.tiers]
cheap = ""
strong = "cbc/strong-one"
"#,
    );
    let config = Config::load().expect("an old configuration loads");

    assert_eq!(config.model.active, "enowx/cbc/deepseek-v4.1-flash");
    assert!(config.is_ready(), "the key came across");
    assert_eq!(config.agent.max_steps, 7, "unrelated settings are kept");
    assert_eq!(config.agent.models["fe"], "enowx/cbc/glm-5");
    assert_eq!(config.agent.tiers.strong, "enowx/cbc/strong-one");
    assert_eq!(config.agent.tiers.cheap, "", "an empty slot stays empty");

    let written = home.read("config.toml");
    assert!(
        !written.contains("sk-legacy"),
        "no key in config.toml:\n{written}"
    );
    assert!(written.contains("[provider.enowx]"), "{written}");
    assert!(
        !written.contains("default ="),
        "the model is not pinned:\n{written}"
    );
    assert!(home.read("auth.json").contains("sk-legacy"));
    assert!(home
        .read("model.json")
        .contains("enowx/cbc/deepseek-v4.1-flash"));
    assert!(
        home.read("config.toml.before-providers.bak")
            .contains("sk-legacy"),
        "the old file is kept as it was"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(home.path.join("auth.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "keys are readable by the user alone");
    }

    // A second load finds the new layout and changes nothing.
    let before = home.read("config.toml");
    let again = Config::load().unwrap();
    assert_eq!(again.model.active, "enowx/cbc/deepseek-v4.1-flash");
    assert_eq!(home.read("config.toml"), before);
}

/// A built-in provider moves with no entry of its own: its endpoint is the
/// built-in one.
#[test]
fn an_old_built_in_provider_needs_no_entry() {
    let home = Home::new("legacy-preset");
    home.write(
        "config.toml",
        r#"
[model]
default = "deepseek-flash"

[provider]
name = "DeepSeek"
preset = "deepseek"
base_url = "https://api.deepseek.com"
models_url = "https://api.deepseek.com/models"
api_key = "sk-deepseek"
"#,
    );
    let config = Config::load().unwrap();
    assert_eq!(config.model.active, "deepseek/deepseek-flash");
    assert!(config.provider.is_empty(), "{:?}", config.provider);
    assert_eq!(config.model.price_input, 0.15, "DeepSeek's own figures");
}

/// Connecting a second provider keeps the first, and the recent pick is
/// the one enx starts on.
#[test]
fn providers_are_kept_side_by_side() {
    let home = Home::new("side-by-side");
    let mut config = Config::load().unwrap();
    config.auth.store("deepseek", "sk-one").unwrap();
    config
        .set("provider.enowx.base_url", "https://ai.enowx.id/v1")
        .unwrap();
    config.save().unwrap();
    config.auth.store("enowx", "sk-two").unwrap();

    let mut state = crate::model_state::ModelState::default();
    state.remember("deepseek/deepseek-flash");
    state.remember("enowx/cbc/glm-5");
    state.save().unwrap();

    let config = Config::load().unwrap();
    let connected: Vec<String> = config.connected().into_iter().map(|c| c.id).collect();
    assert!(connected.contains(&"deepseek".to_owned()), "{connected:?}");
    assert!(connected.contains(&"enowx".to_owned()), "{connected:?}");
    assert_eq!(config.model.active, "enowx/cbc/glm-5", "the latest pick");
    assert!(!home.read("config.toml").contains("sk-"));
}

/// A recent pick on a provider that has since lost its key is passed over
/// for the next one that can run.
#[test]
fn a_disconnected_pick_is_passed_over() {
    let home = Home::new("passed-over");
    let mut config = Config::load().unwrap();
    config.auth.store("deepseek", "sk-one").unwrap();
    let mut state = crate::model_state::ModelState::default();
    state.remember("deepseek/deepseek-flash");
    state.remember("openrouter/some/model");
    state.save().unwrap();
    let config = Config::load().unwrap();
    assert_eq!(config.model.active, "deepseek/deepseek-flash");
    drop(home);
}

/// A pinned `model.default` beats the recent list.
#[test]
fn a_pinned_model_beats_the_recent_one() {
    let home = Home::new("pinned");
    home.write(
        "config.toml",
        "[model]\ndefault = \"deepseek/deepseek-v4-pro\"\n",
    );
    let mut config = Config::load().unwrap();
    config.auth.store("deepseek", "sk-one").unwrap();
    let mut state = crate::model_state::ModelState::default();
    state.remember("deepseek/deepseek-flash");
    state.save().unwrap();
    let config = Config::load().unwrap();
    assert_eq!(config.model.active, "deepseek/deepseek-v4-pro");
    drop(home);
}

/// A container sets an endpoint, a key and a model in the environment and
/// writes nothing.
#[test]
fn an_endpoint_from_the_environment_is_never_written() {
    let home = Home::new("env");
    std::env::set_var("ENX_BASE_URL", "https://gateway.example/v1");
    std::env::set_var("ENX_API_KEY", "sk-env");
    std::env::set_var("ENX_MODEL", "some/model");
    let config = Config::load();
    for name in ["ENX_MODEL", "ENX_BASE_URL", "ENX_API_KEY"] {
        std::env::remove_var(name);
    }
    let config = config.unwrap();
    assert_eq!(config.model.active, "env/some/model");
    assert!(config.is_ready());
    config.save().unwrap();
    let written = home.read("config.toml");
    assert!(!written.contains("gateway.example"), "{written}");
    assert!(!written.contains("sk-env"), "{written}");
    assert!(home.read("auth.json").is_empty());
}

/// A model's thinking efforts come from its models.dev entry, and the one
/// chosen is kept for that model in `model.json`.
#[test]
fn thinking_efforts_come_from_the_catalogue_and_the_choice_is_kept() {
    let home = Home::new("effort");
    let fetched = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    home.write(
        "models.json",
        &serde_json::json!({
            "schema": crate::catalog::CATALOG_SCHEMA,
            "fetched_at": fetched,
            "openai": { "id": "openai", "models": {
                "gpt-5": { "id": "gpt-5", "reasoning": true, "reasoning_options": [
                    { "type": "effort", "values": ["minimal", "low", "medium", "high"] }
                ]},
                "gpt-4o": { "id": "gpt-4o", "reasoning": false }
            }}
        })
        .to_string(),
    );
    let mut config = Config::default();
    assert!(config.use_model("openai/gpt-5"));
    assert_eq!(config.model.efforts, ["minimal", "low", "medium", "high"]);
    assert_eq!(config.model.effort, "");
    config.set_effort("high").unwrap();
    assert!(home
        .read("model.json")
        .contains("\"openai/gpt-5\": \"high\""));
    assert!(
        config.set_effort("xhigh").is_err(),
        "not a level gpt-5 offers"
    );

    let mut again = Config::default();
    assert!(again.use_model("openai/gpt-5"));
    assert_eq!(again.model.effort, "high", "kept for the model");
    assert!(again.use_model("openai/gpt-4o"));
    assert!(again.model.efforts.is_empty() && again.model.effort.is_empty());
    again.use_model("openai/gpt-5");
    again.set_effort("default").unwrap();
    assert!(!home.read("model.json").contains("openai/gpt-5\": "));
}

/// The footer and the turn agree on the model: an agent's own model when it
/// can run, the conversation's when it cannot.
#[test]
fn the_running_model_is_the_agents_own_when_it_can_run() {
    let mut config = Config::default();
    config
        .set("provider.lab.base_url", "http://127.0.0.1:11434/v1")
        .unwrap();
    config.model.active = "lab/chat".into();
    assert_eq!(
        config.running_model("orchestrator", Tier::Strong),
        "lab/chat"
    );
    assert!(!config.agent_has_own_model("orchestrator", Tier::Strong));

    config
        .agent
        .models
        .insert("orchestrator".into(), "lab/planner".into());
    assert_eq!(
        config.running_model("orchestrator", Tier::Strong),
        "lab/planner"
    );
    assert!(config.agent_has_own_model("orchestrator", Tier::Strong));

    // The same model as the conversation's is still the agent's own: a
    // later /model must change it too, or the agent stays where it was.
    config
        .agent
        .models
        .insert("orchestrator".into(), "lab/chat".into());
    assert!(config.agent_has_own_model("orchestrator", Tier::Strong));

    // A model on a provider with no key falls back, and does not count as
    // its own.
    config
        .set("provider.remote.base_url", "https://models.invalid/v1")
        .unwrap();
    config
        .agent
        .models
        .insert("orchestrator".into(), "remote/planner".into());
    assert_eq!(
        config.running_model("orchestrator", Tier::Strong),
        "lab/chat"
    );
    assert!(!config.agent_has_own_model("orchestrator", Tier::Strong));
}
