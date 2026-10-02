//! Which providers exist, which of them are connected, and what a
//! `provider/model` ref points at.
//!
//! Several providers can be connected at once, each with its own key (`crate::auth`), and a model is always named
//! with its provider, `deepseek/deepseek-flash`, so one list holds the
//! models of all of them and each agent can run on any of them.

use std::fmt;

use super::{provider_preset, ProviderPreset, PROVIDER_PRESETS};
use crate::auth::KeySource;
use crate::catalog::{official, Catalog, CatalogModel};
use crate::config::{Config, ProviderEntry, DEFAULT_CONTEXT_WINDOW};

/// A model named with its provider.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModelRef {
    pub provider: String,
    /// The id the provider knows the model by, which may hold slashes of
    /// its own (`anthropic/claude-sonnet-4.5` on OpenRouter).
    pub model: String,
}

impl ModelRef {
    pub fn new(provider: &str, model: &str) -> Self {
        Self {
            provider: provider.trim().to_owned(),
            model: model.trim().to_owned(),
        }
    }

    /// `provider/model`, split at the first `/`.
    pub fn parse(raw: &str) -> Option<Self> {
        let (provider, model) = raw.trim().split_once('/')?;
        let (provider, model) = (provider.trim(), model.trim());
        (!provider.is_empty() && !model.is_empty()).then(|| Self::new(provider, model))
    }
}

impl fmt::Display for ModelRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.provider, self.model)
    }
}

/// A provider as enx will call it: its endpoint and its key, from wherever
/// each came.
#[derive(Debug, Clone, Default)]
pub struct Connection {
    pub id: String,
    pub name: String,
    /// OpenAI-compatible base URL, without `/chat/completions`.
    pub base_url: String,
    /// Where its model list is read from; empty when there is none.
    pub models_url: String,
    pub key: Option<String>,
    pub key_source: Option<KeySource>,
    /// The built-in provider this is, or is based on.
    pub preset: Option<ProviderPreset>,
    /// Whether `config.toml` has an entry for it.
    pub configured: bool,
}

impl Connection {
    fn host(&self) -> Option<String> {
        reqwest::Url::parse(self.base_url.trim())
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
    }

    /// An endpoint on this machine, which needs no key.
    pub fn is_local(&self) -> bool {
        matches!(
            self.host().as_deref(),
            Some("localhost" | "127.0.0.1" | "[::1]")
        )
    }

    /// Whether a model on this provider can be called: an endpoint, and a
    /// key unless the endpoint is on this machine.
    pub fn is_connected(&self) -> bool {
        !self.base_url.trim().is_empty() && (self.key.is_some() || self.is_local())
    }

    /// DeepSeek's own API: its preset, or any entry pointing at its host.
    pub fn is_deepseek(&self) -> bool {
        self.preset.is_some_and(|preset| preset.id == "deepseek")
            || self.host().as_deref() == Some("api.deepseek.com")
    }

    /// The provider's id in the models.dev catalogue, or "" for one it does
    /// not list (a custom endpoint, a gateway).
    pub fn catalog_id(&self) -> &'static str {
        if self.is_deepseek() {
            return "deepseek";
        }
        let id = self.preset.map(|preset| preset.id).unwrap_or("");
        if matches!(id, "openai" | "openrouter" | "groq") {
            id
        } else {
            ""
        }
    }
}

/// What is known about a model: its window, its prices, what it can do.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelFacts {
    pub context_window: u32,
    /// USD per 1M tokens; 0 is unknown.
    pub price_input: f64,
    pub price_output: f64,
    pub price_cache_read: f64,
    pub vision: bool,
    pub tool_call: bool,
    pub reasoning: bool,
}

impl Default for ModelFacts {
    fn default() -> Self {
        Self {
            context_window: DEFAULT_CONTEXT_WINDOW,
            price_input: 0.0,
            price_output: 0.0,
            price_cache_read: 0.0,
            vision: false,
            // An unlisted model is far more often able to call tools than
            // not, and turning tools off would quietly disable all of them.
            tool_call: true,
            reasoning: false,
        }
    }
}

impl Config {
    /// Every provider enx knows, built-in ones first: those, the ones in
    /// `config.toml`, and any set for this process only.
    pub fn connections(&self) -> Vec<Connection> {
        let mut ids: Vec<&str> = PROVIDER_PRESETS.iter().map(|preset| preset.id).collect();
        for id in self.provider.keys().chain(self.session_providers.keys()) {
            if !ids.contains(&id.as_str()) {
                ids.push(id);
            }
        }
        ids.into_iter()
            .filter_map(|id| self.connection(id))
            .collect()
    }

    /// The providers a model can be called on now.
    pub fn connected(&self) -> Vec<Connection> {
        self.connections()
            .into_iter()
            .filter(Connection::is_connected)
            .collect()
    }

    /// Provider `id` with its endpoint and key, or None when enx knows no
    /// provider by that id. A `config.toml` entry with a built-in id changes
    /// only the fields it sets.
    pub fn connection(&self, id: &str) -> Option<Connection> {
        let id = id.trim();
        let preset = provider_preset(id);
        let entry: Option<&ProviderEntry> = self
            .session_providers
            .get(id)
            .or_else(|| self.provider.get(id));
        if preset.is_none() && entry.is_none() {
            return None;
        }
        let pick = |own: Option<&String>, built_in: Option<&'static str>| -> String {
            own.map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .or(built_in)
                .unwrap_or("")
                .trim()
                .to_owned()
        };
        let base_url = pick(entry.map(|e| &e.base_url), preset.map(|p| p.base_url))
            .trim_end_matches('/')
            .to_owned();
        let models_url = pick(entry.map(|e| &e.models_url), preset.map(|p| p.models_url));
        let mut name = pick(entry.map(|e| &e.name), preset.map(|p| p.name));
        if name.is_empty() {
            name = id.to_owned();
        }
        let env = preset.map(|p| p.env).unwrap_or(&[]);
        let (key, key_source) = match self.auth.key(id, env) {
            Some((key, source)) => (Some(key), Some(source)),
            None => (None, None),
        };
        Some(Connection {
            id: id.to_owned(),
            name,
            base_url,
            models_url,
            key,
            key_source,
            preset,
            configured: self.provider.contains_key(id),
        })
    }

    /// The provider of the model in use.
    pub fn active_connection(&self) -> Option<Connection> {
        ModelRef::parse(&self.model.active).and_then(|model| self.connection(&model.provider))
    }

    /// `raw` as a model ref: `provider/model` when its first part names a
    /// provider enx knows. Anything else is a bare model id, the way older
    /// configurations wrote them, on the provider of the model in use.
    pub fn parse_model(&self, raw: &str) -> Option<ModelRef> {
        let raw = raw.trim();
        if raw.is_empty() {
            return None;
        }
        if let Some(model) = ModelRef::parse(raw) {
            if self.connection(&model.provider).is_some() {
                return Some(model);
            }
        }
        let active = ModelRef::parse(&self.model.active)?;
        Some(ModelRef::new(&active.provider, raw))
    }

    /// Make `raw` the model in use, with its window, prices and
    /// capabilities. False, and nothing changes, when it cannot be read as a
    /// model on a provider enx knows.
    ///
    /// The facts are replaced, never kept: the previous model's window or
    /// prices carried over read as real numbers for the new one.
    pub fn use_model(&mut self, raw: &str) -> bool {
        let Some(model) = self.parse_model(raw) else {
            return false;
        };
        let facts = self.facts(&model);
        self.model.active = model.to_string();
        self.model.set_facts(facts);
        self.model.efforts = self.efforts(&model);
        let chosen = crate::model_state::ModelState::load()
            .effort
            .get(&self.model.active)
            .cloned()
            .unwrap_or_default();
        self.model.effort = if self.model.efforts.contains(&chosen) {
            chosen
        } else {
            String::new()
        };
        true
    }

    /// Choose the thinking effort for the model in use, and keep it for the
    /// next time that model is picked. An empty `effort` goes back to the
    /// provider's default. Fails for a level the model does not offer.
    pub fn set_effort(&mut self, effort: &str) -> anyhow::Result<()> {
        let effort = effort.trim().to_ascii_lowercase();
        let effort = if effort == "default" {
            String::new()
        } else {
            effort
        };
        anyhow::ensure!(
            effort.is_empty() || self.model.efforts.contains(&effort),
            "{} offers {}",
            if self.model.active.is_empty() {
                "this model"
            } else {
                &self.model.active
            },
            if self.model.efforts.is_empty() {
                "no effort levels".to_owned()
            } else {
                self.model.efforts.join(", ")
            }
        );
        let mut state = crate::model_state::ModelState::load();
        if effort.is_empty() {
            state.effort.remove(&self.model.active);
        } else {
            state
                .effort
                .insert(self.model.active.clone(), effort.clone());
        }
        state.save()?;
        self.model.effort = effort;
        Ok(())
    }

    /// The thinking efforts `model` offers, from the catalogue listing of
    /// the provider that serves it, or DeepSeek's own figures.
    pub fn efforts(&self, model: &ModelRef) -> Vec<String> {
        let catalog_id = self
            .connection(&model.provider)
            .map(|connection| connection.catalog_id())
            .unwrap_or("");
        if let Some(own) = official(catalog_id, &model.model) {
            return own.efforts();
        }
        let catalog = Catalog::shared();
        let listed = if catalog_id.is_empty() {
            catalog.lookup(&model.model)
        } else {
            catalog.lookup_for(catalog_id, &model.model)
        };
        listed.map(CatalogModel::efforts).unwrap_or_default()
    }

    /// Call `model` at `base_url` with `api_key`, for this process only:
    /// an endpoint from the environment, or a test's fixture server.
    pub fn use_endpoint(&mut self, id: &str, base_url: &str, api_key: &str, model: &str) {
        self.session_providers.insert(
            id.to_owned(),
            ProviderEntry {
                name: id.to_owned(),
                base_url: base_url.trim().to_owned(),
                ..ProviderEntry::default()
            },
        );
        if !api_key.trim().is_empty() {
            self.auth
                .set_for_session(id, api_key.trim(), KeySource::Session);
        }
        self.use_model(&format!("{id}/{}", model.trim()));
    }

    /// What is known about `model` as its provider serves it, most specific
    /// first: its own entry in `config.toml`, what the provider's model list
    /// said, the provider's published figures, then the models.dev
    /// catalogue, asking that provider's listing before anyone else's.
    pub fn facts(&self, model: &ModelRef) -> ModelFacts {
        let catalog_id = self
            .connection(&model.provider)
            .map(|connection| connection.catalog_id())
            .unwrap_or("");
        let listed: Option<CatalogModel> = official(catalog_id, &model.model).or_else(|| {
            let catalog = Catalog::shared();
            if catalog_id.is_empty() {
                catalog.lookup(&model.model).cloned()
            } else {
                catalog.lookup_for(catalog_id, &model.model).cloned()
            }
        });
        let detected = super::detected::Detected::load()
            .model(&model.provider, &model.model)
            .and_then(|info| info.context_window);
        let own = self
            .provider
            .get(&model.provider)
            .and_then(|entry| entry.models.get(&model.model));
        let listed = listed.as_ref();
        let price = |own: Option<f64>, listed: Option<f64>| own.or(listed).unwrap_or(0.0);
        ModelFacts {
            context_window: own
                .and_then(|m| m.context_window)
                .or(detected)
                // A size in the id (`-1m`) names the variant being served,
                // which the catalogue may list only at its base window.
                .or_else(|| window_from_id(&model.model))
                .or_else(|| listed.map(|m| m.limit.context).filter(|c| *c > 0))
                .unwrap_or(DEFAULT_CONTEXT_WINDOW),
            price_input: price(
                own.and_then(|m| m.price_input),
                listed.map(|m| m.cost.input),
            ),
            price_output: price(
                own.and_then(|m| m.price_output),
                listed.map(|m| m.cost.output),
            ),
            price_cache_read: price(
                own.and_then(|m| m.price_cache_read),
                listed.and_then(|m| m.cost.cache_read),
            ),
            vision: own
                .and_then(|m| m.vision)
                .unwrap_or_else(|| listed.is_some_and(|m| m.modalities.supports_vision())),
            tool_call: listed.map(|m| m.tool_call).unwrap_or(true),
            reasoning: listed.is_some_and(|m| m.reasoning),
        }
    }
}

/// A context window read from the model id itself, when the provider and the
/// catalogue give none. A model names its window in its id often enough to
/// trust it: a `-1m` suffix is a million tokens, `-200k` is two hundred
/// thousand. Without this a `claude-opus-4.7-1m` whose provider omits the
/// window falls back to 128k and the whole budget is wrong.
///
/// Read only a size *token* (a number right before `k` or `m`, bounded by the
/// id's edges or a separator), never a bare number, so `gpt-4` is not read as
/// 4 tokens and `claude-4.7` is not read as 7. The largest plausible token
/// wins, so `...-1m` beats a `4` elsewhere in the id.
fn window_from_id(model: &str) -> Option<u32> {
    let id = model.to_ascii_lowercase();
    let bytes = id.as_bytes();
    let mut best: Option<u32> = None;
    let sep = |b: u8| !b.is_ascii_alphanumeric() && b != b'.';
    let mut i = 0;
    while i < bytes.len() {
        // A size token starts at a word boundary with a digit.
        let boundary = i == 0 || sep(bytes[i - 1]);
        if !(boundary && bytes[i].is_ascii_digit()) {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
            i += 1;
        }
        // The unit must be k or m, and the token must end right after it.
        let unit = bytes.get(i).copied();
        let ends = bytes.get(i + 1).map(|b| sep(*b)).unwrap_or(true);
        let scale = match unit {
            Some(b'k') if ends => 1_000u64,
            Some(b'm') if ends => 1_000_000u64,
            _ => continue,
        };
        if let Ok(value) = id[start..i].parse::<f64>() {
            let window = (value * scale as f64).round();
            // Only a plausible window: 8k and up, 2M and down.
            if (8_000.0..=2_000_000.0).contains(&window) {
                let window = window as u32;
                best = Some(best.map_or(window, |b| b.max(window)));
            }
        }
        i += 1;
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_is_read_from_a_size_suffix_in_the_id() {
        assert_eq!(
            window_from_id("enowx/cb/claude-opus-4.7-1m"),
            Some(1_000_000)
        );
        assert_eq!(window_from_id("claude-sonnet-4.5-200k"), Some(200_000));
        assert_eq!(window_from_id("some-model-128k"), Some(128_000));
        assert_eq!(window_from_id("qwen-2.5-1m-instruct"), Some(1_000_000));
        // No size token: a bare version number is not a window.
        assert_eq!(window_from_id("gpt-4"), None);
        assert_eq!(window_from_id("claude-opus-4.7"), None);
        assert_eq!(window_from_id("deepseek-v4.1-flash"), None);
        // A digit glued to a word is not a size token.
        assert_eq!(window_from_id("model-4ktest"), None);
        assert_eq!(window_from_id("gpt4000-turbo"), None);
        // Out of range is ignored.
        assert_eq!(window_from_id("weird-5m"), None);
        assert_eq!(window_from_id("tiny-2k"), None);
        // The largest plausible token wins.
        assert_eq!(window_from_id("a-32k-b-1m"), Some(1_000_000));
    }

    #[test]
    fn a_ref_splits_at_the_first_slash() {
        let model = ModelRef::parse("openrouter/anthropic/claude-sonnet-4.5").unwrap();
        assert_eq!(model.provider, "openrouter");
        assert_eq!(model.model, "anthropic/claude-sonnet-4.5");
        assert_eq!(model.to_string(), "openrouter/anthropic/claude-sonnet-4.5");
        assert!(ModelRef::parse("bare-model").is_none());
        assert!(ModelRef::parse("/model").is_none());
        assert!(ModelRef::parse("provider/").is_none());
    }

    #[test]
    fn a_built_in_provider_is_known_without_configuration() {
        let config = Config::default();
        let deepseek = config.connection("deepseek").expect("built in");
        assert_eq!(deepseek.base_url, "https://api.deepseek.com");
        assert_eq!(deepseek.catalog_id(), "deepseek");
        assert!(config.connection("nowhere").is_none());
    }

    #[test]
    fn a_configured_entry_changes_only_what_it_sets() {
        let mut config = Config::default();
        config.provider.insert(
            "openai".into(),
            ProviderEntry {
                base_url: "https://proxy.example/v1/".into(),
                ..ProviderEntry::default()
            },
        );
        let openai = config.connection("openai").unwrap();
        assert_eq!(openai.base_url, "https://proxy.example/v1");
        assert_eq!(openai.name, "OpenAI", "the name is still the built-in one");
        assert!(openai.configured);
    }

    #[test]
    fn a_bare_id_belongs_to_the_provider_in_use() {
        let mut config = Config::default();
        config.use_endpoint("gateway", "http://127.0.0.1:9", "", "first");
        assert_eq!(config.model.active, "gateway/first");
        let model = config.parse_model("cbc/deepseek-v4.1-flash").unwrap();
        assert_eq!(model.provider, "gateway", "`cbc` is not a provider");
        assert_eq!(model.model, "cbc/deepseek-v4.1-flash");
        let model = config.parse_model("deepseek/deepseek-flash").unwrap();
        assert_eq!(
            model.provider, "deepseek",
            "a known provider is the provider"
        );
    }

    #[test]
    fn a_local_endpoint_needs_no_key() {
        let mut config = Config::default();
        config.use_endpoint("local", "http://localhost:11434/v1", "", "llama");
        assert!(config.active_connection().unwrap().is_connected());
    }

    /// Choosing a model on DeepSeek's own API takes DeepSeek's published
    /// figures, whatever the public catalogue says.
    #[test]
    fn a_deepseek_model_takes_deepseeks_own_figures() {
        let mut config = Config::default();
        config
            .auth
            .set_for_session("deepseek", "k", KeySource::Session);
        assert!(config.use_model("deepseek/deepseek-v4-pro"));
        assert_eq!(config.model.price_input, 0.66);
        assert_eq!(config.model.price_output, 1.98);
        assert_eq!(config.model.price_cache_read, 0.022);
        assert_eq!(config.model.context_window, 1_000_000);
        assert!(!config.model.vision);
        assert!(config.model.reasoning && config.model.tool_call);

        assert!(config.use_model("deepseek/deepseek-flash"));
        assert_eq!(config.model.price_input, 0.15);
        assert!(config.model.vision, "Flash takes images");
    }

    /// Switching models must not carry the old one's numbers across: that
    /// is how a 128k window came to be shown for a 1M-context model.
    #[test]
    fn switching_models_replaces_the_facts() {
        let mut config = Config::default();
        config.use_endpoint("gateway", "http://127.0.0.1:9", "", "first");
        config.model.context_window = 999_999;
        config.model.price_input = 12.5;
        config.model.reasoning = true;
        assert!(config.use_model("gateway/definitely-not-a-real-model-xyz"));
        assert_eq!(config.model.context_window, DEFAULT_CONTEXT_WINDOW);
        assert_eq!(config.model.price_input, 0.0);
        assert!(!config.model.reasoning);
        assert!(config.model.tool_call, "an unknown model keeps its tools");
    }

    /// A window the user wrote for a model wins over every other source.
    #[test]
    fn a_models_own_entry_wins() {
        let mut config = Config::default();
        let mut entry = ProviderEntry {
            base_url: "http://127.0.0.1:9".into(),
            ..ProviderEntry::default()
        };
        entry.models.insert(
            "mine".into(),
            crate::config::ModelEntry {
                context_window: Some(42_000),
                ..Default::default()
            },
        );
        config.provider.insert("gateway".into(), entry);
        assert!(config.use_model("gateway/mine"));
        assert_eq!(config.model.context_window, 42_000);
    }
}
