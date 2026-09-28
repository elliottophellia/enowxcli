//! The model lists providers returned, kept per provider in
//! `~/.enx/provider-models.json`.
//!
//! The `/model` list shows these at once and refreshes them in the
//! background, and a model's context window is read from them when the
//! provider stated one: it describes how the model is actually served,
//! which a public catalogue cannot know.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::ModelInfo;
use crate::config::{atomic_write, home_dir};

pub fn detected_path() -> PathBuf {
    home_dir().join("provider-models.json")
}

/// One provider's list, as it last answered.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Listing {
    /// Unix seconds.
    pub fetched_at: u64,
    pub models: Vec<ModelInfo>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Detected {
    #[serde(flatten)]
    pub providers: BTreeMap<String, Listing>,
}

impl Detected {
    /// Read the file; missing or unreadable is no lists.
    pub fn load() -> Self {
        std::fs::read_to_string(detected_path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn listing(&self, provider: &str) -> Option<&Listing> {
        self.providers.get(provider)
    }

    pub fn model(&self, provider: &str, model: &str) -> Option<&ModelInfo> {
        self.listing(provider)?
            .models
            .iter()
            .find(|info| info.id == model)
    }

    /// Replace `provider`'s list with what it just returned, and write the
    /// file.
    pub fn store(provider: &str, models: &[ModelInfo]) -> Result<()> {
        let mut detected = Self::load();
        detected.providers.insert(
            provider.to_owned(),
            Listing {
                fetched_at: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
                models: models.to_vec(),
            },
        );
        let text = serde_json::to_string(&detected)?;
        atomic_write(&detected_path(), text.as_bytes())
    }

    /// Drop `provider`'s list, when the provider itself is removed.
    pub fn forget(provider: &str) -> Result<()> {
        let mut detected = Self::load();
        if detected.providers.remove(provider).is_some() {
            let text = serde_json::to_string(&detected)?;
            atomic_write(&detected_path(), text.as_bytes())?;
        }
        Ok(())
    }
}
