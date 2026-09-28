//! `/provider`: every provider enx knows, each with its own key, any number
//! of them connected at once.

use super::*;
use crate::app::settings_keys::origin_of;
use crate::modal::form_fields;
use enowx_core::auth::KeySource;
use enowx_core::config::{provider_id_from, valid_provider_id};
use enowx_core::Connection;

/// The row under a provider's name: whether it is connected, how, and
/// whether the model in use is on it.
fn provider_state(connection: &Connection, active: Option<&str>) -> String {
    let mut parts = vec![match &connection.key_source {
        Some(KeySource::Stored) => "Connected".to_owned(),
        Some(KeySource::Env(name)) => format!("Connected · key from {name}"),
        Some(KeySource::Session) => "Connected for this session".to_owned(),
        None if connection.is_local() => "Connected · on this machine, no key needed".to_owned(),
        None if connection.preset.is_some() => "Enter adds your API key".to_owned(),
        None => "No API key yet".to_owned(),
    }];
    if connection.preset.is_none() {
        if let Some(origin) = origin_of(&connection.base_url) {
            parts.push(origin.split("://").nth(1).unwrap_or(&origin).to_owned());
        }
    }
    if active == Some(connection.id.as_str()) {
        parts.push("in use".to_owned());
    }
    parts.join(" · ")
}

impl App {
    pub(crate) fn open_providers(&mut self) {
        self.modal_cursor = 0;
        self.refresh_providers();
    }

    /// Fill the provider list from the configuration, keeping the cursor.
    pub(crate) fn refresh_providers(&mut self) {
        self.modal = Modal::Providers;
        self.modal_error.clear();
        let active = self.config.active_connection().map(|c| c.id);
        let connections = self.config.connections();
        self.provider_ids = connections.iter().map(|c| c.id.clone()).collect();
        self.modal_items = connections
            .iter()
            .map(|c| (c.name.clone(), provider_state(c, active.as_deref())))
            .collect();
        self.modal_items.push((
            "Add a custom provider".into(),
            "Any OpenAI-compatible endpoint".into(),
        ));
        self.modal_cursor = self.modal_cursor.min(self.modal_items.len() - 1);
    }

    pub(crate) fn open_form(&mut self, modal: Modal) {
        self.modal = modal;
        self.modal_cursor = 0;
        self.modal_error.clear();
        self.field_cursor = form_fields(modal)
            .first()
            .map(|field| self.settings.value(*field).len())
            .unwrap_or(0);
    }

    /// Enter on a provider row: a built-in provider asks for its key, a
    /// custom one opens its settings, the last row adds a new one.
    pub(crate) fn select_provider(&mut self) {
        let Some(id) = self.provider_ids.get(self.modal_cursor).cloned() else {
            self.settings = SettingsDraft::default();
            self.open_form(Modal::ProviderForm);
            return;
        };
        let Some(connection) = self.config.connection(&id) else {
            return;
        };
        self.settings = SettingsDraft::for_connection(&connection);
        self.open_form(if connection.preset.is_some() {
            Modal::ProviderKey
        } else {
            Modal::ProviderForm
        });
    }

    /// Store a built-in provider's key, keeping every other provider's, and
    /// open its models.
    pub(crate) fn connect_key(&mut self) -> Result<()> {
        let key = self.settings.api_key.trim().to_owned();
        anyhow::ensure!(!key.is_empty(), "Enter the API key.");
        let id = self.settings.provider_id.clone();
        let mut next = self.config.clone();
        next.auth.store(&id, &key)?;
        if !next.is_ready() {
            next.settle_model();
        }
        let name = next.connection(&id).map(|c| c.name).unwrap_or(id.clone());
        self.adopt(next);
        self.status = format!("{name} connected");
        self.open_model_picker(Some(&id));
        Ok(())
    }

    /// Save the custom provider in the form: its entry in `config.toml`, and
    /// a typed key in `auth.json`.
    pub(crate) fn save_provider_form(&mut self) -> Result<()> {
        let name = self.settings.name.trim().to_owned();
        let base_url = self
            .settings
            .base_url
            .trim()
            .trim_end_matches('/')
            .to_owned();
        let mut models_url = self.settings.models_url.trim().to_owned();
        let key = self.settings.api_key.trim().to_owned();
        anyhow::ensure!(!name.is_empty(), "Give the provider a name.");
        anyhow::ensure!(
            !base_url.is_empty(),
            "Enter its base URL, such as https://host/v1."
        );
        let editing = !self.settings.provider_id.is_empty();
        let id = if editing {
            self.settings.provider_id.clone()
        } else {
            provider_id_from(&name)
        };
        anyhow::ensure!(
            valid_provider_id(&id),
            "`{name}` does not make a usable id; use letters and digits."
        );
        anyhow::ensure!(
            editing || self.config.connection(&id).is_none(),
            "There is a provider called `{id}` already: pick another name, or open it from the list."
        );
        let previous = self.config.connection(&id);
        // A key issued by one service means nothing at another. Moving the
        // endpoint to a different host without typing a new key drops the
        // stored one, decided here against the finished URL: every prefix of
        // a URL is typed on the way to it.
        let old_origin = previous.as_ref().and_then(|p| origin_of(&p.base_url));
        let new_origin = origin_of(&base_url);
        let moved = matches!((&old_origin, &new_origin), (Some(a), Some(b)) if a != b);
        if moved {
            // A model-list URL on the old host moves with it.
            if let (Some(old), Some(new)) = (&old_origin, &new_origin) {
                if let Some(path) = models_url.strip_prefix(old.as_str()) {
                    models_url = format!("{new}{path}");
                }
            }
        }
        let mut next = self.config.clone();
        let mut entry = next.provider.get(&id).cloned().unwrap_or_default();
        entry.name = if name == id {
            String::new()
        } else {
            name.clone()
        };
        entry.base_url = base_url;
        entry.models_url = models_url;
        next.provider.insert(id.clone(), entry);
        next.save()?;
        if !key.is_empty() {
            next.auth.store(&id, &key)?;
        } else if moved {
            next.auth.forget(&id)?;
        }
        if !next.is_ready() {
            next.settle_model();
        }
        let connected = next.connection(&id).is_some_and(|c| c.is_connected());
        self.adopt(next);
        if connected {
            self.status = format!("{name} saved");
            self.open_model_picker(Some(&id));
        } else {
            self.status = format!("{name} saved; it needs an API key");
            self.refresh_providers();
        }
        Ok(())
    }

    /// Delete on a provider row: remove its stored key. A custom provider
    /// with no key left is removed on the next press.
    pub(crate) fn disconnect_provider(&mut self) -> Result<()> {
        let Some(id) = self.provider_ids.get(self.modal_cursor).cloned() else {
            return Ok(());
        };
        let Some(connection) = self.config.connection(&id) else {
            return Ok(());
        };
        let mut next = self.config.clone();
        if next.auth.forget(&id)? {
            self.status = format!(
                "{} disconnected: its key is gone from auth.json",
                connection.name
            );
            if let Some(KeySource::Env(name)) = next.connection(&id).and_then(|c| c.key_source) {
                self.status = format!("{}; {name} still connects it", self.status);
            }
        } else if connection.preset.is_none() && connection.configured {
            next.provider.remove(&id);
            next.save()?;
            let _ = enowx_core::provider::detected::Detected::forget(&id);
            self.status = format!("{} removed", connection.name);
        } else {
            self.status = format!("{} has no stored key", connection.name);
            return Ok(());
        }
        if !next.is_ready() {
            next.settle_model();
        }
        self.adopt(next);
        self.refresh_providers();
        Ok(())
    }
}
