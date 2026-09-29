//! The models the user has picked: the most recent first, and the ones they
//! marked as favourites. Kept in `~/.enx/model.json`, the way opencode keeps
//! its `model.json`, so choosing a model in `/model` never rewrites
//! `config.toml`.
//!
//! At start the first recent model whose provider is still connected is the
//! one in use, unless `config.toml` pins one with `model.default`.

use std::path::PathBuf;

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

use crate::config::{atomic_write, home_dir};

/// Recent models kept, most recent first.
pub const RECENT_LIMIT: usize = 10;

pub fn state_path() -> PathBuf {
    home_dir().join("model.json")
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelState {
    /// `provider/model` refs, most recent first.
    pub recent: Vec<String>,
    /// `provider/model` refs, in the order they were marked.
    pub favorite: Vec<String>,
    /// The thinking effort chosen for a model, by `provider/model` ref.
    /// A model with none uses its provider's default.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub effort: std::collections::BTreeMap<String, String>,
}

impl ModelState {
    /// Read the file; missing or unreadable is an empty history, since
    /// nothing here is worth failing a start over.
    pub fn load() -> Self {
        std::fs::read_to_string(state_path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        let path = state_path();
        let text = serde_json::to_string_pretty(self)?;
        atomic_write(&path, format!("{text}\n").as_bytes())
            .with_context(|| format!("saving {}", path.display()))
    }

    /// Put `model` first in the recent list.
    pub fn remember(&mut self, model: &str) {
        let model = model.trim();
        if model.is_empty() {
            return;
        }
        self.recent.retain(|m| m != model);
        self.recent.insert(0, model.to_owned());
        self.recent.truncate(RECENT_LIMIT);
    }

    /// Mark `model` as a favourite, or unmark it. True when it is one now.
    pub fn toggle_favorite(&mut self, model: &str) -> bool {
        let model = model.trim();
        if let Some(index) = self.favorite.iter().position(|m| m == model) {
            self.favorite.remove(index);
            false
        } else {
            self.favorite.push(model.to_owned());
            true
        }
    }

    pub fn is_favorite(&self, model: &str) -> bool {
        self.favorite.iter().any(|m| m == model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pick_moves_to_the_front_once() {
        let mut state = ModelState::default();
        state.remember("deepseek/deepseek-flash");
        state.remember("enowx/cbc/glm-5");
        state.remember("deepseek/deepseek-flash");
        assert_eq!(state.recent, ["deepseek/deepseek-flash", "enowx/cbc/glm-5"]);
    }

    #[test]
    fn the_recent_list_is_capped() {
        let mut state = ModelState::default();
        for index in 0..RECENT_LIMIT + 5 {
            state.remember(&format!("p/m{index}"));
        }
        assert_eq!(state.recent.len(), RECENT_LIMIT);
        assert_eq!(state.recent[0], format!("p/m{}", RECENT_LIMIT + 4));
    }

    #[test]
    fn a_favourite_toggles() {
        let mut state = ModelState::default();
        assert!(state.toggle_favorite("p/m"));
        assert!(state.is_favorite("p/m"));
        assert!(!state.toggle_favorite("p/m"));
        assert!(state.favorite.is_empty());
    }
}
