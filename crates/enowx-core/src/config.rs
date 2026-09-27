//! On-disk configuration. One file, `~/.enx/config.toml`, plus environment
//! overrides so a container can run without writing anything.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

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
    pub provider: ProviderConfig,
    pub server: ServerConfig,
    pub agent: AgentConfig,
    pub ui: UiConfig,
    pub typesafe: TypeSafeConfig,
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
    /// Model id sent to the provider, e.g. `anthropic/claude-sonnet-4.5`.
    pub default: String,
    /// Sampling temperature. `None` keeps the provider default.
    pub temperature: Option<f32>,
    /// Context window in tokens, used for the fill gauge in the UI.
    pub context_window: u32,
    /// USD per 1M input tokens. 0 means unknown; sidebar shows the running
    /// cost as `$0.00` when unknown rather than hiding it.
    #[serde(default)]
    pub price_input: f64,
    /// USD per 1M output tokens.
    #[serde(default)]
    pub price_output: f64,
    /// USD per 1M cached-read tokens (prompt cache). Optional.
    #[serde(default)]
    pub price_cache_read: f64,
    /// True when the model accepts image inputs; drives whether `/attach`
    /// warns the user.
    #[serde(default)]
    pub vision: bool,
    /// True when the model can call tools.
    #[serde(default = "default_true")]
    pub tool_call: bool,
    /// True when the model produces a reasoning trace.
    #[serde(default)]
    pub reasoning: bool,
}

fn default_true() -> bool {
    true
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            default: String::new(),
            temperature: None,
            context_window: 128_000,
            price_input: 0.0,
            price_output: 0.0,
            price_cache_read: 0.0,
            vision: false,
            tool_call: true,
            reasoning: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderConfig {
    /// Display name of the active provider.
    pub name: String,
    /// Preset id this provider came from, or `custom` for a hand-entered endpoint.
    pub preset: String,
    /// OpenAI-compatible base URL, without the trailing `/chat/completions`.
    pub base_url: String,
    /// Exact endpoint used by model auto-detection, e.g. `https://host/v1/models`.
    pub models_url: String,
    /// API key. `ENX_API_KEY` overrides this.
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub port: u16,
    pub host: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 8787,
            host: "127.0.0.1".to_string(),
        }
    }
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

/// What the provider itself reported about a model. `None` means the provider
/// did not say, which is the common case for OpenAI-compatible gateways.
#[derive(Debug, Clone, Copy, Default)]
pub struct UpstreamModel {
    pub context_window: Option<u32>,
}

impl Config {
    /// Load `~/.enx/config.toml`, falling back to defaults when it is absent,
    /// then apply environment overrides.
    pub fn load() -> Result<Self> {
        let path = config_path();
        let mut config = if path.exists() {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?
        } else {
            Self::default()
        };
        config.apply_env();
        // Fill missing metadata (context window, pricing, capabilities) from
        // the cached models.dev catalog. Never overrides fields the user or
        // provider already set.
        config.fill_from_catalog();
        config.validate()?;
        Ok(config)
    }

    /// Look up the current model in the cached catalog and fill any field
    /// the user did not set. Fetching a fresh catalog is a separate async
    /// step wired by the runtime; here we only read what is on disk.
    pub fn fill_from_catalog(&mut self) {
        if self.model.default.trim().is_empty() {
            return;
        }
        let catalog = crate::catalog::Catalog::load_cached();
        let Some(entry) = self.model_facts(&catalog, &self.model.default) else {
            return;
        };
        if (self.model.context_window == 0 || self.model.context_window == 128_000)
            && entry.limit.context > 0
        {
            self.model.context_window = entry.limit.context;
        }
        if self.model.price_input == 0.0 && entry.cost.input > 0.0 {
            self.model.price_input = entry.cost.input;
        }
        if self.model.price_output == 0.0 && entry.cost.output > 0.0 {
            self.model.price_output = entry.cost.output;
        }
        if self.model.price_cache_read == 0.0 {
            if let Some(cr) = entry.cost.cache_read {
                self.model.price_cache_read = cr;
            }
        }
        if !self.model.vision && entry.modalities.supports_vision() {
            self.model.vision = true;
        }
        if !self.model.reasoning && entry.reasoning {
            self.model.reasoning = true;
        }
        // `tool_call` defaults to true; only set false when catalog says so
        // and user has not explicitly enabled it (we cannot distinguish, so
        // leave alone).
    }

    /// Adopt `model_id` and take its metadata from the catalog.
    ///
    /// Unlike `fill_from_catalog`, which only fills gaps, this REPLACES the
    /// per-model fields, because they describe the previous model and would
    /// otherwise be carried over silently: a 128k window on a model that
    /// takes 1M, or pricing 30x off, both of which then look like real
    /// readouts rather than leftovers.
    ///
    /// `upstream` is whatever the provider reported for this model, and wins
    /// where it is present — it describes how the model is actually being
    /// served, which the public catalogue cannot know. Fields the provider
    /// leaves out fall back to the catalogue, and fields neither knows are
    /// reset to a neutral default rather than kept from the old model.
    pub fn adopt_model(&mut self, model_id: &str, upstream: UpstreamModel) {
        self.model.default = model_id.trim().to_owned();
        let catalog = crate::catalog::Catalog::load_cached();
        let facts = self.model_facts(&catalog, &self.model.default);
        let entry = facts.as_ref();

        self.model.context_window = upstream
            .context_window
            .or_else(|| entry.map(|e| e.limit.context).filter(|c| *c > 0))
            .unwrap_or(DEFAULT_CONTEXT_WINDOW);

        let cost = entry.map(|e| &e.cost);
        self.model.price_input = cost.map(|c| c.input).unwrap_or(0.0);
        self.model.price_output = cost.map(|c| c.output).unwrap_or(0.0);
        self.model.price_cache_read = cost.and_then(|c| c.cache_read).unwrap_or(0.0);

        self.model.vision = entry.is_some_and(|e| e.modalities.supports_vision());
        self.model.reasoning = entry.is_some_and(|e| e.reasoning);
        // Tool calling stays on when the catalogue does not say otherwise:
        // an unlisted model is far more often capable than not, and turning
        // it off would silently disable every tool.
        self.model.tool_call = entry.map(|e| e.tool_call).unwrap_or(true);
    }

    /// What is known about `model_id` as the configured provider serves it:
    /// the provider's own published figures where enx carries them, then
    /// the catalogue, asking that provider's listing before anyone else's.
    fn model_facts(
        &self,
        catalog: &crate::catalog::Catalog,
        model_id: &str,
    ) -> Option<crate::catalog::CatalogModel> {
        let provider = self.catalog_provider();
        if let Some(official) = crate::catalog::official(provider, model_id) {
            return Some(official);
        }
        if provider.is_empty() {
            catalog.lookup(model_id).cloned()
        } else {
            catalog.lookup_for(provider, model_id).cloned()
        }
    }

    /// The catalogue's id for the configured provider, or "" when it has
    /// none (a custom endpoint, a gateway).
    fn catalog_provider(&self) -> &str {
        if crate::provider::is_deepseek(self) {
            return "deepseek";
        }
        match self.provider.preset.as_str() {
            preset @ ("openai" | "openrouter" | "groq") => preset,
            _ => "",
        }
    }

    /// The model an agent should run on: per-agent override, then its tier,
    /// then the active model.
    ///
    /// Falling back to the active model rather than erroring is deliberate —
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
            .unwrap_or_else(|| self.model.default.trim())
            .to_owned()
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
        anyhow::ensure!(
            self.model.context_window > 0,
            "model.context_window must be greater than zero"
        );
        if !self.provider.base_url.trim().is_empty() {
            validate_http_url(&self.provider.base_url, "provider.base_url")?;
        }
        if !self.provider.models_url.trim().is_empty() {
            validate_http_url(&self.provider.models_url, "provider.models_url")?;
        }
        Ok(())
    }

    fn apply_env(&mut self) {
        if let Ok(v) = std::env::var("ENX_API_KEY") {
            self.provider.api_key = v;
        }
        if let Ok(v) = std::env::var("ENX_BASE_URL") {
            self.provider.base_url = v;
            if self.provider.name.trim().is_empty() {
                self.provider.name = reqwest::Url::parse(&self.provider.base_url)
                    .ok()
                    .and_then(|url| url.host_str().map(str::to_owned))
                    .unwrap_or_default();
            }
        }
        if let Ok(v) = std::env::var("ENX_MODEL") {
            self.model.default = v;
        }
        if let Ok(v) = std::env::var("ENX_PORT") {
            if let Ok(port) = v.parse() {
                self.server.port = port;
            }
        }
        if let Ok(v) = std::env::var("ENX_THEME") {
            self.ui.theme = v;
        }
        // `TYPESAFE_API_KEY` is the name TypeSafe's own SDKs read, so a key
        // already exported for another tool works here without being copied.
        if let Ok(v) = std::env::var("TYPESAFE_API_KEY") {
            self.typesafe.api_key = v;
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

    /// Whether a provider has been explicitly configured and can own models.
    pub fn provider_active(&self) -> bool {
        !self.provider.name.trim().is_empty() && !self.provider.base_url.trim().is_empty()
    }

    /// Whether the active provider has everything required for a model call.
    pub fn is_ready(&self) -> bool {
        let local = reqwest::Url::parse(&self.provider.base_url)
            .ok()
            .is_some_and(|url| matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")));
        self.provider_active()
            && !self.model.default.trim().is_empty()
            && (local || !self.provider.api_key.is_empty())
    }

    /// Read a dotted key. Used by `enx config get`.
    pub fn get(&self, key: &str) -> Option<String> {
        // Never echo the key: `enx config get` output lands in shell history,
        // terminal scrollback, and pasted bug reports.
        let mut value = serde_json::to_value(self).ok()?;
        value["provider"]["api_key"] = serde_json::Value::String(
            if self.provider.api_key.is_empty() {
                "(unset)"
            } else {
                "(redacted)"
            }
            .into(),
        );
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

    /// Write a dotted key. Numbers and booleans are parsed so `server.port=9000`
    /// stays an integer in the file.
    pub fn set(&mut self, key: &str, raw: &str) -> Result<()> {
        // Changing the model changes everything scoped to it. Route through
        // `adopt_model` so `enx config set` lands the same window, pricing
        // and capabilities the interface would, instead of leaving the
        // previous model's numbers in place.
        if key == "model.default" {
            self.adopt_model(raw, UpstreamModel::default());
            return Ok(());
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
        if !slot.contains_key(last) {
            anyhow::bail!("unknown config key: {key}");
        }
        let parsed = if slot[last].is_string() || key == "agent.workspace" {
            serde_json::Value::String(raw.to_string())
        } else if raw == "null" {
            serde_json::Value::Null
        } else {
            parse_scalar(raw)
        };
        slot.insert(last.to_string(), parsed);
        let next: Self =
            serde_json::from_value(value).with_context(|| format!("invalid value for {key}"))?;
        next.validate()?;
        *self = next;
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
mod tests {
    use super::*;
    use crate::agent_def::Tier;

    #[test]
    fn set_keeps_scalar_types() {
        let mut config = Config::default();
        config.set("server.port", "9100").unwrap();
        assert_eq!(config.server.port, 9100);
        config.set("model.default", "zai/glm-4.6").unwrap();
        assert_eq!(config.model.default, "zai/glm-4.6");
        assert!(config.set("model.nope", "x").is_err());
        config.provider.api_key = "secret".into();
        assert_eq!(
            config.get("provider.api_key").as_deref(),
            Some("(redacted)")
        );
    }

    #[test]
    fn models_require_an_active_provider() {
        let mut config = Config::default();
        assert!(!config.provider_active());
        assert!(!config.is_ready());

        config.provider.name = "fixture".into();
        config.provider.base_url = "http://127.0.0.1:8913".into();
        assert!(config.provider_active());
        assert!(!config.is_ready());

        config.model.default = "fixture/model".into();
        assert!(config.is_ready());
    }

    /// Set every level at once: a resolution order that reads the wrong slot
    /// first still returns *a* model, so only a populated ladder catches it.
    #[test]
    fn the_most_specific_model_wins() {
        let mut config = Config::default();
        config.model.default = "active/model".into();
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
        config.model.default = "active/model".into();
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

    /// Configs written before the tier table existed have to keep loading.
    #[test]
    fn a_config_without_the_agent_tables_parses() {
        let config: Config = toml::from_str(
            r#"
[model]
default = "active/model"

[agent]
max_steps = 8
"#,
        )
        .expect("an older config still loads");
        assert_eq!(config.agent.max_steps, 8);
        assert!(config.agent.models.is_empty());
        assert_eq!(config.model_for("fe", Tier::Strong), "active/model");
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
}

#[cfg(test)]
mod adopt_model_tests {
    use super::*;

    /// A gateway that reports nothing but an id is the common case; the
    /// catalogue has to supply the rest or the UI shows placeholder numbers
    /// as if they were real.
    #[test]
    fn upstream_silence_falls_back_to_the_catalogue() {
        let catalog = crate::catalog::Catalog::load_cached();
        // Skip when no catalogue is cached (offline CI); the fallback rules
        // are covered by the tests below that do not need one.
        let Some((id, entry)) = catalog
            .providers
            .values()
            .flat_map(|p| p.models.iter())
            .find(|(_, m)| m.limit.context > 0 && m.cost.input > 0.0)
            .map(|(id, m)| (id.clone(), m.clone()))
        else {
            return;
        };
        let mut config = Config::default();
        config.adopt_model(&id, UpstreamModel::default());
        assert_eq!(
            config.model.context_window, entry.limit.context,
            "the window should come from the catalogue"
        );
        assert_eq!(config.model.price_input, entry.cost.input);
        assert_eq!(config.model.reasoning, entry.reasoning);
    }

    /// The provider knows how it is actually serving the model; a public
    /// catalogue cannot. Where they disagree, the provider wins.
    #[test]
    fn upstream_wins_over_the_catalogue() {
        let mut config = Config::default();
        config.adopt_model(
            "gpt-4o",
            UpstreamModel {
                context_window: Some(42_000),
            },
        );
        assert_eq!(config.model.context_window, 42_000);
    }

    /// Switching models must not carry the old one's numbers across — that is
    /// how a 128k window ended up displayed for a 1M-context model.
    #[test]
    fn switching_models_replaces_rather_than_keeps() {
        let mut config = Config::default();
        config.model.context_window = 999_999;
        config.model.price_input = 12.5;
        config.model.price_output = 99.0;
        config.model.reasoning = true;
        config.model.vision = true;

        config.adopt_model("definitely-not-a-real-model-xyz", UpstreamModel::default());

        assert_eq!(
            config.model.context_window, DEFAULT_CONTEXT_WINDOW,
            "an unknown model falls back to the default window, not the previous model's"
        );
        assert_eq!(
            config.model.price_input, 0.0,
            "stale pricing must be dropped"
        );
        assert_eq!(config.model.price_output, 0.0);
        assert!(
            !config.model.reasoning,
            "capabilities describe the old model"
        );
        assert!(!config.model.vision);
    }

    /// An unlisted model is far more often tool-capable than not, and turning
    /// tools off silently would disable the agent's whole toolset.
    #[test]
    fn an_unknown_model_keeps_tool_calling_enabled() {
        let mut config = Config::default();
        config.adopt_model("definitely-not-a-real-model-xyz", UpstreamModel::default());
        assert!(config.model.tool_call);
    }

    #[test]
    fn the_model_id_is_trimmed() {
        let mut config = Config::default();
        config.adopt_model("  spaced-model  ", UpstreamModel::default());
        assert_eq!(config.model.default, "spaced-model");
    }

    /// Choosing a model on DeepSeek's own API takes DeepSeek's published
    /// figures, whatever the public catalogue says.
    #[test]
    fn a_deepseek_model_takes_deepseeks_own_figures() {
        let mut config = Config::default();
        config.provider.preset = "deepseek".into();
        config.provider.base_url = "https://api.deepseek.com".into();
        config.adopt_model("deepseek-v4-pro", UpstreamModel::default());
        assert_eq!(config.model.price_input, 0.66);
        assert_eq!(config.model.price_output, 1.98);
        assert_eq!(config.model.price_cache_read, 0.022);
        assert_eq!(config.model.context_window, 1_000_000);
        assert!(!config.model.vision);
        assert!(config.model.reasoning && config.model.tool_call);

        config.adopt_model("deepseek-flash", UpstreamModel::default());
        assert_eq!(config.model.price_input, 0.15);
        assert!(config.model.vision, "Flash takes images");
    }
}
