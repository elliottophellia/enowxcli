//! Which agents a skill found on disk goes to.
//!
//! Built-in skills come with the agents that carry them. A skill installed in
//! a project or the user's home has no owner, so every agent is offered it
//! until it is bound: the orchestrator binds it with `skill_bind` to the
//! agents whose work it serves, and from then on only they see it. Bindings
//! live in `~/.enx/skill-bindings.json`, keyed by skill name, so they hold
//! across sessions and workspaces.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

/// Bindings shared by the discovery and the tools that read or change them,
/// so a binding made mid-turn reaches the next delegation.
pub type SharedBindings = Arc<RwLock<SkillBindings>>;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillBindings {
    /// Skill name to the agents it goes to, never empty.
    #[serde(flatten)]
    map: BTreeMap<String, Vec<String>>,
}

pub fn path() -> PathBuf {
    crate::config::home_dir().join("skill-bindings.json")
}

impl SkillBindings {
    /// The saved bindings; none when the file is missing or unreadable, so a
    /// broken file leaves every skill offered to everyone rather than hidden.
    pub fn load() -> Self {
        std::fs::read_to_string(path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        let path = path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
        }
        let text = serde_json::to_string_pretty(self)? + "\n";
        std::fs::write(&path, text).with_context(|| format!("write {}", path.display()))
    }

    /// The agents `skill` is bound to, or None while every agent is offered it.
    pub fn agents_for(&self, skill: &str) -> Option<&[String]> {
        self.map.get(skill).map(Vec::as_slice)
    }

    /// Bind `skill` to `agents`; an empty list unbinds it.
    pub fn set(&mut self, skill: &str, agents: Vec<String>) {
        let mut agents = agents;
        agents.sort();
        agents.dedup();
        if agents.is_empty() {
            self.map.remove(skill);
        } else {
            self.map.insert(skill.to_owned(), agents);
        }
    }

    /// The skills bound to `agent`.
    pub fn skills_of(&self, agent: &str) -> Vec<String> {
        self.map
            .iter()
            .filter(|(_, agents)| agents.iter().any(|a| a == agent))
            .map(|(skill, _)| skill.clone())
            .collect()
    }

    pub fn is_bound(&self, skill: &str) -> bool {
        self.map.contains_key(skill)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binding_sorts_and_an_empty_list_unbinds() {
        let mut b = SkillBindings::default();
        b.set("stripe", vec!["be".into(), "fe".into(), "be".into()]);
        assert_eq!(
            b.agents_for("stripe"),
            Some(&["be".to_owned(), "fe".to_owned()][..])
        );
        assert_eq!(b.skills_of("fe"), vec!["stripe".to_owned()]);
        assert!(b.skills_of("db").is_empty());
        b.set("stripe", Vec::new());
        assert!(!b.is_bound("stripe"));
    }

    #[test]
    fn saved_as_a_plain_map() {
        let mut b = SkillBindings::default();
        b.set("stripe", vec!["be".into()]);
        let text = serde_json::to_string(&b).unwrap();
        assert_eq!(text, r#"{"stripe":["be"]}"#);
        assert_eq!(serde_json::from_str::<SkillBindings>(&text).unwrap(), b);
    }
}
