//! On-disk configuration: `~/.enx/config.toml`, plus environment overrides
//! so a container can run without writing anything.
//!
//! Provider keys are not kept here but in `~/.enx/auth.json`
//! (`crate::auth`), and the model picked in `/model` in `~/.enx/model.json`
//! (`crate::model_state`), the way opencode splits them. This file holds the
//! custom providers, a pinned starting model when the user wants one, and
//! everything else.

mod legacy;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

use crate::provider::registry::{ModelFacts, ModelRef};

/// Where the agent keeps configuration, sessions, and logs.
pub fn home_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("ENX_HOME") {
        return PathBuf::from(dir);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".enx")
}

pub fn config_path() -> PathBuf {
    home_dir().join("config.toml")
}

pub fn sessions_dir() -> PathBuf {
    home_dir().join("sessions")
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub model: ModelConfig,
    /// Custom providers, and changes to built-in ones, by id:
    /// `[provider.<id>]`. A built-in provider needs no entry to be used.
    pub provider: BTreeMap<String, ProviderEntry>,
    pub agent: AgentConfig,
    pub ui: UiConfig,
    pub typesafe: TypeSafeConfig,
    /// Providers for this process only, never written: an endpoint from
    /// `ENX_BASE_URL`, or a test's fixture server.
    #[serde(skip)]
    pub session_providers: BTreeMap<String, ProviderEntry>,
    /// Provider keys: `auth.json`, the environment, this process.
    #[serde(skip)]
    pub auth: crate::auth::Auth,
}

/// TypeSafe's System One model, used for small typed judgements inside the
/// harness — not for talking to the user.
///
/// Deliberately not a `provider`: nothing here can answer a prompt, and
/// putting it in the provider list would offer it as a chat model. With no
/// key set every feature below stays off and the existing behaviour is what
/// runs, so this is additive by construction.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TypeSafeConfig {
    /// Empty means every TypeSafe-backed feature is off.
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    /// Judge whether a tool result is worth keeping in context rather than
    /// deciding from the tool's name alone.
    pub gate_tool_results: bool,
    /// Score turns by how live they still are, so compaction drops what is
    /// finished rather than what is merely old.
    pub rank_compaction: bool,
    /// Milliseconds to wait before giving up and using the existing rules.
    /// A judgement that arrives late is worse than no judgement: it stalls
    /// the turn the user is waiting on.
    pub timeout_ms: u64,
}

impl Default for TypeSafeConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.typesafe.ai/v1/systemone".into(),
            model: "jev-latest".into(),
            gate_tool_results: true,
            rank_compaction: true,
            timeout_ms: 1500,
        }
    }
}

impl TypeSafeConfig {
    /// Nothing runs without a key, whatever the toggles say.
    pub fn active(&self) -> bool {
        !self.api_key.trim().is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub theme: String,
    pub show_sidebar: bool,
    /// Skills the user has toggled off via `/skills`.
    pub disabled_skills: Vec<String>,
    /// 3-letter ISO code shown next to prices. Defaults to `USD`.
    pub currency: String,
    /// Multiplier applied to USD prices before display (e.g. `15800.0` for
    /// IDR). `0` or `1.0` mean "no conversion, show as USD".
    pub currency_rate: f64,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "obsidian_ice".to_string(),
            show_sidebar: true,
            disabled_skills: Vec::new(),
            currency: "USD".to_string(),
            currency_rate: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelConfig {
    /// The model to start on, as `provider/model`. Empty, the usual case:
    /// the one picked last in `/model`, kept in `model.json`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub default: String,
    /// Sampling temperature. `None` keeps the provider default.
    pub temperature: Option<f32>,
    /// The model in use in this process, as `provider/model`: resolved at
    /// load, changed by `/model`. Never written.
    #[serde(skip)]
    pub active: String,
    /// Context window in tokens, used for the fill gauge in the UI. This
    /// and the fields below describe `active`, and are set with it.
    #[serde(skip)]
    pub context_window: u32,
    /// USD per 1M input tokens. 0 means unknown; the sidebar shows the
    /// running cost as `$0.00` when unknown rather than hiding it.
    #[serde(skip)]
    pub price_input: f64,
    /// USD per 1M output tokens.
    #[serde(skip)]
    pub price_output: f64,
    /// USD per 1M cached-read tokens (prompt cache).
    #[serde(skip)]
    pub price_cache_read: f64,
    /// True when the model accepts image inputs; drives whether `/attach`
    /// warns the user.
    #[serde(skip)]
    pub vision: bool,
    /// True when the model can call tools.
    #[serde(skip)]
    pub tool_call: bool,
    /// True when the model produces a reasoning trace.
    #[serde(skip)]
    pub reasoning: bool,
}

impl Default for ModelConfig {
    fn default() -> Self {
        let facts = ModelFacts::default();
        let mut model = Self {
            default: String::new(),
            temperature: None,
            active: String::new(),
            context_window: 0,
            price_input: 0.0,
            price_output: 0.0,
            price_cache_read: 0.0,
            vision: false,
            tool_call: true,
            reasoning: false,
        };
        model.set_facts(facts);
        model
    }
}

impl ModelConfig {
    pub fn set_facts(&mut self, facts: ModelFacts) {
        self.context_window = facts.context_window;
        self.price_input = facts.price_input;
        self.price_output = facts.price_output;
        self.price_cache_read = facts.price_cache_read;
        self.vision = facts.vision;
        self.tool_call = facts.tool_call;
        self.reasoning = facts.reasoning;
    }
}

/// A custom provider, or a change to a built-in one: `[provider.<id>]`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderEntry {
    /// Shown in the interface; the id when empty.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// OpenAI-compatible base URL, without `/chat/completions`. Empty for a
    /// built-in provider keeps its own.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub base_url: String,
    /// Where the model list is read, e.g. `https://host/v1/models`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub models_url: String,
    /// Models added by hand, or given figures of their own, by model id.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub models: BTreeMap<String, ModelEntry>,
}

/// What the user states about one model: `[provider.<id>.models."<model>"]`.
/// Anything left out comes from the provider's list or the catalogue.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_input: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_output: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_cache_read: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    /// Cap on model calls in a single turn; 0 means none, and the turn runs
    /// until the work is done. A cap cut long work off mid-task ("Stopped
    /// after 32 model calls"), so what stops a runaway turn by default is the
    /// loop guard in the agent: the same calls three steps running.
    pub max_steps: u32,
    /// Working directory the file and shell tools are rooted in.
    pub workspace: Option<PathBuf>,
    /// Seconds a single shell command may run before it is killed.
    pub shell_timeout_secs: u64,
    /// Trigger auto-compact when this fraction of the context window is used
    /// (0.0 disables). Default 0.85: at 85% the agent summarises older turns
    /// before the next user request runs, so a long session never crashes on
    /// a full context.
    pub auto_compact_at: f32,
    /// Whether the auto-compact trigger fires. Users can still run `/compact`
    /// manually with this off.
    pub auto_compact: bool,
    /// Number of trailing turns to keep verbatim during compact. Older turns
    /// get folded into a single summary assistant turn.
    pub compact_keep_last: usize,
    /// Whether the router may change agent on its own. With this off the UI
    /// asks first, so a user who does not want their specialist swapped
    /// mid-conversation keeps the decision.
    pub auto_switch: bool,
    /// Model id per tier. An unmapped tier falls back to the active model
    /// rather than failing: a missing table must not stop delegation.
    pub tiers: TierModels,
    /// Model id per agent name, overriding the agent's tier. This is the
    /// escape hatch for "everything is fine except `fe`".
    pub models: BTreeMap<String, String>,
    /// Whether written files go to the project's language server, so type
    /// errors and lint warnings come back with the edit.
    pub lsp: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_steps: 0,
            workspace: None,
            shell_timeout_secs: 120,
            auto_compact_at: 0.85,
            auto_compact: true,
            compact_keep_last: 4,
            auto_switch: true,
            tiers: TierModels::default(),
            models: BTreeMap::new(),
            lsp: true,
        }
    }
}

/// The `[agent.tiers]` table: one model id per tier.
///
/// Named fields rather than a map because the tiers are a closed set — a
/// typo'd key in a map would be silently ignored, and `enx config set
/// agent.tiers.strong` has to resolve to something.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TierModels {
    pub cheap: String,
    pub balanced: String,
    pub strong: String,
}

impl TierModels {
    /// The configured id for `tier`, or `None` when the slot is unset.
    pub fn get(&self, tier: crate::agent_def::Tier) -> Option<&str> {
        use crate::agent_def::Tier;
        let id = match tier {
            Tier::Cheap => &self.cheap,
            Tier::Balanced => &self.balanced,
            Tier::Strong => &self.strong,
        };
        non_empty(id)
    }
}

/// Treat blank as unset: TOML has no way to say "no value" for a string that
/// was written out by `save`, so every level of the lookup has to skip it.
fn non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

/// Window used when neither the provider nor the catalogue states one.
pub const DEFAULT_CONTEXT_WINDOW: u32 = 128_000;

/// The id of the provider `ENX_BASE_URL` describes.
pub const ENV_PROVIDER: &str = "env";

/// Whether `id` can name a provider: lowercase letters, digits, `-` and
/// `_`, starting with a letter or digit, as opencode requires. `env` is
/// taken by the endpoint from `ENX_BASE_URL`.
pub fn valid_provider_id(id: &str) -> bool {
    let mut chars = id.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        && id != ENV_PROVIDER
}

/// A provider id made from a name: `My Gateway` becomes `my-gateway`.
pub fn provider_id_from(name: &str) -> String {
    let mut id = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            id.push(c.to_ascii_lowercase());
        } else if !id.is_empty() && !id.ends_with('-') {
            id.push('-');
        }
    }
    id.trim_end_matches('-').to_owned()
}

impl Config {
    /// Load `~/.enx/config.toml` (defaults when it is absent), the keys in
    /// `auth.json` and the environment, then settle the model in use.
    ///
    /// A configuration written before providers had ids is moved to this
    /// layout the first time it is loaded, with a copy of the old file kept
    /// beside it.
    pub fn load() -> Result<Self> {
        let path = config_path();
        let mut table = if path.exists() {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            toml::from_str::<toml::Table>(&text)
                .with_context(|| format!("parsing {}", path.display()))?
        } else {
            toml::Table::new()
        };
        let moved = legacy::take(&mut table);
        let mut config: Config = toml::Value::Table(table)
            .try_into()
            .with_context(|| format!("parsing {}", path.display()))?;
        config.auth = crate::auth::Auth::load()?;
        if let Some(moved) = moved {
            moved.finish(&mut config, &path)?;
        }
        let state = crate::model_state::ModelState::load();
        config.apply_env(&state);
        config.resolve_active(&state);
        config.validate()?;
        Ok(config)
    }

    /// The model an agent should run on: per-agent override, then its tier,
    /// then the model in use.
    ///
    /// Falling back to the model in use rather than erroring is deliberate:
    /// a user who never wrote a tier table still gets working delegation.
    pub fn model_for(&self, agent: &str, tier: crate::agent_def::Tier) -> String {
        let agent = crate::agent_def::canonical_name(agent);
        self.agent
            .models
            .get(agent)
            // An override still keyed by an agent's old name applies to it.
            .or_else(|| {
                self.agent
                    .models
                    .iter()
                    .find(|(name, _)| crate::agent_def::canonical_name(name) == agent)
                    .map(|(_, id)| id)
            })
            .and_then(|id| non_empty(id))
            .or_else(|| self.agent.tiers.get(tier))
            .unwrap_or_else(|| self.model.active.trim())
            .to_owned()
    }

    /// `raw` when it is a `provider/model` ref to a provider enx knows.
    fn known_ref(&self, raw: &str) -> Option<ModelRef> {
        ModelRef::parse(raw).filter(|model| self.connection(&model.provider).is_some())
    }

    /// The models to start on, in order: `ENX_MODEL`, the pinned
    /// `model.default`, then the recent picks.
    fn start_candidates(&self, state: &crate::model_state::ModelState) -> Vec<String> {
        std::env::var("ENX_MODEL")
            .ok()
            .into_iter()
            .chain(std::iter::once(self.model.default.clone()))
            .map(|raw| raw.trim().to_owned())
            .filter(|raw| !raw.is_empty())
            .chain(state.recent.iter().cloned())
            .collect()
    }

    /// Settle the model in use again, as at start: after the provider it
    /// was on is disconnected, or when a provider is connected with no model
    /// in use yet.
    pub fn settle_model(&mut self) {
        let state = crate::model_state::ModelState::load();
        self.resolve_active(&state);
    }

    /// Settle the model in use: the first candidate whose provider is
    /// connected. A bare id in `ENX_MODEL` or `model.default` belongs to the
    /// endpoint from `ENX_BASE_URL`, or else to the provider of the first
    /// full ref; the recent list only ever holds full refs.
    fn resolve_active(&mut self, state: &crate::model_state::ModelState) {
        let candidates = self.start_candidates(state);
        let explicit = candidates.len() - state.recent.len();
        let home = if self.session_providers.contains_key(ENV_PROVIDER) {
            Some(ENV_PROVIDER.to_owned())
        } else {
            candidates
                .iter()
                .find_map(|raw| self.known_ref(raw))
                .map(|model| model.provider)
        };
        for (index, raw) in candidates.iter().enumerate() {
            let model = match self.known_ref(raw) {
                Some(model) => model,
                None if index < explicit => match &home {
                    Some(provider) => ModelRef::new(provider, raw),
                    None => continue,
                },
                None => continue,
            };
            if self
                .connection(&model.provider)
                .is_some_and(|connection| connection.is_connected())
                && self.use_model(&model.to_string())
            {
                return;
            }
        }
        self.model.active.clear();
        self.model.set_facts(ModelFacts::default());
    }

    fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.agent.max_steps <= 10_000,
            "agent.max_steps must be 0 (no limit) to 10000"
        );
        anyhow::ensure!(
            self.agent.shell_timeout_secs > 0 && self.agent.shell_timeout_secs <= 86_400,
            "agent.shell_timeout_secs must be 1..=86400"
        );
        for (id, entry) in &self.provider {
            anyhow::ensure!(
                valid_provider_id(id),
                "provider id `{id}` must be lowercase letters, digits, `-` or `_`"
            );
            if !entry.base_url.trim().is_empty() {
                validate_http_url(&entry.base_url, &format!("provider.{id}.base_url"))?;
            }
            if !entry.models_url.trim().is_empty() {
                validate_http_url(&entry.models_url, &format!("provider.{id}.models_url"))?;
            }
        }
        Ok(())
    }

    fn apply_env(&mut self, state: &crate::model_state::ModelState) {
        use crate::auth::KeySource;
        let var = |name: &str| {
            std::env::var(name)
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        };
        if let Some(base_url) = var("ENX_BASE_URL") {
            let name = reqwest::Url::parse(&base_url)
                .ok()
                .and_then(|url| url.host_str().map(str::to_owned))
                .unwrap_or_else(|| ENV_PROVIDER.to_owned());
            self.session_providers.insert(
                ENV_PROVIDER.to_owned(),
                ProviderEntry {
                    name,
                    base_url,
                    ..ProviderEntry::default()
                },
            );
            if let Some(key) = var("ENX_API_KEY") {
                self.auth
                    .set_for_session(ENV_PROVIDER, &key, KeySource::Env("ENX_API_KEY".into()));
            }
        } else if let Some(key) = var("ENX_API_KEY") {
            // With no endpoint of its own, the key is for the provider the
            // start model is on, as when enx had a single provider.
            let provider = self
                .start_candidates(state)
                .iter()
                .find_map(|raw| self.known_ref(raw))
                .map(|model| model.provider);
            if let Some(provider) = provider {
                self.auth
                    .set_for_session(&provider, &key, KeySource::Env("ENX_API_KEY".into()));
            }
        }
        if let Some(theme) = var("ENX_THEME") {
            self.ui.theme = theme;
        }
        // `TYPESAFE_API_KEY` is the name TypeSafe's own SDKs read, so a key
        // already exported for another tool works here without being copied.
        if let Some(key) = var("TYPESAFE_API_KEY") {
            self.typesafe.api_key = key;
        }
    }

    pub fn save(&self) -> Result<PathBuf> {
        self.validate()?;
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let text = toml::to_string_pretty(self)?;
        atomic_write(&path, text.as_bytes())?;
        Ok(path)
    }

    /// The directory tools operate in: configured workspace, else the process cwd.
    pub fn workspace(&self) -> PathBuf {
        self.agent
            .workspace
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }

    /// Whether any provider can be called now.
    pub fn has_connected_provider(&self) -> bool {
        self.connections()
            .iter()
            .any(crate::provider::registry::Connection::is_connected)
    }

    /// Whether a model is in use and its provider can be called.
    pub fn is_ready(&self) -> bool {
        self.active_connection()
            .is_some_and(|connection| connection.is_connected())
    }

    /// Read a dotted key. Used by `enx config get`. `model.active` is the
    /// model in use, which is never written to the file.
    pub fn get(&self, key: &str) -> Option<String> {
        if key == "model.active" {
            return Some(self.model.active.clone());
        }
        // Never echo a key: `enx config get` output lands in shell history,
        // terminal scrollback, and pasted bug reports.
        let mut value = serde_json::to_value(self).ok()?;
        value["typesafe"]["api_key"] = serde_json::Value::String(
            if self.typesafe.api_key.is_empty() {
                "(unset)"
            } else {
                "(redacted)"
            }
            .into(),
        );
        let mut cursor = &value;
        for part in key.split('.') {
            cursor = cursor.get(part)?;
        }
        Some(match cursor {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        })
    }

    /// Write a dotted key. Numbers and booleans are parsed so `agent.shell_timeout_secs=90`
    /// stays an integer in the file.
    pub fn set(&mut self, key: &str, raw: &str) -> Result<()> {
        if key == "model.default" {
            return self.pin_model(raw);
        }
        if let Some(rest) = key.strip_prefix("provider.") {
            return self.set_provider(rest, raw.trim());
        }
        let mut value = serde_json::to_value(&*self)?;
        let parts: Vec<&str> = key.split('.').collect();
        let mut cursor = &mut value;
        for part in &parts[..parts.len() - 1] {
            cursor = cursor
                .get_mut(*part)
                .ok_or_else(|| anyhow::anyhow!("unknown config section: {part}"))?;
        }
        let last = parts[parts.len() - 1];
        let slot = cursor
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("{key} is not a settable field"))?;
        // `agent.models` is keyed by agent name, so its keys are new ones:
        // `agent.models.fe` sets fe's model, and an empty value clears it.
        let agent_model = parts.len() == 3 && parts[..2] == ["agent", "models"];
        if agent_model && raw.trim().is_empty() {
            slot.remove(last);
        } else if !agent_model && !slot.contains_key(last) {
            anyhow::bail!("unknown config key: {key}");
        }
        let parsed = if agent_model || slot[last].is_string() || key == "agent.workspace" {
            serde_json::Value::String(raw.to_string())
        } else if raw == "null" {
            serde_json::Value::Null
        } else {
            parse_scalar(raw)
        };
        if !(agent_model && raw.trim().is_empty()) {
            slot.insert(last.to_string(), parsed);
        }
        let mut next: Self =
            serde_json::from_value(value).with_context(|| format!("invalid value for {key}"))?;
        next.validate()?;
        // What the file does not hold comes across as it was: the model in
        // use and its figures, the keys, this process's providers.
        next.model = ModelConfig {
            default: std::mem::take(&mut next.model.default),
            temperature: next.model.temperature,
            ..self.model.clone()
        };
        next.session_providers = std::mem::take(&mut self.session_providers);
        next.auth = std::mem::take(&mut self.auth);
        *self = next;
        Ok(())
    }

    /// Pin the model to start on, or unpin it with an empty value.
    fn pin_model(&mut self, raw: &str) -> Result<()> {
        let raw = raw.trim();
        if raw.is_empty() {
            self.model.default.clear();
            return Ok(());
        }
        let model = self.parse_model(raw).ok_or_else(|| {
            anyhow::anyhow!("`{raw}` names no provider enx knows; write it as provider/model")
        })?;
        self.model.default = model.to_string();
        self.use_model(&model.to_string());
        Ok(())
    }

    /// `provider.<id>.<field>`: name, base_url, models_url, or
    /// `models.<model>.<figure>`. An empty value clears the field, and an
    /// entry left with nothing in it is removed.
    fn set_provider(&mut self, rest: &str, raw: &str) -> Result<()> {
        let (id, field) = rest
            .split_once('.')
            .ok_or_else(|| anyhow::anyhow!("write provider.<id>.<field>"))?;
        anyhow::ensure!(
            valid_provider_id(id),
            "provider id `{id}` must be lowercase letters, digits, `-` or `_`"
        );
        if field == "api_key" {
            anyhow::bail!("keys are kept in auth.json, not config.toml: run `enx auth login {id}`");
        }
        let mut entry = self.provider.get(id).cloned().unwrap_or_default();
        match field {
            "name" => entry.name = raw.to_owned(),
            "base_url" => entry.base_url = raw.trim_end_matches('/').to_owned(),
            "models_url" => entry.models_url = raw.to_owned(),
            other => {
                let rest = other
                    .strip_prefix("models.")
                    .ok_or_else(|| anyhow::anyhow!("unknown provider field: {other}"))?;
                let figure = rest.rsplit_once('.').filter(|(_, figure)| {
                    matches!(
                        *figure,
                        "context_window" | "price_input" | "price_output" | "price_cache_read"
                    )
                });
                match figure {
                    Some((model, figure)) => {
                        let slot = entry.models.entry(model.to_owned()).or_default();
                        let number = || -> Result<Option<f64>> {
                            if raw.is_empty() {
                                return Ok(None);
                            }
                            raw.parse::<f64>()
                                .map(Some)
                                .map_err(|_| anyhow::anyhow!("{figure} must be a number"))
                        };
                        match figure {
                            "context_window" => {
                                slot.context_window = if raw.is_empty() {
                                    None
                                } else {
                                    Some(raw.parse().map_err(|_| {
                                        anyhow::anyhow!("context_window must be a whole number")
                                    })?)
                                }
                            }
                            "price_input" => slot.price_input = number()?,
                            "price_output" => slot.price_output = number()?,
                            _ => slot.price_cache_read = number()?,
                        }
                    }
                    // `provider.x.models.<model>` alone adds the model, or
                    // with an empty value removes it.
                    None if raw.is_empty() => {
                        entry.models.remove(rest);
                    }
                    None => {
                        entry.models.entry(rest.to_owned()).or_default();
                    }
                }
            }
        }
        let previous = if entry == ProviderEntry::default() {
            self.provider.remove(id)
        } else {
            self.provider.insert(id.to_owned(), entry)
        };
        // A value that does not validate leaves the entry as it was.
        if let Err(error) = self.validate() {
            match previous {
                Some(previous) => self.provider.insert(id.to_owned(), previous),
                None => self.provider.remove(id),
            };
            return Err(error);
        }
        // The model in use may be one whose figures just changed.
        let active = self.model.active.clone();
        if !active.is_empty() {
            self.use_model(&active);
        }
        Ok(())
    }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("path has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        if let Ok(metadata) = std::fs::metadata(path) {
            file.set_permissions(metadata.permissions())?;
        }
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result.with_context(|| format!("writing {}", path.display()))
}

fn parse_scalar(raw: &str) -> serde_json::Value {
    if let Ok(b) = raw.parse::<bool>() {
        return serde_json::Value::Bool(b);
    }
    if let Ok(n) = raw.parse::<u64>() {
        return serde_json::Value::from(n);
    }
    if let Ok(n) = raw.parse::<f64>() {
        return serde_json::Value::from(n);
    }
    serde_json::Value::String(raw.to_string())
}

fn validate_http_url(raw: &str, field: &str) -> Result<()> {
    let url = reqwest::Url::parse(raw).with_context(|| format!("invalid {field}"))?;
    anyhow::ensure!(
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none(),
        "{field} must be HTTP(S) without credentials"
    );
    Ok(())
}

/// Resolve a possibly-relative path against the workspace root, rejecting
/// escapes so a tool call cannot read outside the workspace.
pub fn resolve_in_workspace(root: &Path, candidate: &str) -> Result<PathBuf> {
    let joined = if Path::new(candidate).is_absolute() {
        PathBuf::from(candidate)
    } else {
        root.join(candidate)
    };
    let normalized = normalize(&joined);
    let root = normalize(root);
    if !normalized.starts_with(&root) {
        anyhow::bail!("path escapes the workspace: {candidate}");
    }
    Ok(normalized)
}

/// Lexical normalization: `..` and `.` are resolved without touching the disk,
/// so a path that does not exist yet (a file about to be written) still checks out.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests;
