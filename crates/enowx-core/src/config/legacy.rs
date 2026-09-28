//! Configurations written before providers had ids: one `[provider]` table
//! holding the endpoint and its key, and a bare model id in
//! `[model] default`. Moved to the current layout the first time they are
//! loaded:
//!
//! - the endpoint to `[provider.<id>]`, or nowhere when it is a built-in
//!   provider's own;
//! - the key to `auth.json`;
//! - the model to the recent list in `model.json`, as `<id>/<model>`;
//! - model ids in `[agent.models]` and `[agent.tiers]` prefixed with the id,
//!   since they all ran on that one provider.

use std::path::Path;

use anyhow::{Context as _, Result};
use toml::{Table, Value};

use super::{provider_id_from, valid_provider_id, Config};
use crate::provider::provider_preset;

/// What an old configuration kept that now lives in other files.
pub(super) struct Moved {
    id: String,
    key: Option<String>,
    model: Option<String>,
}

/// The fields of the old single `[provider]` table.
const OLD_FIELDS: [&str; 5] = ["name", "preset", "base_url", "models_url", "api_key"];

/// Take an old `[provider]` table out of `table` and put the current layout
/// in its place. None, and nothing changed, when `table` is not an old
/// configuration.
pub(super) fn take(table: &mut Table) -> Option<Moved> {
    let old = table.get("provider")?.as_table()?;
    if !OLD_FIELDS
        .iter()
        .any(|field| old.get(*field).is_some_and(Value::is_str))
    {
        return None;
    }
    let text = |field: &str| {
        old.get(field)
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_owned()
    };
    let (name, preset_id, base_url, models_url, key) = (
        text("name"),
        text("preset"),
        text("base_url"),
        text("models_url"),
        text("api_key"),
    );
    let preset = provider_preset(&preset_id);
    let id = match preset {
        Some(preset) => preset.id.to_owned(),
        None => [
            provider_id_from(&name),
            reqwest::Url::parse(&base_url)
                .ok()
                .and_then(|url| url.host_str().map(provider_id_from))
                .unwrap_or_default(),
        ]
        .into_iter()
        .find(|id| valid_provider_id(id))
        .unwrap_or_else(|| "custom".to_owned()),
    };

    // Only what differs from the built-in provider is written.
    let differs = |own: &str, built_in: Option<&str>| {
        !own.is_empty()
            && Some(own.trim_end_matches('/')) != built_in.map(|b| b.trim_end_matches('/'))
    };
    let mut entry = Table::new();
    if differs(&name, preset.map(|p| p.name)) && name != id {
        entry.insert("name".into(), Value::String(name.clone()));
    }
    if differs(&base_url, preset.map(|p| p.base_url)) {
        entry.insert(
            "base_url".into(),
            Value::String(base_url.trim_end_matches('/').to_owned()),
        );
    }
    if differs(&models_url, preset.map(|p| p.models_url)) {
        entry.insert("models_url".into(), Value::String(models_url.clone()));
    }
    let mut providers = Table::new();
    if !entry.is_empty() {
        providers.insert(id.clone(), Value::Table(entry));
    }
    table.insert("provider".into(), Value::Table(providers));

    let model = table
        .get_mut("model")
        .and_then(Value::as_table_mut)
        .and_then(|model| model.remove("default"))
        .and_then(|value| value.as_str().map(|m| m.trim().to_owned()))
        .filter(|model| !model.is_empty())
        .map(|model| format!("{id}/{model}"));

    if let Some(agent) = table.get_mut("agent").and_then(Value::as_table_mut) {
        for section in ["models", "tiers"] {
            let Some(models) = agent.get_mut(section).and_then(Value::as_table_mut) else {
                continue;
            };
            for (_, value) in models.iter_mut() {
                if let Some(model) = value.as_str().map(str::trim).filter(|m| !m.is_empty()) {
                    *value = Value::String(format!("{id}/{model}"));
                }
            }
        }
    }

    Some(Moved {
        id,
        key: (!key.is_empty()).then_some(key),
        model,
    })
}

impl Moved {
    /// Keep a copy of the old file, store the key and the model where they
    /// live now, then write the configuration in its new form. The old file
    /// is replaced last, so a failure part-way loses nothing: the next start
    /// moves it again.
    pub(super) fn finish(self, config: &mut Config, path: &Path) -> Result<()> {
        if path.exists() {
            let backup = path.with_file_name("config.toml.before-providers.bak");
            if !backup.exists() {
                std::fs::copy(path, &backup)
                    .with_context(|| format!("keeping a copy at {}", backup.display()))?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&backup, std::fs::Permissions::from_mode(0o600))?;
                }
            }
        }
        if let Some(key) = &self.key {
            if !config.auth.is_stored(&self.id) {
                config.auth.store(&self.id, key)?;
            }
        }
        if let Some(model) = &self.model {
            let mut state = crate::model_state::ModelState::load();
            if !state.recent.contains(model) {
                state.remember(model);
                state.save()?;
            }
        }
        config.save()?;
        Ok(())
    }
}
