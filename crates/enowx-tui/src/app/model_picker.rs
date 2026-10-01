//! `/model`: the models of every connected provider in one list, the
//! favourite and recent ones first, filtered as the user types. A pick is
//! remembered in `model.json`, never written to `config.toml`.

use super::*;
use enowx_core::config::ModelEntry;
use enowx_core::model_state::ModelState;
use enowx_core::provider::detected::Detected;
use enowx_core::{Connection, ModelRef};

/// One provider's models as far as they are known: the last list it gave,
/// its catalogue entry until then, and the models added by hand.
pub(crate) struct ProviderModels {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) models: Vec<ModelInfo>,
    /// Its list is being asked for.
    pub(crate) loading: bool,
    pub(crate) error: Option<String>,
}

/// Model lists arriving from providers, by provider id.
type Listings = mpsc::UnboundedReceiver<(String, std::result::Result<Vec<ModelInfo>, String>)>;

#[derive(Default)]
pub(crate) struct ModelPicker {
    pub(crate) providers: Vec<ProviderModels>,
    pub(crate) state: ModelState,
    pub(crate) events: Option<Listings>,
}

/// A row of the model list.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PickerRow {
    Heading(String),
    Model {
        /// `provider/model`.
        model: String,
        label: String,
        detail: String,
    },
    Note(String),
}

/// `1M`, `200k`: a context window at a glance.
fn tokens(n: u32) -> String {
    if n >= 1_000_000 {
        let m = n as f64 / 1_000_000.0;
        if (m - m.round()).abs() < 0.05 {
            format!("{}M", m.round())
        } else {
            format!("{m:.1}M")
        }
    } else if n >= 1_000 {
        format!("{}k", (n as f64 / 1_000.0).round())
    } else {
        n.to_string()
    }
}

/// The models `connection` is known to serve before its list is asked for.
fn known_models(config: &Config, connection: &Connection, detected: &Detected) -> Vec<ModelInfo> {
    let mut models = detected
        .listing(&connection.id)
        .map(|listing| listing.models.clone())
        .unwrap_or_default();
    if models.is_empty() && !connection.catalog_id().is_empty() {
        let catalog = enowx_core::catalog::Catalog::shared();
        if let Some(listing) = catalog.providers.get(connection.catalog_id()) {
            models = listing
                .models
                .iter()
                .map(|(id, model)| ModelInfo {
                    id: id.clone(),
                    name: (!model.name.is_empty()).then(|| model.name.clone()),
                    context_window: (model.limit.context > 0).then_some(model.limit.context),
                    max_output_tokens: (model.limit.output > 0).then_some(model.limit.output),
                })
                .collect();
        }
    }
    with_own_models(config, &connection.id, models)
}

/// `models` with the ones added by hand for `provider` that it lacks.
fn with_own_models(config: &Config, provider: &str, mut models: Vec<ModelInfo>) -> Vec<ModelInfo> {
    if let Some(entry) = config.provider.get(provider) {
        for (id, own) in &entry.models {
            if !models.iter().any(|m| &m.id == id) {
                models.push(ModelInfo {
                    id: id.clone(),
                    name: None,
                    context_window: own.context_window,
                    max_output_tokens: None,
                });
            }
        }
    }
    models
}

impl App {
    /// Open the list, on the model in use or else on `focus`'s models.
    pub(crate) fn open_model_picker(&mut self, focus: Option<&str>) {
        let connected = self.config.connected();
        if connected.is_empty() {
            self.status = "connect a provider first".into();
            self.open_providers();
            return;
        }
        let detected = Detected::load();
        self.picker = ModelPicker {
            state: ModelState::load(),
            providers: connected
                .iter()
                .map(|connection| ProviderModels {
                    id: connection.id.clone(),
                    name: connection.name.clone(),
                    models: known_models(&self.config, connection, &detected),
                    loading: !connection.models_url.trim().is_empty(),
                    error: None,
                })
                .collect(),
            events: None,
        };
        self.fetch_models(connected);
        self.modal = Modal::Models;
        self.modal_search.clear();
        self.modal_error.clear();
        let active = self.config.model.active.clone();
        let rows = self.picker_rows();
        let on_active = rows
            .iter()
            .position(|row| matches!(row, PickerRow::Model { model, .. } if *model == active));
        let on_focus = focus.and_then(|focus| {
            rows.iter().position(|row| {
                matches!(row, PickerRow::Model { model, .. }
                    if ModelRef::parse(model).is_some_and(|m| m.provider == focus))
            })
        });
        self.modal_cursor = match focus {
            Some(_) => on_focus.or(on_active),
            None => on_active.or(on_focus),
        }
        .or_else(|| first_model(&rows))
        .unwrap_or(0);
    }

    /// Ask every connected provider with a model-list URL for its list, all
    /// at once. Outside a runtime (a unit test) the known lists stay.
    fn fetch_models(&mut self, connections: Vec<Connection>) {
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            for provider in &mut self.picker.providers {
                provider.loading = false;
            }
            return;
        };
        let (tx, rx) = mpsc::unbounded_channel();
        self.picker.events = Some(rx);
        for connection in connections {
            if connection.models_url.trim().is_empty() {
                continue;
            }
            let tx = tx.clone();
            handle.spawn(async move {
                let result = async {
                    Provider::for_connection(&connection)?
                        .models(&connection.models_url)
                        .await
                }
                .await
                .map_err(|error| format!("{error:#}"));
                let _ = tx.send((connection.id, result));
            });
        }
    }

    /// Take in the lists that have arrived, keeping the selected model
    /// selected as rows move around it.
    pub(crate) fn drain_picker_events(&mut self) {
        let Some(events) = self.picker.events.as_mut() else {
            return;
        };
        let mut arrived = Vec::new();
        while let Ok(item) = events.try_recv() {
            arrived.push(item);
        }
        if arrived.is_empty() {
            return;
        }
        let selected = self.selected_model();
        for (id, result) in arrived {
            let own = |models| with_own_models(&self.config, &id, models);
            let Some(provider) = self.picker.providers.iter_mut().find(|p| p.id == id) else {
                continue;
            };
            provider.loading = false;
            match result {
                Ok(models) => {
                    let _ = Detected::store(&id, &models);
                    provider.models = own(models);
                    provider.error = None;
                }
                Err(error) => provider.error = Some(error),
            }
        }
        if self.modal == Modal::Models {
            let rows = self.picker_rows();
            self.modal_cursor = selected
                .and_then(|selected| {
                    rows.iter().position(
                        |row| matches!(row, PickerRow::Model { model, .. } if *model == selected),
                    )
                })
                .or_else(|| first_model(&rows))
                .unwrap_or(0);
        }
    }

    /// The rows of the list: favourites and recent picks, then each
    /// provider's models; with a search, only the models that match it.
    pub(crate) fn picker_rows(&self) -> Vec<PickerRow> {
        let search = self.modal_search.trim().to_lowercase();
        let active = self.config.model.active.as_str();
        let pin = self.config.model.default.as_str();
        let provider_of = |model: &str| {
            ModelRef::parse(model)
                .and_then(|m| self.picker.providers.iter().find(|p| p.id == m.provider))
        };
        let info_of = |model: &str| {
            let parsed = ModelRef::parse(model)?;
            provider_of(model)?
                .models
                .iter()
                .find(|info| info.id == parsed.model)
        };
        let detail = |model: &str, info: Option<&ModelInfo>, with_provider: bool| {
            let mut parts = Vec::new();
            if with_provider {
                if let Some(provider) = provider_of(model) {
                    parts.push(provider.name.clone());
                }
            }
            if let Some(info) = info {
                if let Some(name) = info.name.as_deref().filter(|name| *name != info.id) {
                    parts.push(name.to_owned());
                }
                if let Some(window) = info.context_window {
                    parts.push(format!("{} context", tokens(window)));
                }
            }
            if model == active {
                parts.push("in use".to_owned());
            }
            if model == pin {
                parts.push("pinned in config.toml".to_owned());
            }
            parts.join(" · ")
        };
        let row = |model: &str, label: String, with_provider: bool| {
            let star = if self.picker.state.is_favorite(model) {
                "★ "
            } else {
                ""
            };
            PickerRow::Model {
                model: model.to_owned(),
                label: format!("{star}{label}"),
                detail: detail(model, info_of(model), with_provider),
            }
        };
        let mut rows = Vec::new();
        if search.is_empty() {
            let favourites: Vec<&String> = self
                .picker
                .state
                .favorite
                .iter()
                .filter(|model| provider_of(model).is_some())
                .collect();
            if !favourites.is_empty() {
                rows.push(PickerRow::Heading("Favourites".into()));
                for model in favourites {
                    let label = ModelRef::parse(model).map(|m| m.model).unwrap_or_default();
                    rows.push(row(model, label, true));
                }
            }
            let recent: Vec<&String> = self
                .picker
                .state
                .recent
                .iter()
                .filter(|model| {
                    provider_of(model).is_some() && !self.picker.state.is_favorite(model)
                })
                .collect();
            if !recent.is_empty() {
                rows.push(PickerRow::Heading("Recent".into()));
                for model in recent {
                    let label = ModelRef::parse(model).map(|m| m.model).unwrap_or_default();
                    rows.push(row(model, label, true));
                }
            }
        }
        for provider in &self.picker.providers {
            let models: Vec<&ModelInfo> = provider
                .models
                .iter()
                .filter(|info| {
                    search.is_empty()
                        || format!("{}/{}", provider.id, info.id)
                            .to_lowercase()
                            .contains(&search)
                        || provider.name.to_lowercase().contains(&search)
                        || info
                            .name
                            .as_deref()
                            .is_some_and(|name| name.to_lowercase().contains(&search))
                })
                .collect();
            if models.is_empty() && !search.is_empty() {
                continue;
            }
            rows.push(PickerRow::Heading(provider.name.clone()));
            if models.is_empty() {
                rows.push(PickerRow::Note(if provider.loading {
                    format!("Asking {} for its models…", provider.name)
                } else if let Some(error) = &provider.error {
                    format!("{error}. F5 asks again; F2 adds a model by hand.")
                } else {
                    "No models listed. F2 adds one by hand.".into()
                }));
                continue;
            }
            if let Some(error) = &provider.error {
                rows.push(PickerRow::Note(format!(
                    "Showing its last list; asking again failed: {error}"
                )));
            }
            for info in models {
                let model = format!("{}/{}", provider.id, info.id);
                rows.push(row(&model, info.id.clone(), false));
            }
        }
        rows
    }

    /// The model on the selected row.
    pub(crate) fn selected_model(&self) -> Option<String> {
        match self.picker_rows().get(self.modal_cursor)? {
            PickerRow::Model { model, .. } => Some(model.clone()),
            _ => None,
        }
    }

    /// Move the selection by `step` model rows, over headings and notes.
    pub(crate) fn move_picker(&mut self, step: isize) {
        let rows = self.picker_rows();
        let models: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| matches!(row, PickerRow::Model { .. }))
            .map(|(index, _)| index)
            .collect();
        if models.is_empty() {
            return;
        }
        let at = models
            .iter()
            .position(|index| *index >= self.modal_cursor)
            .unwrap_or(models.len() - 1);
        let next = (at as isize + step).clamp(0, models.len() as isize - 1) as usize;
        self.modal_cursor = models[next];
    }

    /// Typing narrows the list; the selection goes to the first match.
    pub(crate) fn search_models(&mut self, edit: impl FnOnce(&mut String)) {
        edit(&mut self.modal_search);
        self.modal_cursor = first_model(&self.picker_rows()).unwrap_or(0);
    }

    /// Use `raw` from now on, and put it first in the recent list.
    pub(crate) fn choose_model(&mut self, raw: &str) -> Result<()> {
        let model = self.config.parse_model(raw).ok_or_else(|| {
            anyhow::anyhow!("`{raw}` names no provider enx knows; write it as provider/model")
        })?;
        let connection = self
            .config
            .connection(&model.provider)
            .ok_or_else(|| anyhow::anyhow!("enx knows no provider `{}`", model.provider))?;
        anyhow::ensure!(
            connection.is_connected(),
            "{} is not connected: add its key in /provider",
            connection.name
        );
        let mut next = self.config.clone();
        next.use_model(&model.to_string());
        let mut state = ModelState::load();
        state.remember(&model.to_string());
        state.save()?;
        let pin = next.model.default.clone();
        self.adopt(next);
        self.modal = Modal::None;
        self.modal_search.clear();
        self.status = if !pin.is_empty() && pin != model.to_string() {
            format!("model: {model} (config.toml still starts on {pin})")
        } else {
            format!("model: {model}")
        };
        Ok(())
    }

    /// Mark the selected model as a favourite, or unmark it.
    pub(crate) fn toggle_favorite_model(&mut self) -> Result<()> {
        let Some(model) = self.selected_model() else {
            return Ok(());
        };
        let favourite = self.picker.state.toggle_favorite(&model);
        self.picker.state.save()?;
        self.status = if favourite {
            format!("{model} is a favourite")
        } else {
            format!("{model} is no longer a favourite")
        };
        let rows = self.picker_rows();
        if let Some(index) = rows
            .iter()
            .position(|row| matches!(row, PickerRow::Model { model: m, .. } if *m == model))
        {
            self.modal_cursor = index;
        }
        Ok(())
    }

    /// F5: ask every provider for its list again.
    pub(crate) fn refresh_models(&mut self) {
        let connected = self.config.connected();
        for provider in &mut self.picker.providers {
            provider.loading = connected
                .iter()
                .any(|c| c.id == provider.id && !c.models_url.trim().is_empty());
            provider.error = None;
        }
        self.fetch_models(connected);
    }

    /// F2: a model by hand, for the provider of the selected row.
    pub(crate) fn open_manual_model(&mut self) {
        let rows = self.picker_rows();
        let provider = self
            .selected_model()
            .and_then(|model| ModelRef::parse(&model))
            .map(|model| model.provider)
            .or_else(|| {
                // On a heading or a note: the provider section it sits in.
                rows[..=self.modal_cursor.min(rows.len().saturating_sub(1))]
                    .iter()
                    .rev()
                    .find_map(|row| match row {
                        PickerRow::Heading(name) => self
                            .picker
                            .providers
                            .iter()
                            .find(|p| &p.name == name)
                            .map(|p| p.id.clone()),
                        _ => None,
                    })
            })
            .or_else(|| self.picker.providers.first().map(|p| p.id.clone()));
        let Some(provider) = provider else {
            return;
        };
        let name = self
            .config
            .connection(&provider)
            .map(|c| c.name)
            .unwrap_or_else(|| provider.clone());
        self.settings = SettingsDraft {
            provider_id: provider,
            name,
            ..SettingsDraft::default()
        };
        self.open_form(Modal::ModelManual);
    }

    /// F3: edit the selected model's properties. Prefills from what is set
    /// now (an explicit override, else the detected or catalogue value) so
    /// the form shows the real figures and a save keeps the ones untouched.
    pub(crate) fn open_edit_model(&mut self) {
        let Some(id) = self.selected_model() else {
            self.status = "Select a model row to edit.".into();
            return;
        };
        let Some(model) = ModelRef::parse(&id) else {
            return;
        };
        let facts = self.config.facts(&model);
        let efforts = self.config.efforts(&model);
        let own = self
            .config
            .provider
            .get(&model.provider)
            .and_then(|entry| entry.models.get(&model.model))
            .cloned()
            .unwrap_or_default();
        let effort = ModelState::load()
            .effort
            .get(&id)
            .cloned()
            .filter(|level| efforts.contains(level))
            .unwrap_or_default();
        let num = |value: f64| if value > 0.0 { format!("{value}") } else { String::new() };
        self.settings = SettingsDraft {
            provider_id: model.provider.clone(),
            model: model.model.clone(),
            context_window: own
                .context_window
                .map(|w| w.to_string())
                .unwrap_or_else(|| facts.context_window.to_string()),
            effort,
            vision: match own.vision {
                Some(true) => "yes".into(),
                Some(false) => "no".into(),
                None => String::new(),
            },
            price_input: own.price_input.map(num).unwrap_or_default(),
            price_output: own.price_output.map(num).unwrap_or_default(),
            efforts,
            ..SettingsDraft::default()
        };
        self.open_form(Modal::ModelEdit);
    }

    /// Save the edited model's properties to its `ModelEntry`, apply the
    /// effort, and keep using the model. An empty field clears that override
    /// and lets the detected or catalogue value take over again.
    pub(crate) fn save_edit_model(&mut self) -> Result<()> {
        let id = self.settings.provider_id.clone();
        let model = self.settings.model.clone();
        anyhow::ensure!(!model.is_empty(), "No model to edit.");
        let full = format!("{id}/{model}");

        let parse_u32 = |raw: &str| -> Result<Option<u32>> {
            let raw = raw.trim().replace(['_', ',', '.'], "");
            if raw.is_empty() {
                return Ok(None);
            }
            Ok(Some(raw.parse::<u32>().map_err(|_| {
                anyhow::anyhow!("The context window is a whole number of tokens.")
            })?))
        };
        let parse_price = |raw: &str, what: &str| -> Result<Option<f64>> {
            let raw = raw.trim().replace([',', '_'], "");
            if raw.is_empty() {
                return Ok(None);
            }
            let value = raw
                .parse::<f64>()
                .map_err(|_| anyhow::anyhow!("The {what} is a number of dollars per 1M tokens."))?;
            anyhow::ensure!(value >= 0.0, "The {what} cannot be negative.");
            Ok(Some(value))
        };

        let context_window = parse_u32(&self.settings.context_window)?;
        let price_input = parse_price(&self.settings.price_input, "input price")?;
        let price_output = parse_price(&self.settings.price_output, "output price")?;
        let vision = match self.settings.vision.as_str() {
            "yes" => Some(true),
            "no" => Some(false),
            _ => None,
        };
        let effort = self.settings.effort.clone();

        let mut next = self.config.clone();
        let entry = next.provider.entry(id.clone()).or_default();
        if context_window.is_none()
            && vision.is_none()
            && price_input.is_none()
            && price_output.is_none()
        {
            // Everything cleared: drop the override row rather than leave an
            // empty one behind.
            let kept_cache = entry.models.get(&model).and_then(|m| m.price_cache_read);
            if kept_cache.is_some() {
                entry.models.insert(
                    model.clone(),
                    ModelEntry { price_cache_read: kept_cache, ..ModelEntry::default() },
                );
            } else {
                entry.models.remove(&model);
            }
        } else {
            let cache = entry.models.get(&model).and_then(|m| m.price_cache_read);
            entry.models.insert(
                model.clone(),
                ModelEntry {
                    context_window,
                    vision,
                    price_input,
                    price_output,
                    price_cache_read: cache,
                },
            );
        }
        next.save()?;
        self.adopt(next);
        self.choose_model(&full)?;
        // Effort lives in model state, applied against the now-active model.
        self.config.set_effort(&effort)?;
        self.status = format!("edited {full}");
        self.modal = Modal::None;
        Ok(())
    }

    /// Add the model in the form to its provider's entry, and use it.
    pub(crate) fn save_manual_model(&mut self) -> Result<()> {
        let model = self.settings.model.trim().to_owned();
        anyhow::ensure!(!model.is_empty(), "Enter the model ID the provider uses.");
        let window = self
            .settings
            .context_window
            .trim()
            .replace(['_', ',', '.'], "");
        let context_window =
            if window.is_empty() {
                None
            } else {
                Some(window.parse::<u32>().map_err(|_| {
                    anyhow::anyhow!("The context window is a whole number of tokens.")
                })?)
            };
        let id = self.settings.provider_id.clone();
        let mut next = self.config.clone();
        next.provider.entry(id.clone()).or_default().models.insert(
            model.clone(),
            ModelEntry {
                context_window,
                ..ModelEntry::default()
            },
        );
        next.save()?;
        self.adopt(next);
        self.choose_model(&format!("{id}/{model}"))
    }
}

fn first_model(rows: &[PickerRow]) -> Option<usize> {
    rows.iter()
        .position(|row| matches!(row, PickerRow::Model { .. }))
}

#[cfg(test)]
mod tests {
    use super::tokens;

    #[test]
    fn a_window_reads_at_a_glance() {
        assert_eq!(tokens(1_000_000), "1M");
        assert_eq!(tokens(1_048_576), "1M");
        assert_eq!(tokens(1_500_000), "1.5M");
        assert_eq!(tokens(128_000), "128k");
        assert_eq!(tokens(200_000), "200k");
        assert_eq!(tokens(512), "512");
    }
}
