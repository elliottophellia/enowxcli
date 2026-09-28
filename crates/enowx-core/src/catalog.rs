//! Model catalog fetched from https://models.dev/api.json and cached locally.
//!
//! The catalog is *fill-in-only*: it never overrides upstream provider data.
//! Fields the provider already reports (context window, pricing, capabilities)
//! stay as-is; we only fill zeros/`None`/missing entries.
//!
//! Fetch is best-effort. Missing network, malformed JSON, or an expired cache
//! all fall back to whatever is on disk (or nothing). Never blocks startup.

use std::{
    collections::BTreeMap,
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
}

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
    /// Load the cached catalog if present and still fresh. Missing/expired
    /// returns an empty catalog so callers can proceed without special-casing.
    pub fn load_cached() -> Self {
        let path = cache_path();
        let Ok(text) = fs::read_to_string(&path) else {
            return Catalog::default();
        };
        let cat: Catalog = serde_json::from_str(&text).unwrap_or_default();
        if cat.is_stale() {
            return Catalog::default();
        }
        cat
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
        let mut cat: Catalog = serde_json::from_str(&text)?;
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
        Ok(cat)
    }

    /// The entry for `model_id` as `provider` serves it: that provider's own
    /// listing first, then the whole catalogue.
    ///
    /// `lookup` alone takes the first provider in alphabetical order that
    /// lists the id, and many resellers list the same ids at their own
    /// prices: `deepseek-chat` resolved to helicone's before DeepSeek's.
    pub fn lookup_for(&self, provider: &str, model_id: &str) -> Option<&CatalogModel> {
        if let Some(listing) = self.providers.get(provider.trim()) {
            if let Some(model) = listing.models.get(model_id) {
                return Some(model);
            }
            let bare = model_id.split('/').next_back().unwrap_or(model_id);
            if let Some(model) = listing
                .models
                .iter()
                .find(|(id, _)| id.split('/').next_back().unwrap_or(id) == bare)
                .map(|(_, model)| model)
            {
                return Some(model);
            }
        }
        self.lookup(model_id)
    }

    /// Best-effort lookup: exact match first, then fuzzy across the whole
    /// catalog. Returns `None` when no candidate scores well enough.
    pub fn lookup(&self, model_id: &str) -> Option<&CatalogModel> {
        // Exact match under any provider.
        for prov in self.providers.values() {
            if let Some(m) = prov.models.get(model_id) {
                return Some(m);
            }
        }
        // Also try the "provider/model" form: strip provider prefix.
        let bare = model_id.split('/').next_back().unwrap_or(model_id);
        for prov in self.providers.values() {
            for (id, m) in &prov.models {
                let cand = id.split('/').next_back().unwrap_or(id);
                if cand == bare {
                    return Some(m);
                }
            }
        }
        // Fuzzy: pick the best candidate by lowercase substring / edit
        // distance. We only trust close matches so we do not silently attach
        // pricing from a wildly different model.
        let mut best: Option<(&CatalogModel, usize)> = None;
        let needle = model_id.to_ascii_lowercase();
        for prov in self.providers.values() {
            for (id, m) in &prov.models {
                let hay = id.to_ascii_lowercase();
                let score = fuzzy_score(&needle, &hay);
                if score > 0 && best.is_none_or(|(_, s)| score > s) {
                    best = Some((m, score));
                }
            }
        }
        best.filter(|(_, s)| *s >= 70).map(|(m, _)| m)
    }
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
        assert_eq!(catalog.lookup("deepseek-flash").unwrap().cost.input, 9.0);
        assert_eq!(
            catalog
                .lookup_for("deepseek", "deepseek-flash")
                .unwrap()
                .cost
                .input,
            0.15
        );
        // Unknown to that provider: the whole catalogue still answers.
        assert_eq!(
            catalog
                .lookup_for("openai", "deepseek-flash")
                .unwrap()
                .cost
                .input,
            9.0
        );
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
