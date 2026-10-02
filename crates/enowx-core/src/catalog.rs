//! Model catalog fetched from https://models.dev/api.json and cached locally.
//!
//! The catalog is *fill-in-only*: it never overrides upstream provider data.
//! Fields the provider already reports (context window, pricing, capabilities)
//! stay as-is; we only fill zeros/`None`/missing entries.
//!
//! Fetch is best-effort. Missing network, malformed JSON, or an expired cache
//! all fall back to whatever is on disk (or nothing). Never blocks startup.

use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::config::home_dir;

const CATALOG_URL: &str = "https://models.dev/api.json";
const CACHE_TTL_SECS: u64 = 24 * 60 * 60;

pub fn cache_path() -> PathBuf {
    home_dir().join("models.json")
}

/// One model entry as returned by models.dev. Only fields we consume are
/// deserialized; anything else is ignored so a schema addition upstream never
/// breaks parsing.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CatalogModel {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub reasoning: bool,
    #[serde(default)]
    pub tool_call: bool,
    #[serde(default)]
    pub attachment: bool,
    #[serde(default)]
    pub limit: CatalogLimit,
    #[serde(default)]
    pub cost: CatalogCost,
    #[serde(default)]
    pub modalities: CatalogModalities,
    /// How the model's thinking can be set: effort levels, a token budget,
    /// or on and off.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasoning_options: Vec<ReasoningOption>,
}

/// One way a model's thinking can be set, as models.dev lists it:
/// `{"type":"effort","values":["low","high"]}`, `{"type":"budget_tokens",
/// "min":1024}` or `{"type":"toggle"}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReasoningOption {
    #[serde(rename = "type", default)]
    pub kind: String,
    /// Effort levels. Entries that are not text (models.dev lists a `null`
    /// for "off" on some) are left out.
    #[serde(
        default,
        deserialize_with = "text_only",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub values: Vec<String>,
    /// Budget bounds; any number, since some entries say -1 for "none".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
}

fn text_only<'de, D: serde::Deserializer<'de>>(de: D) -> Result<Vec<String>, D::Error> {
    let values: Vec<serde_json::Value> = Deserialize::deserialize(de).unwrap_or_default();
    Ok(values
        .into_iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect())
}

impl CatalogModel {
    /// The thinking efforts the user can choose for this model, weakest
    /// first: the effort levels it lists; `low`, `medium` and `high` for one
    /// set by a token budget, which OpenAI-compatible gateways map to one;
    /// and `none` first when thinking can be turned off. Empty for a model
    /// that does not think or says nothing about how.
    pub fn efforts(&self) -> Vec<String> {
        if !self.reasoning {
            return Vec::new();
        }
        let has = |kind: &str| self.reasoning_options.iter().any(|o| o.kind == kind);
        let mut levels: Vec<String> = self
            .reasoning_options
            .iter()
            .find(|o| o.kind == "effort")
            .map(|o| o.values.clone())
            .unwrap_or_default();
        if levels.is_empty() && has("budget_tokens") {
            levels = ["low", "medium", "high"].map(String::from).to_vec();
        }
        if has("toggle") && !levels.iter().any(|l| l == "none") {
            levels.insert(0, "none".to_owned());
        }
        levels
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CatalogLimit {
    #[serde(default)]
    pub context: u32,
    #[serde(default)]
    pub output: u32,
}

/// Prices are USD per 1M tokens. `cache_read`/`cache_write` are optional
/// because most providers do not offer prompt caching.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CatalogCost {
    #[serde(default)]
    pub input: f64,
    #[serde(default)]
    pub output: f64,
    #[serde(default)]
    pub cache_read: Option<f64>,
    #[serde(default)]
    pub cache_write: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CatalogModalities {
    #[serde(default)]
    pub input: Vec<String>,
    #[serde(default)]
    pub output: Vec<String>,
}

impl CatalogModalities {
    pub fn supports_vision(&self) -> bool {
        self.input.iter().any(|m| m == "image")
    }
}

/// Full catalog: `provider_id -> { model_id -> CatalogModel }`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Catalog {
    #[serde(flatten)]
    pub providers: BTreeMap<String, CatalogProvider>,
    /// Unix seconds the cache was written. Used to enforce TTL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fetched_at: Option<u64>,
    /// Which fields the cache was written with. A cache from before a field
    /// was read lacks it, so it counts as stale and is fetched again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<u32>,
    /// Every listed model under each of its [`names`], built on first use.
    #[serde(skip)]
    index: std::sync::OnceLock<HashMap<String, Vec<Listed>>>,
}

/// A catalogue entry reachable under a name: who lists it, its id there, and
/// how many decorations came off its id to reach the name (0: the id itself).
#[derive(Debug, Clone)]
struct Listed {
    rank: usize,
    provider: String,
    id: String,
}

/// Bumped whenever `CatalogModel` reads a new field from models.dev.
pub const CATALOG_SCHEMA: u32 = 2;

/// How many times this process has fetched the catalogue, so a host can
/// look its model's facts up again once a fresh one is in.
pub static REFRESHED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CatalogProvider {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub models: BTreeMap<String, CatalogModel>,
}

impl Catalog {
    /// Read a catalogue, keeping every model that parses: one entry in a shape
    /// enx does not expect (models.dev adds and changes fields) leaves that
    /// model out rather than the whole catalogue, which would put every model
    /// on the default window.
    pub fn parse(text: &str) -> Self {
        let Ok(serde_json::Value::Object(root)) = serde_json::from_str::<serde_json::Value>(text)
        else {
            return Catalog::default();
        };
        let mut catalog = Catalog::default();
        for (key, value) in root {
            match key.as_str() {
                "fetched_at" => catalog.fetched_at = value.as_u64(),
                "schema" => catalog.schema = value.as_u64().map(|v| v as u32),
                _ => {
                    let Some(object) = value.as_object() else {
                        continue;
                    };
                    let text_of = |k: &str| {
                        object
                            .get(k)
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_owned()
                    };
                    let mut provider = CatalogProvider {
                        id: text_of("id"),
                        name: text_of("name"),
                        models: BTreeMap::new(),
                    };
                    if let Some(models) = object.get("models").and_then(|m| m.as_object()) {
                        for (id, model) in models {
                            if let Ok(model) = serde_json::from_value::<CatalogModel>(model.clone())
                            {
                                provider.models.insert(id.clone(), model);
                            }
                        }
                    }
                    catalog.providers.insert(key, provider);
                }
            }
        }
        catalog
    }

    /// The cached catalogue. One past its time, or written by an older enx,
    /// is still used until a fresh one arrives: old limits and prices are
    /// far closer than none, which would put every model on the default
    /// window.
    pub fn load_cached() -> Self {
        let path = cache_path();
        let Ok(text) = fs::read_to_string(&path) else {
            return Catalog::default();
        };
        Self::parse(&text)
    }

    /// Whether the cached catalogue should be fetched again.
    pub fn needs_refresh(&self) -> bool {
        self.is_stale()
    }

    /// The cached catalogue, parsed once per process and again only when
    /// the file changes. It is a few megabytes of JSON, and a model's facts
    /// are looked up on every switch of model, agent or delegation.
    pub fn shared() -> std::sync::Arc<Catalog> {
        use std::sync::{Arc, Mutex, OnceLock};
        type Cached = Option<(PathBuf, SystemTime, Arc<Catalog>)>;
        static CACHE: OnceLock<Mutex<Cached>> = OnceLock::new();
        let path = cache_path();
        let Ok(modified) = fs::metadata(&path).and_then(|meta| meta.modified()) else {
            return Arc::new(Catalog::default());
        };
        let mut cache = CACHE
            .get_or_init(|| Mutex::new(None))
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some((at_path, at, catalog)) = cache.as_ref() {
            if *at_path == path && *at == modified {
                return catalog.clone();
            }
        }
        let catalog = Arc::new(Self::load_cached());
        *cache = Some((path, modified, catalog.clone()));
        catalog
    }

    fn is_stale(&self) -> bool {
        if self.schema != Some(CATALOG_SCHEMA) {
            return true;
        }
        let Some(ts) = self.fetched_at else {
            return true;
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        now.saturating_sub(ts) > CACHE_TTL_SECS
    }

    /// Fetch the catalog from models.dev and write it to the cache. Best
    /// effort; failure returns the currently cached (possibly empty) catalog.
    pub async fn refresh() -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        let text = client.get(CATALOG_URL).send().await?.text().await?;
        let mut cat = Catalog::parse(&text);
        anyhow::ensure!(
            !cat.providers.is_empty(),
            "models.dev sent nothing enx could read"
        );
        cat.schema = Some(CATALOG_SCHEMA);
        cat.fetched_at = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        );
        let path = cache_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let json = serde_json::to_string(&cat)?;
        let _ = fs::write(&path, json);
        REFRESHED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(cat)
    }

    /// The entry for `model_id` as `provider` serves it: that provider's own
    /// listing first, then the model's maker's, then anyone's.
    ///
    /// `lookup` alone takes the first provider in alphabetical order that
    /// lists the id, and many resellers list the same ids at their own
    /// prices: `deepseek-chat` resolved to helicone's before DeepSeek's.
    pub fn lookup_for(&self, provider: &str, model_id: &str) -> Option<&CatalogModel> {
        let provider = provider.trim();
        if let Some(model) = self
            .providers
            .get(provider)
            .and_then(|listing| listing.models.get(model_id))
        {
            return Some(model);
        }
        self.resolve(Some(provider), model_id)
            .or_else(|| self.fuzzy(model_id))
    }

    /// The entry for `model_id` from whoever lists it, the model's maker
    /// first. Returns `None` when no name matches closely enough.
    pub fn lookup(&self, model_id: &str) -> Option<&CatalogModel> {
        self.resolve(None, model_id)
            .or_else(|| self.fuzzy(model_id))
    }

    /// The listing for `model_id` by name, however the upstream spells it.
    /// Its [`names`] are tried most specific first, so `claude-opus-4.7-1m`
    /// is `claude-opus-4-7-1m` if anyone lists that, and `claude-opus-4-7`
    /// otherwise. A listing counts under its own names too, so the maker's
    /// `gemini-3.1-pro-preview` answers for `gemini-3.1-pro`.
    ///
    /// Among the listings under a name, `provider` (the one serving the
    /// model) wins, then the model's maker, whose figures are the reference
    /// a reseller's may differ from; each with its closest listing. Failing
    /// both, the closest listing that states a price, since a gateway listing
    /// a model at $0 says nothing of what it costs.
    fn resolve(&self, provider: Option<&str>, model_id: &str) -> Option<&CatalogModel> {
        let index = self.index();
        for name in names(model_id) {
            let Some(hits) = index.get(&name) else {
                continue;
            };
            let maker = maker_of(&name);
            let closest = |from: Option<&str>| {
                hits.iter()
                    .filter(|hit| from.is_none_or(|from| hit.provider == from))
                    .min_by_key(|hit| hit.rank)
            };
            let model = |hit: &Listed| self.providers.get(&hit.provider)?.models.get(&hit.id);
            let chosen = provider
                .and_then(|p| closest(Some(p)))
                .or_else(|| maker.and_then(|m| closest(Some(m))));
            if let Some(hit) = chosen {
                return model(hit);
            }
            let best_rank = hits.iter().map(|hit| hit.rank).min()?;
            let nearest = || hits.iter().filter(|hit| hit.rank == best_rank);
            return nearest()
                .filter_map(model)
                .find(|m| m.cost.input > 0.0)
                .or_else(|| nearest().find_map(model));
        }
        None
    }

    /// Every listed model under each of its names, in provider order.
    fn index(&self) -> &HashMap<String, Vec<Listed>> {
        self.index.get_or_init(|| {
            let mut index: HashMap<String, Vec<Listed>> = HashMap::new();
            for (prov, listing) in &self.providers {
                for id in listing.models.keys() {
                    for (rank, name) in names(id).into_iter().enumerate() {
                        index.entry(name).or_default().push(Listed {
                            rank,
                            provider: prov.clone(),
                            id: id.clone(),
                        });
                    }
                }
            }
            index
        })
    }

    /// The closest id by spelling, for a name [`names`] cannot reach. Only
    /// close matches with the same version numbers count: `claude-opus-9`
    /// is a model the catalogue does not list yet, not `claude-opus-4-7`, and
    /// is better on the defaults than on another model's figures.
    fn fuzzy(&self, model_id: &str) -> Option<&CatalogModel> {
        let mut best: Option<(&CatalogModel, usize)> = None;
        let needle = model_id.to_ascii_lowercase();
        let version = names(model_id)
            .last()
            .map(|name| numbers(name))
            .unwrap_or_default();
        let version_of = |id: &str| {
            names(id)
                .last()
                .map(|name| numbers(name))
                .unwrap_or_default()
        };
        for prov in self.providers.values() {
            for (id, m) in &prov.models {
                let hay = id.to_ascii_lowercase();
                if version_of(&hay) != version {
                    continue;
                }
                let score = fuzzy_score(&needle, &hay);
                if score > 0 && best.is_none_or(|(_, s)| score > s) {
                    best = Some((m, score));
                }
            }
        }
        best.filter(|(_, s)| *s >= 70).map(|(m, _)| m)
    }
}

/// A model id reduced to the name of the model: lowercase, without the
/// provider path (`anthropic/`) or a `:variant` (`:thinking`, `:free`), every
/// separator a `-`. `anthropic/claude-opus-4.7:thinking` and
/// `claude-opus-4-7` are both `claude-opus-4-7`.
pub fn canonical(id: &str) -> String {
    let id = id.trim().to_ascii_lowercase();
    let id = id.rsplit('/').next().unwrap_or(&id);
    let id = id.split(':').next().unwrap_or(id);
    let mut out = String::with_capacity(id.len());
    for c in id.chars() {
        let c = if matches!(c, '.' | '_' | ' ') { '-' } else { c };
        if c == '-' && (out.is_empty() || out.ends_with('-')) {
            continue;
        }
        out.push(c);
    }
    out.trim_end_matches('-').to_owned()
}

/// The canonical names `model_id` may be listed under, most specific first.
/// Upstreams decorate a model's name: a region or vendor in front
/// (`us.anthropic.`, `databricks-`), and a size, date, version or mode
/// behind (`-1m`, `-20251101`, `-v1`, `-thinking`, `-high`). Each name drops
/// one more decoration from the end, and each is also tried without up to
/// three leading words.
pub fn names(model_id: &str) -> Vec<String> {
    let canon = canonical(model_id);
    let mut tokens: Vec<&str> = canon.split('-').filter(|t| !t.is_empty()).collect();
    let mut out: Vec<String> = Vec::new();
    loop {
        for skip in 0..=3.min(tokens.len().saturating_sub(2)) {
            let rest = &tokens[skip..];
            if rest
                .first()
                .is_some_and(|t| t.starts_with(|c: char| c.is_ascii_alphabetic()))
            {
                let name = rest.join("-");
                if !out.contains(&name) {
                    out.push(name);
                }
            }
        }
        let n = tokens.len();
        if n >= 4 && is_date(&tokens[n - 3..]) {
            tokens.truncate(n - 3);
        } else if n >= 2 && is_decoration(tokens[n - 1]) {
            tokens.truncate(n - 1);
        } else {
            break;
        }
    }
    out
}

/// The numbers in a name, in order: a model's version.
fn numbers(name: &str) -> Vec<String> {
    name.split(|c: char| !c.is_ascii_digit())
        .filter(|run| !run.is_empty())
        .map(str::to_owned)
        .collect()
}

/// `2024-11-20` split into its three words.
fn is_date(words: &[&str]) -> bool {
    let digits = |w: &str, len: usize| w.len() == len && w.bytes().all(|b| b.is_ascii_digit());
    words.len() == 3
        && digits(words[0], 4)
        && words[0].starts_with("20")
        && digits(words[1], 2)
        && digits(words[2], 2)
}

/// A trailing word that dresses a model's name rather than naming another
/// model: a date stamp, a version, a context size, a thinking mode or level,
/// a release channel.
fn is_decoration(word: &str) -> bool {
    let digits = |w: &str| !w.is_empty() && w.bytes().all(|b| b.is_ascii_digit());
    if (word.len() == 8 || word.len() == 4) && digits(word) {
        return true;
    }
    if word.strip_prefix('v').is_some_and(digits) {
        return true;
    }
    if word.strip_suffix(['k', 'm']).is_some_and(digits) {
        return true;
    }
    matches!(
        word,
        "latest"
            | "thinking"
            | "think"
            | "reasoning"
            | "nothink"
            | "minimal"
            | "low"
            | "medium"
            | "high"
            | "xhigh"
            | "max"
            | "fast"
            | "preview"
            | "exp"
            | "experimental"
            | "beta"
            | "free"
    )
}

/// The provider whose catalogue entry is the reference for a model family:
/// the maker's own listing.
fn maker_of(name: &str) -> Option<&'static str> {
    let first = name.split('-').next().unwrap_or(name);
    let starts = |prefixes: &[&str]| prefixes.iter().any(|p| first.starts_with(p));
    let openai_o = first.len() >= 2
        && first.starts_with('o')
        && first[1..].starts_with(|c: char| c.is_ascii_digit());
    Some(if starts(&["claude"]) {
        "anthropic"
    } else if starts(&["gpt", "chatgpt", "codex"]) || openai_o {
        "openai"
    } else if starts(&["gemini", "gemma"]) {
        "google"
    } else if starts(&["deepseek"]) {
        "deepseek"
    } else if starts(&["grok"]) {
        "xai"
    } else if starts(&["kimi"]) {
        "moonshotai"
    } else if starts(&["glm"]) {
        "zhipuai"
    } else if starts(&["qwen", "qwq"]) {
        "alibaba"
    } else if starts(&[
        "mistral",
        "codestral",
        "devstral",
        "magistral",
        "ministral",
        "pixtral",
    ]) {
        "mistral"
    } else if starts(&["minimax"]) {
        "minimax"
    } else if starts(&["llama"]) {
        "meta"
    } else {
        return None;
    })
}

/// Limits and prices from a provider's own documentation, for models where
/// the public catalogue has drifted from it. Checked before the catalogue.
///
/// DeepSeek, from https://api-docs.deepseek.com/quick_start/pricing on
/// 2026-09-27: models.dev listed `deepseek-v4-pro` at $0.435/$0.87 per 1M
/// where DeepSeek charges $0.66/$1.98. These are DeepSeek's off-peak rates.
/// Peak hours (01:00–04:00 and 06:00–10:00 UTC on weekdays) cost double,
/// which a per-model price cannot express. `deepseek-v4-flash` and
/// `deepseek-v4-flash-vision-exp` are retired names DeepSeek still accepts
/// and bills as `deepseek-flash`.
pub fn official(provider: &str, model_id: &str) -> Option<CatalogModel> {
    if provider.trim() != "deepseek" {
        return None;
    }
    let (name, input, output, cache_read, vision) = match model_id.trim() {
        "deepseek-flash" | "deepseek-v4-flash" | "deepseek-v4-flash-vision-exp" => {
            ("DeepSeek-V4.1-Flash", 0.15, 0.6, 0.003, true)
        }
        "deepseek-v4-pro" => ("DeepSeek-V4-Pro", 0.66, 1.98, 0.022, false),
        _ => return None,
    };
    let mut input_modes = vec!["text".to_owned()];
    if vision {
        input_modes.push("image".to_owned());
    }
    Some(CatalogModel {
        id: model_id.trim().to_owned(),
        name: name.to_owned(),
        description: String::new(),
        reasoning: true,
        tool_call: true,
        attachment: vision,
        limit: CatalogLimit {
            context: 1_000_000,
            output: 393_216,
        },
        cost: CatalogCost {
            input,
            output,
            cache_read: Some(cache_read),
            cache_write: None,
        },
        modalities: CatalogModalities {
            input: input_modes,
            output: vec!["text".to_owned()],
        },
        // As models.dev lists DeepSeek's own V4 models.
        reasoning_options: vec![
            ReasoningOption {
                kind: "toggle".to_owned(),
                ..ReasoningOption::default()
            },
            ReasoningOption {
                kind: "effort".to_owned(),
                values: if vision {
                    &["low", "high", "max"][..]
                } else {
                    &["high", "max"][..]
                }
                .iter()
                .map(|v| (*v).to_owned())
                .collect(),
                ..ReasoningOption::default()
            },
        ],
    })
}

/// 0-100 score. 100 = exact, 90+ = one contains the other, drops with
/// character-level edit distance for shorter mismatches. Only names that share
/// a meaningful prefix ever pass the threshold.
fn fuzzy_score(needle: &str, hay: &str) -> usize {
    if needle == hay {
        return 100;
    }
    // Require a shared prefix of at least 4 chars so `gpt-4` never matches
    // `claude-4`. This is a coding agent - drift here is worse than a miss.
    let shared_prefix = needle
        .chars()
        .zip(hay.chars())
        .take_while(|(a, b)| a == b)
        .count();
    if shared_prefix < 4 {
        return 0;
    }
    if hay.contains(needle) || needle.contains(hay) {
        return 90;
    }
    let dist = levenshtein(needle, hay);
    let max_len = needle.len().max(hay.len());
    if max_len == 0 {
        return 0;
    }
    100 - (dist * 100 / max_len)
}

/// Classic Wagner-Fischer. Small strings (model ids ≤ ~50 chars), so the
/// quadratic table is negligible.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur: Vec<usize> = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_rejects_unrelated_families() {
        assert_eq!(fuzzy_score("gpt-4o", "claude-3-sonnet"), 0);
    }

    #[test]
    fn fuzzy_matches_close_variants() {
        assert!(fuzzy_score("claude-sonnet-4.5", "claude-sonnet-4-5") >= 70);
        assert!(fuzzy_score("gpt-4o-2024-11", "gpt-4o") >= 70);
    }

    fn thinking(options: serde_json::Value) -> CatalogModel {
        CatalogModel {
            reasoning: true,
            reasoning_options: serde_json::from_value(options).unwrap(),
            ..CatalogModel::default()
        }
    }

    #[test]
    fn efforts_follow_what_models_dev_lists() {
        let effort =
            thinking(serde_json::json!([{ "type": "effort", "values": ["low", "high", "max"] }]));
        assert_eq!(effort.efforts(), ["low", "high", "max"]);
        let budget = thinking(serde_json::json!([{ "type": "budget_tokens", "min": 1024 }]));
        assert_eq!(budget.efforts(), ["low", "medium", "high"]);
        let both = thinking(
            serde_json::json!([{ "type": "toggle" }, { "type": "effort", "values": ["low", "high"] }]),
        );
        assert_eq!(both.efforts(), ["none", "low", "high"]);
        let toggle = thinking(serde_json::json!([{ "type": "toggle" }]));
        assert_eq!(toggle.efforts(), ["none"]);
        let silent = CatalogModel {
            reasoning: false,
            ..effort.clone()
        };
        assert!(silent.efforts().is_empty(), "a model that does not think");
        assert_eq!(
            official("deepseek", "deepseek-v4-pro").unwrap().efforts(),
            ["none", "high", "max"]
        );
    }

    /// A model in a shape enx does not expect is left out, not the whole
    /// catalogue: that once put every model on the default window.
    #[test]
    fn one_odd_entry_does_not_empty_the_catalogue() {
        let text = serde_json::json!({
            "deepseek": { "id": "deepseek", "models": {
                "deepseek-v4.1-flash": { "id": "deepseek-v4.1-flash", "reasoning": true,
                    "limit": { "context": 1000000, "output": 384000 },
                    "reasoning_options": [{ "type": "effort", "values": [null, "low", "high"] },
                                          { "type": "budget_tokens", "min": -1, "max": 32768 }] },
                "broken": { "id": "broken", "limit": { "context": "a lot" } }
            }}
        })
        .to_string();
        let catalog = Catalog::parse(&text);
        let model = catalog.lookup("cbc/deepseek-v4.1-flash").unwrap();
        assert_eq!(model.limit.context, 1_000_000);
        assert_eq!(model.efforts(), ["low", "high"]);
        assert!(!catalog.providers["deepseek"].models.contains_key("broken"));
    }

    #[test]
    fn an_old_cache_without_the_new_fields_is_stale() {
        let old = Catalog {
            fetched_at: Some(u64::MAX / 2),
            ..Catalog::default()
        };
        assert!(old.is_stale());
    }

    fn priced(input: f64) -> CatalogModel {
        CatalogModel {
            cost: CatalogCost {
                input,
                ..CatalogCost::default()
            },
            ..CatalogModel::default()
        }
    }

    /// A reseller listing the same id sorts first alphabetically; the
    /// provider actually serving the model has to win.
    #[test]
    fn the_serving_providers_entry_wins_over_a_resellers() {
        let mut catalog = Catalog::default();
        for (provider, price) in [("aaa-reseller", 9.0), ("deepseek", 0.15)] {
            catalog.providers.insert(
                provider.into(),
                CatalogProvider {
                    models: [("deepseek-flash".to_owned(), priced(price))].into(),
                    ..CatalogProvider::default()
                },
            );
        }
        // Asked without a provider, the maker's own listing is the reference.
        assert_eq!(catalog.lookup("deepseek-flash").unwrap().cost.input, 0.15);
        assert_eq!(
            catalog
                .lookup_for("deepseek", "deepseek-flash")
                .unwrap()
                .cost
                .input,
            0.15
        );
        // Unknown to that provider: the maker's listing answers.
        assert_eq!(
            catalog
                .lookup_for("openai", "deepseek-flash")
                .unwrap()
                .cost
                .input,
            0.15
        );
    }

    /// A catalogue with `models` under each provider, each model's context
    /// set so a test can tell which entry was found.
    fn listing(entries: &[(&str, &str, u32)]) -> Catalog {
        let mut catalog = Catalog::default();
        for (provider, id, context) in entries {
            catalog
                .providers
                .entry((*provider).to_owned())
                .or_default()
                .models
                .insert(
                    (*id).to_owned(),
                    CatalogModel {
                        id: (*id).to_owned(),
                        limit: CatalogLimit {
                            context: *context,
                            output: 0,
                        },
                        ..CatalogModel::default()
                    },
                );
        }
        catalog
    }

    #[test]
    fn a_name_is_reduced_to_the_model_it_names() {
        for (sent, want) in [
            ("claude-opus-4.7", "claude-opus-4-7"),
            ("anthropic/claude-opus-4.7:thinking", "claude-opus-4-7"),
            ("Claude_Opus 4.7", "claude-opus-4-7"),
            ("gpt-5.1", "gpt-5-1"),
        ] {
            assert_eq!(canonical(sent), want, "{sent}");
        }
    }

    #[test]
    fn decorations_come_off_one_at_a_time() {
        let tried = names("us.anthropic.claude-opus-4.7-20251101-v1:0");
        let at = |name: &str| tried.iter().position(|n| n == name);
        assert_eq!(tried[0], "us-anthropic-claude-opus-4-7-20251101-v1");
        assert!(at("claude-opus-4-7-20251101").is_some(), "{tried:?}");
        assert!(at("claude-opus-4-7").is_some(), "{tried:?}");
        // The most specific name is tried before the bare model.
        assert!(at("claude-opus-4-7-20251101") < at("claude-opus-4-7"));
        assert!(names("claude-opus-4.7-1m").contains(&"claude-opus-4-7".to_owned()));
        assert!(names("gpt-5.5-xhigh").contains(&"gpt-5-5".to_owned()));
        assert!(names("gemini-2.5-pro-2025-06-17").contains(&"gemini-2-5-pro".to_owned()));
        // A number that is part of the model's name stays.
        assert!(!names("claude-opus-4-1").contains(&"claude-opus-4".to_owned()));
        assert!(!names("gpt-4o").contains(&"gpt".to_owned()));
    }

    /// Upstreams spell a model their own way; each finds the maker's entry.
    #[test]
    fn an_upstreams_spelling_finds_the_catalogue_entry() {
        let catalog = listing(&[
            ("anthropic", "claude-opus-4-7", 1_000_000),
            (
                "aaa-reseller",
                "anthropic/claude-opus-4.7:thinking",
                200_000,
            ),
            ("openai", "gpt-5.5", 1_050_000),
        ]);
        for sent in [
            "claude-opus-4.7",
            "claude-opus-4.7-1m",
            "Claude-Opus-4.7",
            "claude-opus-4-7-thinking",
            "claude-opus-4.7-high",
            "us.anthropic.claude-opus-4-7",
            "claude-opus-4-7-20260101",
        ] {
            let found = catalog.lookup(sent).unwrap_or_else(|| panic!("{sent}"));
            assert_eq!(found.limit.context, 1_000_000, "{sent} found {}", found.id);
        }
        assert_eq!(catalog.lookup("gpt-5.5-xhigh").unwrap().id, "gpt-5.5");
        // The provider serving the model still answers for itself.
        assert_eq!(
            catalog
                .lookup_for("aaa-reseller", "claude-opus-4.7")
                .unwrap()
                .limit
                .context,
            200_000
        );
    }

    #[test]
    fn a_dated_or_sized_listing_wins_over_the_bare_one() {
        let catalog = listing(&[
            ("anthropic", "claude-sonnet-4-5", 200_000),
            ("anthropic", "claude-sonnet-4-5-20250929", 1_000_000),
        ]);
        assert_eq!(
            catalog
                .lookup("claude-sonnet-4.5-20250929")
                .unwrap()
                .limit
                .context,
            1_000_000
        );
        assert_eq!(
            catalog.lookup("claude-sonnet-4.5").unwrap().limit.context,
            200_000
        );
    }

    /// The maker lists only a decorated id; a gateway lists the bare name
    /// at $0. The maker's figures still answer.
    #[test]
    fn the_makers_decorated_listing_answers_for_the_bare_name() {
        let mut catalog = listing(&[
            ("aaa-gateway", "gemini-3-1-pro", 1_048_576),
            ("google", "gemini-3.1-pro-preview", 1_048_576),
        ]);
        catalog
            .providers
            .get_mut("google")
            .unwrap()
            .models
            .get_mut("gemini-3.1-pro-preview")
            .unwrap()
            .cost
            .input = 2.0;
        let found = catalog.lookup("gemini-3.1-pro").unwrap();
        assert_eq!(
            (found.id.as_str(), found.cost.input),
            ("gemini-3.1-pro-preview", 2.0)
        );
    }

    /// With no maker listing, a priced entry beats a gateway's $0.
    #[test]
    fn a_stated_price_beats_a_free_listing() {
        let mut catalog = listing(&[
            ("aaa-gateway", "acme-coder-2", 128_000),
            ("bbb-reseller", "acme-coder-2", 128_000),
        ]);
        catalog
            .providers
            .get_mut("bbb-reseller")
            .unwrap()
            .models
            .get_mut("acme-coder-2")
            .unwrap()
            .cost
            .input = 1.5;
        assert_eq!(catalog.lookup("acme-coder-2").unwrap().cost.input, 1.5);
    }

    #[test]
    fn an_unknown_model_is_not_given_anothers_figures() {
        let catalog = listing(&[("anthropic", "claude-opus-4-7", 1_000_000)]);
        assert!(catalog.lookup("totally-unknown-model").is_none());
        assert!(catalog.lookup("claude-opus-9").is_none());
    }

    #[test]
    fn deepseek_publishes_its_own_prices() {
        let pro = official("deepseek", "deepseek-v4-pro").expect("pro");
        assert_eq!((pro.cost.input, pro.cost.output), (0.66, 1.98));
        assert_eq!(pro.cost.cache_read, Some(0.022));
        assert_eq!(pro.limit.context, 1_000_000);
        assert!(!pro.modalities.supports_vision());

        let flash = official("deepseek", "deepseek-flash").expect("flash");
        assert_eq!((flash.cost.input, flash.cost.output), (0.15, 0.6));
        assert!(flash.modalities.supports_vision());
        assert!(flash.reasoning && flash.tool_call);

        // Retired names still work and bill as Flash.
        let legacy = official("deepseek", "deepseek-v4-flash").expect("legacy");
        assert_eq!(legacy.cost.input, 0.15);

        assert!(official("deepseek", "deepseek-chat").is_none());
        assert!(official("openai", "deepseek-flash").is_none());
    }
}
