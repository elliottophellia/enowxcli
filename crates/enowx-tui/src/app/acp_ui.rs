//! Settings > ACP agents: Claude Code, Codex, Gemini CLI and one custom
//! agent as engines for enowx's agents. Each built-in shows what is installed
//! and signed in (Enter installs its adapter into ~/.enx/acp, or checks
//! again), and takes a model, a thinking effort and how its permission
//! prompts are answered. Which agent runs on which engine is set in the
//! agent roster with `e`. Saved to `config.toml` under `[acp]`.

use super::*;
use crate::modal::{AcpField, SettingsField};
use enowx_core::acp::{self, CustomAgent, EngineConfig, KINDS};

/// What the background work reports.
pub(crate) enum AcpNews {
    /// A line for one engine's status row.
    Line(String, String),
    /// Every engine looked at again.
    Detected(Vec<acp::Detection>),
}

impl App {
    /// Open the section, filled from the config, and look at what is
    /// installed in the background.
    pub(crate) fn open_acp(&mut self) {
        let mut draft = std::collections::BTreeMap::new();
        for (i, kind) in KINDS.iter().enumerate() {
            let engine = self.config.acp.engine(kind.id);
            draft.insert(AcpField::Status(i).key(), "checking…".to_owned());
            draft.insert(AcpField::Model(i).key(), engine.model);
            draft.insert(AcpField::Effort(i).key(), engine.effort);
            draft.insert(AcpField::Permission(i).key(), engine.permission);
        }
        if let Some((name, custom)) = self.config.acp.custom.iter().next() {
            draft.insert(AcpField::CustomName.key(), name.clone());
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
            provider_id: "acp".into(),
            name: "acp".into(),
            acp: draft,
            ..Default::default()
        };
        self.modal_cursor = 0;
        self.field_cursor = 0;
        self.modal_error.clear();
        self.modal = Modal::Acp;
        self.detect_acp();
    }

    /// Look at every engine again, off the UI thread.
    fn detect_acp(&mut self) {
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let (tx, rx) = std::sync::mpsc::channel();
        self.acp_news = Some(rx);
        let config = self.config.acp.clone();
        handle.spawn(async move {
            let found = acp::detect(&config).await;
            let _ = tx.send(AcpNews::Detected(found));
        });
    }

    /// Enter on an engine's status row: install its adapter when it is
    /// missing, otherwise look again.
    fn acp_status_action(&mut self, index: usize) {
        let Some(kind) = KINDS.get(index) else {
            return;
        };
        let installed = self
            .settings
            .value(SettingsField::Acp(AcpField::Status(index)))
            .starts_with("adapter");
        if installed {
            self.detect_acp();
            self.settings
                .acp
                .insert(AcpField::Status(index).key(), "checking…".into());
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        self.acp_news = Some(rx);
        let key = AcpField::Status(index).key();
        self.settings.acp.insert(key.clone(), "installing…".into());
        let config = self.config.acp.clone();
        std::thread::spawn(move || {
            let line_tx = tx.clone();
            let line_key = key.clone();
            let result = acp::install(kind, |line| {
                let _ = line_tx.send(AcpNews::Line(
                    line_key.clone(),
                    line.chars().take(90).collect(),
                ));
            });
            if let Err(why) = result {
                let _ = tx.send(AcpNews::Line(key, why));
                return;
            }
            let found = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map(|rt| rt.block_on(acp::detect(&config)))
                .unwrap_or_default();
            let _ = tx.send(AcpNews::Detected(found));
        });
    }

    /// Fold in what detection and installs found. Called from the frame
    /// loop.
    pub(crate) fn drain_acp(&mut self) {
        let Some(rx) = self.acp_news.as_ref() else {
            return;
        };
        let mut news = Vec::new();
        let mut ended = false;
        loop {
            match rx.try_recv() {
                Ok(item) => news.push(item),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    ended = true;
                    break;
                }
            }
        }
        if ended {
            self.acp_news = None;
        }
        if self.modal != Modal::Acp {
            return;
        }
        for item in news {
            match item {
                AcpNews::Line(key, line) => {
                    self.settings.acp.insert(key, line);
                }
                AcpNews::Detected(found) => {
                    for (i, kind) in KINDS.iter().enumerate() {
                        if let Some(d) = found.iter().find(|d| d.id == kind.id) {
                            self.settings
                                .acp
                                .insert(AcpField::Status(i).key(), d.line());
                        }
                    }
                }
            }
        }
    }

    /// Enter in the section: act on a status row, or save.
    pub(crate) fn save_acp(&mut self) -> Result<()> {
        if let Some(SettingsField::Acp(AcpField::Status(i))) =
            crate::modal::ACP_FIELDS.get(self.modal_cursor).copied()
        {
            self.acp_status_action(i);
            return Ok(());
        }
        let get = |f: AcpField| self.settings.value(SettingsField::Acp(f)).trim().to_owned();
        let mut config = self.config.clone();
        let mut bypass = Vec::new();
        for (i, kind) in KINDS.iter().enumerate() {
            let engine = EngineConfig {
                model: get(AcpField::Model(i)),
                effort: get(AcpField::Effort(i)),
                permission: get(AcpField::Permission(i)),
            };
            if engine.permission == "bypass" {
                bypass.push(kind.title.to_owned());
            }
            if engine == EngineConfig::default() {
                config.acp.engines.remove(kind.id);
            } else {
                config.acp.engines.insert(kind.id.to_owned(), engine);
            }
        }
        let name = get(AcpField::CustomName);
        if KINDS.iter().any(|k| k.id == name) || name == "enowx" {
            self.modal_error = format!("`{name}` is taken by a built-in engine: pick another name");
            return Ok(());
        }
        // The form edits one custom agent; others in the file are kept.
        let editing = self.config.acp.custom.keys().next().cloned();
        if let Some(old) = &editing {
            if *old != name {
                config.acp.custom.remove(old);
                config.acp.engines.remove(old);
                for engine in config.acp.agents.values_mut() {
                    if engine == old && !name.is_empty() {
                        *engine = name.clone();
                    }
                }
                config.acp.agents.retain(|_, engine| engine != old);
            }
        }
        if !name.is_empty() {
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
            let permission = get(AcpField::CustomPermission);
            if permission == "bypass" {
                bypass.push(name.clone());
            }
            config.acp.engines.insert(
                name.clone(),
                EngineConfig {
                    permission,
                    ..EngineConfig::default()
                },
            );
        }
        if let Err(error) = config.save() {
            self.modal_error = format!("{error:#}");
            return Ok(());
        }
        // Running engines pick up new settings on their next start.
        for id in config.acp.engine_ids() {
            acp::manager::manager().stop(&id);
        }
        let cursor = self.modal_cursor;
        let shown = self.settings.acp.clone();
        self.adopt(config);
        self.modal = Modal::Acp;
        self.settings.acp = shown;
        self.modal_cursor = cursor;
        self.status = if bypass.is_empty() {
            "ACP agents saved; put an agent on one with /agent, then e".into()
        } else {
            format!(
                "saved. Careful: {} now run anything without asking you",
                bypass.join(", ")
            )
        };
        Ok(())
    }

    /// `e` in the roster: put the selected agent on the next engine, or back
    /// on the configured model.
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
        if next == "enowx" {
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
            format!("{name} runs on the configured model")
        } else {
            format!(
                "{name} runs on {} (set it up in Settings > ACP agents)",
                acp::title(&next)
            )
        };
        Ok(())
    }
}
