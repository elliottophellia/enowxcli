//! API keys, one per provider, kept apart from the configuration.
//!
//! `auth.json` is a file of credentials keyed by provider id, readable only by the user, so `config.toml` holds
//! no secrets and can be read, shared or committed. Connecting a second
//! provider adds an entry; it never replaces the first.
//!
//! Keys can also come from the environment, by each built-in provider's
//! own variable (`DEEPSEEK_API_KEY`, `OPENAI_API_KEY`...). Those are read,
//! never written.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::config::{atomic_write, home_dir};

pub fn auth_path() -> PathBuf {
    home_dir().join("auth.json")
}

/// One stored credential. Tagged by `type` so other kinds can be added later
/// without a new file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Credential {
    Api { key: String },
}

/// Where a provider's key came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeySource {
    /// `auth.json`.
    Stored,
    /// An environment variable, by name.
    Env(String),
    /// Set in memory for this process only: tests, and `ENX_API_KEY`.
    Session,
}

/// The keys known to this process: stored ones, then environment ones.
#[derive(Debug, Clone, Default)]
pub struct Auth {
    stored: BTreeMap<String, String>,
    session: BTreeMap<String, (String, KeySource)>,
}

impl Auth {
    /// Read `auth.json`. A missing file is no keys; an entry of a kind this
    /// version does not know is skipped rather than failing the whole file.
    pub fn load() -> Result<Self> {
        let mut auth = Self::default();
        for (provider, value) in read_raw()? {
            if let Ok(Credential::Api { key }) = serde_json::from_value(value) {
                if !key.trim().is_empty() {
                    auth.stored.insert(provider, key);
                }
            }
        }
        Ok(auth)
    }

    /// The key for `provider` and where it came from. A key set for this
    /// session wins, then the one stored in `auth.json` (typed on purpose,
    /// so it wins), then the environment.
    pub fn key(&self, provider: &str, env: &[&str]) -> Option<(String, KeySource)> {
        if let Some((key, source)) = self.session.get(provider) {
            return Some((key.clone(), source.clone()));
        }
        if let Some(key) = self.stored.get(provider) {
            return Some((key.clone(), KeySource::Stored));
        }
        env.iter().find_map(|name| {
            std::env::var(name)
                .ok()
                .map(|key| key.trim().to_owned())
                .filter(|key| !key.is_empty())
                .map(|key| (key, KeySource::Env((*name).to_owned())))
        })
    }

    /// Whether `auth.json` holds a key for `provider`.
    pub fn is_stored(&self, provider: &str) -> bool {
        self.stored.contains_key(provider)
    }

    /// Store `key` for `provider` in `auth.json`, keeping every other entry,
    /// and here.
    pub fn store(&mut self, provider: &str, key: &str) -> Result<()> {
        let provider = provider.trim();
        let key = key.trim();
        anyhow::ensure!(!provider.is_empty(), "a provider id is required");
        anyhow::ensure!(!key.is_empty(), "the API key is empty");
        let mut raw = read_raw()?;
        raw.insert(
            provider.to_owned(),
            serde_json::to_value(Credential::Api {
                key: key.to_owned(),
            })?,
        );
        write_raw(&raw)?;
        self.stored.insert(provider.to_owned(), key.to_owned());
        Ok(())
    }

    /// Remove `provider`'s key from `auth.json`. True when there was one.
    pub fn forget(&mut self, provider: &str) -> Result<bool> {
        let mut raw = read_raw()?;
        let had = raw.remove(provider.trim()).is_some();
        if had {
            write_raw(&raw)?;
        }
        self.stored.remove(provider.trim());
        Ok(had)
    }

    /// Use `key` for `provider` in this process only.
    pub fn set_for_session(&mut self, provider: &str, key: &str, source: KeySource) {
        self.session
            .insert(provider.to_owned(), (key.to_owned(), source));
    }
}

fn read_raw() -> Result<Map<String, Value>> {
    let path = auth_path();
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(error) => return Err(error).with_context(|| format!("reading {}", path.display())),
    };
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(&text)
        .with_context(|| format!("parsing {}", path.display()))?
    {
        Value::Object(map) => Ok(map),
        _ => anyhow::bail!("{} is not a JSON object", path.display()),
    }
}

fn write_raw(raw: &Map<String, Value>) -> Result<()> {
    let path = auth_path();
    let text = serde_json::to_string_pretty(raw)?;
    atomic_write(&path, format!("{text}\n").as_bytes())?;
    // Readable by the user alone, whatever the file's mode was before:
    // `atomic_write` keeps an existing file's permissions.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("restricting {}", path.display()))?;
    }
    Ok(())
}
