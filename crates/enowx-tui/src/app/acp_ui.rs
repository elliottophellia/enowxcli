//! Claude Code, Codex, Gemini CLI and custom ACP agents in the interface.
//!
//! The main path: `/acp <name>` runs the session (the lead and every
//! specialist) on that agent, installing its adapter and checking its
//! sign-in first; `/model` and `/effort` then pick from what that agent
//! offers; `/acp off` (or `/native`) goes back to enowx's own model. `/acp`
//! alone lists the agents to pick from. Settings > ACP agents has a card per
//! agent with the same choices and its permissions. The roster's `e` still
//! moves one agent on its own, for mixing engines.

use super::*;
use crate::modal::{AcpField, SettingsField};
use enowx_core::acp::{self, manager::Offer, CustomAgent, EngineConfig};

/// What background work reports.
pub(crate) enum AcpNews {
    /// Progress for the status line.
    Line(String),
    Detected(Vec<acp::Detection>),
    /// What an agent offers, or why it could not be asked.
    Offer(String, std::result::Result<Offer, String>),
    /// `/acp <name>` finished getting the agent ready.
    Ready(String, std::result::Result<Offer, String>),
}

impl App {
    fn acp_sender(&mut self) -> std::sync::mpsc::Sender<AcpNews> {
        if let Some(tx) = &self.acp_tx {
            return tx.clone();
        }
        let (tx, rx) = std::sync::mpsc::channel();
        self.acp_tx = Some(tx.clone());
        self.acp_news = Some(rx);
        tx
    }

    fn workspace_dir(&self) -> std::path::PathBuf {
        std::fs::canonicalize(self.config.workspace()).unwrap_or_else(|_| self.config.workspace())
    }

    /// The engine the active agent runs on, if any.
    pub(crate) fn acp_engine(&self) -> Option<String> {
        self.config
            .acp
            .engine_for(self.active_agent())
            .map(str::to_owned)
    }

    /// Every agent: built-ins, then custom ones.
    fn acp_ids(&self) -> Vec<String> {
        self.config.acp.engine_ids()
    }

    /// `/acp …`.
    pub(crate) fn acp_command(&mut self, args: &str) -> Result<()> {
        match args.trim() {
            "" => self.open_acp_pick(),
            "off" | "native" | "enowx" => self.acp_off()?,
            name => self.use_acp(name)?,
        }
        Ok(())
    }

    /// Go back to enowx's own model for every agent.
    pub(crate) fn acp_off(&mut self) -> Result<()> {
        let mut config = self.config.clone();
        let was = config.acp.active().map(acp::title);
        config.acp.active.clear();
        config.save()?;
        self.adopt(config);
        self.status = match was {
            Some(title) => format!("back on enowx's own model (was {title})"),
            None => "already on enowx's own model".into(),
        };
        self.push(
            TranscriptKind::System,
            "This session runs on enowx's own model again. /model picks it.".to_owned(),
        );
        Ok(())
    }

    /// Run the session on `name`: install its adapter if it is missing,
    /// check the sign-in, read what it offers, then switch.
    pub(crate) fn use_acp(&mut self, name: &str) -> Result<()> {
        let id = self
            .acp_ids()
            .into_iter()
            .find(|id| id.eq_ignore_ascii_case(name) || acp::title(id).eq_ignore_ascii_case(name))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "no ACP agent named `{name}`. There are: {}, or /acp off",
                    self.acp_ids().join(", ")
                )
            })?;
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            anyhow::bail!("cannot start {} here", acp::title(&id));
        };
        let tx = self.acp_sender();
        let config = self.config.acp.clone();
        let cwd = self.workspace_dir();
        self.status = format!("getting {} ready…", acp::title(&id));
        handle.spawn(async move {
            let result = ready(&config, &id, &cwd, &tx).await;
            let _ = tx.send(AcpNews::Ready(id, result));
        });
        Ok(())
    }

    /// `/acp` alone: the agents, with where each stands, and enowx's own.
    pub(crate) fn open_acp_pick(&mut self) {
        let active = self.config.acp.active().map(str::to_owned);
        let mut items = vec![(
            "enowx".to_owned(),
            format!(
                "enowx's own model{}\n{}",
                if active.is_none() { " · in use" } else { "" },
                if self.config.model.active.is_empty() {
                    "no model chosen yet".to_owned()
                } else {
                    self.config.model.active.clone()
                }
            ),
        )];
        for id in self.acp_ids() {
            items.push((id.clone(), self.acp_card_text(&id, active.as_deref())));
        }
        self.modal_items = items;
        self.modal_cursor = active
            .and_then(|a| self.modal_items.iter().position(|(id, _)| *id == a))
            .unwrap_or(0);
        self.modal = Modal::AcpPick;
        self.detect_acp();
    }

    /// Two lines under an agent's name: where it stands, and its settings.
    fn acp_card_text(&self, id: &str, active: Option<&str>) -> String {
        let status = self
            .acp_detected
            .iter()
            .find(|d| d.id == id)
            .map_or_else(|| "checking…".to_owned(), acp::Detection::line);
        let engine = self.config.acp.engine(id);
        let model = if engine.model.is_empty() {
            "default model".to_owned()
        } else {
            self.acp_offers
                .get(id)
                .map_or_else(|| engine.model.clone(), |o| o.model_name(&engine.model))
        };
        let effort = if engine.effort.is_empty() {
            "default effort".to_owned()
        } else {
            format!("effort {}", engine.effort)
        };
        format!(
            "{status}{}\n{model} · {effort} · {}",
            if active == Some(id) { " · in use" } else { "" },
            acp::permission_label(&engine.permission)
        )
    }

    /// Look at every agent again, off the UI thread.
    fn detect_acp(&mut self) {
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let tx = self.acp_sender();
        let config = self.config.acp.clone();
        handle.spawn(async move {
            let _ = tx.send(AcpNews::Detected(acp::detect(&config).await));
        });
    }

    /// Ask `id` what it offers, off the UI thread.
    fn fetch_offer(&mut self, id: &str) {
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let tx = self.acp_sender();
        let config = self.config.acp.clone();
        let cwd = self.workspace_dir();
        let id = id.to_owned();
        handle.spawn(async move {
            let offer = acp::manager::manager().offer(&config, &id, &cwd).await;
            let _ = tx.send(AcpNews::Offer(id, offer));
        });
    }

    /// Fold in what background work found. Called from the frame loop.
    pub(crate) fn drain_acp(&mut self) {
        let mut news = Vec::new();
        if let Some(rx) = self.acp_news.as_ref() {
            while let Ok(item) = rx.try_recv() {
                news.push(item);
            }
        }
        for item in news {
            match item {
                AcpNews::Line(line) => self.status = line,
                AcpNews::Detected(found) => {
                    self.acp_detected = found;
                    self.refresh_acp_views();
                }
                AcpNews::Offer(id, Ok(offer)) => {
                    self.acp_offers.insert(id, offer);
                    self.refresh_acp_views();
                }
                AcpNews::Offer(id, Err(why)) => {
                    self.status = why.clone();
                    if self.modal == Modal::AcpEngine && self.settings.provider_id == id {
                        self.settings.acp.insert(AcpField::Status.key(), why);
                    }
                    if matches!(self.modal, Modal::AcpModels | Modal::AcpEfforts) {
                        self.modal_items = vec![(String::new(), "could not ask it".into())];
                    }
                }
                AcpNews::Ready(id, Ok(offer)) => {
                    self.acp_offers.insert(id.clone(), offer);
                    if let Err(error) = self.activate_acp(&id) {
                        self.push(TranscriptKind::Error, format!("{error:#}"));
                    }
                }
                AcpNews::Ready(id, Err(why)) => {
                    self.status = format!("{} is not ready", acp::title(&id));
                    self.push(TranscriptKind::Error, why);
                }
            }
        }
    }

    /// Redraw whatever ACP view is open with what is known now.
    fn refresh_acp_views(&mut self) {
        match self.modal {
            Modal::Acp => {
                let cursor = self.modal_cursor;
                self.open_acp();
                self.modal_cursor = cursor.min(self.modal_items.len().saturating_sub(1));
            }
            Modal::AcpPick => {
                let cursor = self.modal_cursor;
                self.open_acp_pick();
                self.modal_cursor = cursor.min(self.modal_items.len().saturating_sub(1));
            }
            Modal::AcpEngine => {
                let id = self.settings.provider_id.clone();
                if let Some(d) = self.acp_detected.iter().find(|d| d.id == id) {
                    self.settings.acp.insert(AcpField::Status.key(), d.line());
                }
                self.fill_offer(&id);
            }
            Modal::AcpModels => self.list_acp_models(),
            Modal::AcpEfforts => self.list_acp_efforts(),
            _ => {}
        }
    }

    /// The agent is ready: the session runs on it from the next message.
    fn activate_acp(&mut self, id: &str) -> Result<()> {
        let mut config = self.config.clone();
        config.acp.active = id.to_owned();
        config.save()?;
        self.adopt(config);
        self.status = format!("this session runs on {}", acp::title(id));
        self.push(
            TranscriptKind::System,
            format!(
                "This session runs on {} (ACP): the lead and every specialist. /model and \
                 /effort pick from what it offers; /acp off goes back to enowx's own model.",
                acp::title(id)
            ),
        );
        Ok(())
    }

    // ------------------------------------------------------------ model

    /// `/model` while an ACP agent runs the session.
    pub(crate) fn open_acp_models(&mut self) {
        self.modal = Modal::AcpModels;
        self.list_acp_models();
    }

    fn list_acp_models(&mut self) {
        let Some(id) = self.acp_engine() else {
            return;
        };
        let chosen = self.config.acp.engine(&id).model;
        match self.acp_offers.get(&id).cloned() {
            Some(offer) => {
                let mut items = vec![(String::new(), format!("{}'s default", acp::title(&id)))];
                items.extend(offer.models.iter().map(|c| {
                    // The id beside the name only when the name does not
                    // already say it.
                    let name = if c.name.to_lowercase().contains(&c.value.to_lowercase()) {
                        c.name.clone()
                    } else {
                        format!("{}  {}", c.name, c.value)
                    };
                    (c.value.clone(), name)
                }));
                self.modal_cursor = items.iter().position(|(v, _)| *v == chosen).unwrap_or(0);
                self.modal_items = items;
            }
            None => {
                self.modal_items = vec![(String::new(), format!("asking {}…", acp::title(&id)))];
                self.modal_cursor = 0;
                self.fetch_offer(&id);
            }
        }
    }

    /// `/effort` while an ACP agent runs the session.
    pub(crate) fn open_acp_efforts(&mut self) {
        self.modal = Modal::AcpEfforts;
        self.list_acp_efforts();
    }

    fn list_acp_efforts(&mut self) {
        let Some(id) = self.acp_engine() else {
            return;
        };
        let chosen = self.config.acp.engine(&id).effort;
        let Some(offer) = self.acp_offers.get(&id).cloned() else {
            self.modal_items = vec![(String::new(), format!("asking {}…", acp::title(&id)))];
            self.modal_cursor = 0;
            self.fetch_offer(&id);
            return;
        };
        let mut items = vec![(String::new(), "its default".to_owned())];
        items.extend(
            offer
                .efforts
                .iter()
                .map(|c| (c.value.clone(), c.name.clone())),
        );
        if offer.efforts.is_empty() {
            items = vec![(
                String::new(),
                format!("{} offers no effort setting", acp::title(&id)),
            )];
        }
        self.modal_cursor = items.iter().position(|(v, _)| *v == chosen).unwrap_or(0);
        self.modal_items = items;
    }

    /// Enter in the model or effort list.
    pub(crate) fn accept_acp_choice(&mut self) -> Result<()> {
        let Some(id) = self.acp_engine() else {
            self.modal = Modal::None;
            return Ok(());
        };
        let Some((value, name)) = self.modal_items.get(self.modal_cursor).cloned() else {
            return Ok(());
        };
        if name.starts_with("asking ") || name == "could not ask it" {
            return Ok(());
        }
        let picking_model = self.modal == Modal::AcpModels;
        let mut config = self.config.clone();
        let mut engine = config.acp.engine(&id);
        if picking_model {
            engine.model = value.clone();
        } else {
            engine.effort = value.clone();
        }
        self.save_engine(&mut config, &id, engine)?;
        self.status = format!(
            "{}: {} {}",
            acp::title(&id),
            if picking_model { "model" } else { "effort" },
            if value.is_empty() {
                "default".to_owned()
            } else {
                name
            }
        );
        // The effort follows the model, when the agent has one to choose.
        let efforts = self
            .acp_offers
            .get(&id)
            .is_some_and(|o| !o.efforts.is_empty());
        if picking_model && efforts {
            self.open_acp_efforts();
        } else {
            self.modal = Modal::None;
        }
        Ok(())
    }

    /// Save an engine's settings and apply them to its live sessions.
    fn save_engine(&mut self, config: &mut Config, id: &str, engine: EngineConfig) -> Result<()> {
        if engine == EngineConfig::default() {
            config.acp.engines.remove(id);
        } else {
            config.acp.engines.insert(id.to_owned(), engine.clone());
        }
        config.save()?;
        self.adopt(config.clone());
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let tx = self.acp_sender();
            let id = id.to_owned();
            handle.spawn(async move {
                let notes = acp::manager::manager().reconfigure(&id, &engine).await;
                if let Some(note) = notes.first() {
                    let _ = tx.send(AcpNews::Line(format!("{}: {note}", acp::title(&id))));
                }
            });
        }
        Ok(())
    }

    // ------------------------------------------------------------ settings

    /// Settings > ACP agents: a card per agent, and a row to add one.
    pub(crate) fn open_acp(&mut self) {
        let active = self.config.acp.active().map(str::to_owned);
        let mut items: Vec<(String, String)> = self
            .acp_ids()
            .into_iter()
            .map(|id| {
                let text = self.acp_card_text(&id, active.as_deref());
                (id, text)
            })
            .collect();
        items.push((
            String::new(),
            "Add a custom ACP agent\nany program that speaks ACP on stdio".into(),
        ));
        self.modal_items = items;
        self.modal_cursor = 0;
        self.modal_error.clear();
        self.modal = Modal::Acp;
        if self.acp_detected.is_empty() {
            self.detect_acp();
        }
    }

    /// Enter on a card: open it, or the form that adds a custom agent.
    pub(crate) fn accept_acp_row(&mut self) {
        let Some((id, _)) = self.modal_items.get(self.modal_cursor).cloned() else {
            return;
        };
        if id.is_empty() {
            self.open_acp_custom(None);
        } else {
            self.open_acp_engine(&id);
        }
    }

    /// Enter in `/acp`'s list: run the session on it.
    pub(crate) fn accept_acp_pick(&mut self) -> Result<()> {
        let Some((id, _)) = self.modal_items.get(self.modal_cursor).cloned() else {
            return Ok(());
        };
        self.modal = Modal::None;
        if id == "enowx" {
            self.acp_off()
        } else {
            self.use_acp(&id)
        }
    }

    fn open_acp_engine(&mut self, id: &str) {
        let engine = self.config.acp.engine(id);
        let mut draft = std::collections::BTreeMap::new();
        let status = self
            .acp_detected
            .iter()
            .find(|d| d.id == id)
            .map_or_else(|| "checking…".to_owned(), acp::Detection::line);
        draft.insert(AcpField::Status.key(), status);
        draft.insert(AcpField::Model.key(), engine.model);
        draft.insert(AcpField::Effort.key(), engine.effort);
        draft.insert(AcpField::Permission.key(), engine.permission);
        draft.insert(
            AcpField::Use.key(),
            if self.config.acp.active() == Some(id) {
                "in use: /acp off goes back to enowx's own model".into()
            } else {
                format!("Enter: save and run this session on {}", acp::title(id))
            },
        );
        draft.insert(AcpField::Edit.key(), "Enter to change".into());
        self.settings = crate::modal::SettingsDraft {
            provider_id: id.to_owned(),
            name: acp::title(id),
            acp: draft,
            ..Default::default()
        };
        self.fill_offer(id);
        self.modal_cursor = 0;
        self.field_cursor = 0;
        self.modal_error.clear();
        self.modal = Modal::AcpEngine;
        if !self.acp_offers.contains_key(id)
            && self.acp_detected.iter().any(|d| d.id == id && d.ready())
        {
            self.fetch_offer(id);
        }
    }

    /// Put what `id` offers into the open card.
    fn fill_offer(&mut self, id: &str) {
        if let Some(offer) = self.acp_offers.get(id) {
            let pairs = |list: &[acp::manager::Choice]| {
                list.iter()
                    .map(|c| (c.value.clone(), c.name.clone()))
                    .collect::<Vec<_>>()
            };
            self.settings.acp_models = pairs(&offer.models);
            self.settings.acp_efforts = pairs(&offer.efforts);
        }
    }

    /// The fields of the open ACP form.
    pub(crate) fn acp_fields(&self) -> &'static [SettingsField] {
        if self.modal == Modal::AcpCustom {
            &crate::modal::ACP_CUSTOM
        } else if self
            .config
            .acp
            .custom
            .contains_key(&self.settings.provider_id)
        {
            &crate::modal::ACP_CARD_CUSTOM
        } else {
            &crate::modal::ACP_CARD
        }
    }

    /// Enter in a card: act on a button, or save.
    pub(crate) fn save_acp_engine(&mut self) -> Result<()> {
        let id = self.settings.provider_id.clone();
        let field = self.acp_fields().get(self.modal_cursor).copied();
        if field == Some(SettingsField::Acp(AcpField::Edit)) {
            self.open_acp_custom(Some(&id));
            return Ok(());
        }
        if field == Some(SettingsField::Acp(AcpField::Status)) {
            self.acp_status_action(&id);
            return Ok(());
        }
        let get = |f: AcpField| self.settings.value(SettingsField::Acp(f)).trim().to_owned();
        let engine = EngineConfig {
            model: get(AcpField::Model),
            effort: get(AcpField::Effort),
            permission: get(AcpField::Permission),
        };
        let bypass = engine.permission == "bypass";
        let mut config = self.config.clone();
        let cursor = self.modal_cursor;
        if let Err(error) = self.save_engine(&mut config, &id, engine) {
            self.modal_error = format!("{error:#}");
            return Ok(());
        }
        if field == Some(SettingsField::Acp(AcpField::Use)) {
            self.modal = Modal::None;
            return self.use_acp(&id);
        }
        self.open_acp_engine(&id);
        self.modal_cursor = cursor;
        self.status = if bypass {
            format!(
                "saved. Careful: {} now runs anything without asking you",
                acp::title(&id)
            )
        } else {
            format!("{} saved", acp::title(&id))
        };
        Ok(())
    }

    /// Enter on Status: install the adapter when it is missing, otherwise
    /// look again and read the agent's models.
    fn acp_status_action(&mut self, id: &str) {
        let missing = self
            .acp_detected
            .iter()
            .find(|d| d.id == id)
            .is_some_and(|d| d.adapter.is_none());
        let key = AcpField::Status.key();
        let tx = self.acp_sender();
        match acp::kind(id).filter(|_| missing) {
            Some(kind) => {
                self.settings.acp.insert(key, "installing…".into());
                let config = self.config.acp.clone();
                std::thread::spawn(move || {
                    let line_tx = tx.clone();
                    let done = acp::install(kind, |line| {
                        let _ = line_tx.send(AcpNews::Line(line.chars().take(90).collect()));
                    });
                    if let Err(why) = done {
                        let _ = tx.send(AcpNews::Line(why));
                    }
                    let found = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map(|rt| rt.block_on(acp::detect(&config)))
                        .unwrap_or_default();
                    let _ = tx.send(AcpNews::Detected(found));
                });
            }
            None => {
                self.settings.acp.insert(key, "checking…".into());
                self.detect_acp();
                self.acp_offers.remove(id);
                self.fetch_offer(id);
            }
        }
    }

    /// Add a custom agent, or edit `name`.
    fn open_acp_custom(&mut self, name: Option<&str>) {
        let mut draft = std::collections::BTreeMap::new();
        if let Some(custom) = name.and_then(|n| self.config.acp.custom.get(n)) {
            let name = name.unwrap_or_default();
            draft.insert(AcpField::CustomName.key(), name.to_owned());
            draft.insert(AcpField::CustomCommand.key(), custom.command.clone());
            draft.insert(AcpField::CustomArgs.key(), custom.args.join(" "));
            draft.insert(
                AcpField::CustomEnv.key(),
                custom
                    .env
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join("; "),
            );
            draft.insert(
                AcpField::CustomPermission.key(),
                self.config.acp.engine(name).permission,
            );
        } else {
            draft.insert(AcpField::CustomPermission.key(), "ask".into());
        }
        self.settings = crate::modal::SettingsDraft {
            provider_id: name.unwrap_or_default().to_owned(),
            name: "custom".into(),
            acp: draft,
            ..Default::default()
        };
        self.modal_cursor = 0;
        self.field_cursor = 0;
        self.modal_error.clear();
        self.modal = Modal::AcpCustom;
    }

    /// Enter in the custom agent form.
    pub(crate) fn save_acp_custom(&mut self) -> Result<()> {
        let get = |f: AcpField| self.settings.value(SettingsField::Acp(f)).trim().to_owned();
        let name = get(AcpField::CustomName);
        let old = self.settings.provider_id.clone();
        if name.is_empty() {
            self.modal_error = "a custom agent needs a name".into();
            return Ok(());
        }
        if acp::kind(&name).is_some() || name == "enowx" || name == "off" || name == "native" {
            self.modal_error = format!("`{name}` is taken: pick another name");
            return Ok(());
        }
        let command = get(AcpField::CustomCommand);
        if command.is_empty() {
            self.modal_error = "a custom agent needs its command".into();
            return Ok(());
        }
        let env = get(AcpField::CustomEnv)
            .split(';')
            .filter_map(|pair| {
                let (k, v) = pair.split_once('=')?;
                Some((k.trim().to_owned(), v.trim().to_owned()))
            })
            .filter(|(k, _)| !k.is_empty())
            .collect();
        let mut config = self.config.clone();
        let mut engine = config.acp.engine(&old);
        if !old.is_empty() && old != name {
            config.acp.custom.remove(&old);
            config.acp.engines.remove(&old);
            for value in config.acp.agents.values_mut() {
                if *value == old {
                    *value = name.clone();
                }
            }
            if config.acp.active == old {
                config.acp.active = name.clone();
            }
        }
        config.acp.custom.insert(
            name.clone(),
            CustomAgent {
                command,
                args: get(AcpField::CustomArgs)
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect(),
                env,
            },
        );
        engine.permission = get(AcpField::CustomPermission);
        let bypass = engine.permission == "bypass";
        config.acp.engines.insert(name.clone(), engine);
        if let Err(error) = config.save() {
            self.modal_error = format!("{error:#}");
            return Ok(());
        }
        // A changed command takes effect on the agent's next start.
        acp::manager::manager().stop(&name);
        self.adopt(config);
        self.detect_acp();
        self.open_acp_engine(&name);
        self.status = if bypass {
            format!("{name} saved. Careful: it runs anything without asking you")
        } else {
            format!("{name} saved; /acp {name} runs the session on it")
        };
        Ok(())
    }

    /// `e` in the roster: put the selected agent on the next engine, or back
    /// on the configured model. For mixing engines; `/acp` is the usual way.
    pub(crate) fn cycle_agent_engine(&mut self) -> Result<()> {
        let Some((agent, _)) = self.modal_items.get(self.modal_cursor).cloned() else {
            return Ok(());
        };
        let mut choices = vec!["enowx".to_owned()];
        choices.extend(self.config.acp.engine_ids());
        let current = self
            .config
            .acp
            .engine_for(&agent)
            .unwrap_or("enowx")
            .to_owned();
        let here = choices.iter().position(|c| *c == current).unwrap_or(0);
        let next = choices[(here + 1) % choices.len()].clone();
        let mut config = self.config.clone();
        let follows = config.acp.active().unwrap_or("enowx").to_owned();
        if next == follows {
            config.acp.agents.remove(&agent);
        } else {
            config.acp.agents.insert(agent.clone(), next.clone());
        }
        config.save()?;
        let cursor = self.modal_cursor;
        self.adopt(config);
        self.open_agents();
        self.modal_cursor = cursor;
        let name = enowx_core::agent_def::display_name(&agent);
        self.status = if next == "enowx" {
            format!("{name} runs on enowx's own model")
        } else {
            format!("{name} runs on {}", acp::title(&next))
        };
        Ok(())
    }
}

/// Get `id` ready to run the session: its adapter, its sign-in, and what it
/// offers. Progress goes to the status line.
async fn ready(
    config: &acp::AcpConfig,
    id: &str,
    cwd: &std::path::Path,
    tx: &std::sync::mpsc::Sender<AcpNews>,
) -> std::result::Result<Offer, String> {
    let title = acp::title(id);
    let _ = tx.send(AcpNews::Line(format!("{title}: checking…")));
    let found = acp::detect(config).await;
    let mine = found.iter().find(|d| d.id == id).cloned();
    let _ = tx.send(AcpNews::Detected(found));
    let mine = mine.ok_or_else(|| format!("{title} is not known"))?;
    if mine.adapter.is_none() {
        let kind = acp::kind(id).ok_or_else(|| {
            format!("{title}'s command was not found; fix it in Settings > ACP agents")
        })?;
        if mine.node.is_none() {
            return Err(format!(
                "{title} needs Node.js 20 or newer. Install it from nodejs.org, then run /acp {id} again."
            ));
        }
        let progress = tx.clone();
        let label = title.clone();
        tokio::task::spawn_blocking(move || {
            acp::install(kind, |line| {
                let line: String = line.chars().take(80).collect();
                let _ = progress.send(AcpNews::Line(format!("{label}: {line}")));
            })
        })
        .await
        .map_err(|e| e.to_string())??;
        let _ = tx.send(AcpNews::Detected(acp::detect(config).await));
    }
    if mine.login.as_ref().is_some_and(|l| !l.signed_in) {
        return Err(format!(
            "{title} is not signed in. {} Then run /acp {id} again.",
            acp::kind(id).map_or("", |k| k.login_hint)
        ));
    }
    let _ = tx.send(AcpNews::Line(format!("{title}: starting…")));
    acp::manager::manager().offer(config, id, cwd).await
}
