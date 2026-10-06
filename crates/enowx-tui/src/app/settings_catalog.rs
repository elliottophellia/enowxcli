//! Stable, searchable rows for the Settings workspace.

use super::{pages::Page, App};
use crate::modal::{ConfKind, CONF_FIELDS, DISPLAY_FIELDS, GENERAL_FIELDS};
use crossterm::event::KeyCode;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SettingRowKind {
    Toggle {
        config_key: String,
    },
    Cycle {
        config_key: String,
        choices: Vec<(String, String)>,
    },
    Text {
        config_key: String,
        placeholder: String,
    },
    Action {
        enter: SettingAction,
        space: Option<SettingAction>,
        shortcuts: Vec<(KeyCode, SettingAction)>,
    },
    ReadOnly,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SettingAction {
    ChooseModel,
    SetEffort,
    RefreshModels,
    AddModel,
    OpenProvider(String),
    AddProvider,
    DisconnectProvider(String),
    SwitchAgent(String),
    SetAgentModel(String),
    ClearAgentModel(String),
    ToggleMcp(String),
    ShowMcpTools(String),
    ConfigureMcp(String),
    AddMcp,
    ConfigureRag,
    ToggleSkill(String),
    ReadSkill(String),
    SetTypeSafeKey,
    ChooseTeamReviewer,
    ResumeSession(String),
    ChooseTheme,
    UpdateNow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SettingRow {
    pub(crate) id: String,
    pub(crate) category: Page,
    pub(crate) label: String,
    pub(crate) description: String,
    pub(crate) value: String,
    pub(crate) kind: SettingRowKind,
}

impl App {
    /// Rebuild all visible settings from their authoritative config/discovery sources.
    pub(crate) fn settings_rows(&self) -> Vec<SettingRow> {
        use SettingAction as Act;
        use SettingRowKind as Kind;
        let mut rows = Vec::new();
        let mut push =
            |id: String, category, label: String, description: String, value: String, kind| {
                rows.push(SettingRow {
                    id,
                    category,
                    label,
                    description,
                    value,
                    kind,
                });
            };

        let conf_row = |index: usize, category: Page| {
            let (key, label, kind, placeholder) = CONF_FIELDS[index];
            let stored = self.config.get(key).unwrap_or_default();
            let value = if kind == ConfKind::Bool {
                if stored == "true" {
                    "on".to_owned()
                } else {
                    "off".to_owned()
                }
            } else if key == "agent.auto_compact_at" {
                format!(
                    "{}%",
                    stored
                        .parse::<f32>()
                        .map(|fraction| (fraction * 100.0).round() as u32)
                        .unwrap_or(0)
                )
            } else {
                stored
            };
            let desc = match key {
                "agent.preview" => "Allow agents to inspect web pages in headless Chrome.",
                "agent.lsp" => {
                    "Send written edits to the project's language server for diagnostics."
                }
                "agent.background_delegation" => {
                    "Let the main turn finish while delegated agents continue."
                }
                "agent.auto_switch" => "Allow routing to move the conversation to another agent.",
                "agent.auto_compact" => {
                    "Summarize older context automatically when the threshold is reached."
                }
                "agent.auto_compact_at" => {
                    "Context-fill fraction that triggers automatic compaction."
                }
                "agent.compact_keep_last" => {
                    "Number of recent turns kept verbatim during compaction."
                }
                "agent.max_steps" => {
                    "Maximum model calls per turn; zero means no configured limit."
                }
                "agent.shell_timeout_secs" => {
                    "Maximum time a shell command may run before it is stopped."
                }
                "agent.tiers.cheap" => "Model used for delegation work in the cheap tier.",
                "agent.tiers.balanced" => "Model used for delegation work in the balanced tier.",
                "agent.tiers.strong" => "Model used for delegation work in the strong tier.",
                "ui.show_sidebar" => "Show or hide the Chat sidebar.",
                "ui.currency" => "Currency label used when displaying model prices.",
                "ui.currency_rate" => {
                    "Multiplier converting US-dollar prices to the selected currency."
                }
                _ => "Configure this application setting.",
            }
            .to_owned();
            let row_kind = match kind {
                ConfKind::Bool => Kind::Toggle {
                    config_key: key.to_owned(),
                },
                ConfKind::Choice(options) => Kind::Cycle {
                    config_key: key.to_owned(),
                    choices: options
                        .iter()
                        .map(|value| ((*value).to_owned(), (*value).to_owned()))
                        .collect(),
                },
                ConfKind::Text => Kind::Text {
                    config_key: key.to_owned(),
                    placeholder: placeholder.to_owned(),
                },
            };
            SettingRow {
                id: key.to_owned(),
                category,
                label: label.to_owned(),
                description: desc,
                value,
                kind: row_kind,
            }
        };
        for field in GENERAL_FIELDS {
            if let crate::modal::SettingsField::Conf(i) = field {
                let row = conf_row(i, Page::General);
                push(
                    row.id,
                    row.category,
                    row.label,
                    row.description,
                    row.value,
                    row.kind,
                );
            }
        }

        let action = |enter| Kind::Action {
            enter,
            space: None,
            shortcuts: Vec::new(),
        };
        push("model:choose".into(), Page::Models, "Choose current model".into(), "Select a model from connected providers; the picker includes recent and favorite models.".into(), self.model_label(), action(Act::ChooseModel));
        push(
            "model:effort".into(),
            Page::Models,
            "Thinking effort".into(),
            "Choose the reasoning effort offered by the current model.".into(),
            if self.config.model.effort.is_empty() {
                "default".into()
            } else {
                self.config.model.effort.clone()
            },
            action(Act::SetEffort),
        );
        push(
            "model:refresh".into(),
            Page::Models,
            "Refresh model list".into(),
            "Ask connected providers to refresh their available model lists.".into(),
            String::new(),
            Kind::Action {
                enter: Act::RefreshModels,
                space: None,
                shortcuts: vec![(KeyCode::Char('r'), Act::RefreshModels)],
            },
        );
        push(
            "model:add".into(),
            Page::Models,
            "Add a model manually".into(),
            "Add a model identifier that its provider does not list.".into(),
            String::new(),
            Kind::Action {
                enter: Act::AddModel,
                space: None,
                shortcuts: vec![(KeyCode::Char('n'), Act::AddModel)],
            },
        );

        for conn in self.config.connections() {
            let state = if conn.is_connected() {
                "connected"
            } else if conn.configured {
                "configured, not connected"
            } else {
                "not configured"
            };
            let source = conn
                .key_source
                .as_ref()
                .map(|s| match s {
                    enowx_core::auth::KeySource::Stored => " · key stored".to_owned(),
                    enowx_core::auth::KeySource::Env(name) => format!(" · key from {name}"),
                    enowx_core::auth::KeySource::Session => " · session key".to_owned(),
                })
                .unwrap_or_default();
            let in_use = self
                .config
                .model
                .active
                .split_once('/')
                .is_some_and(|(id, _)| id == conn.id);
            let value = format!("{state}{source}{}", if in_use { " · in use" } else { "" });
            let shortcuts = vec![
                (KeyCode::Char('d'), Act::DisconnectProvider(conn.id.clone())),
                (KeyCode::Delete, Act::DisconnectProvider(conn.id.clone())),
            ];
            push(format!("provider:{}", conn.id), Page::Providers, conn.name.clone(), format!("Provider ID: {}. Configure its endpoint or credentials; secrets are never displayed.", conn.id), value, Kind::Action { enter: Act::OpenProvider(conn.id.clone()), space: None, shortcuts });
        }
        push(
            "provider:add".into(),
            Page::Providers,
            "Add custom provider".into(),
            "Configure an OpenAI-compatible provider endpoint and its credentials.".into(),
            String::new(),
            action(Act::AddProvider),
        );

        for agent in &self.discovery.agents {
            let tier_model = self
                .config
                .agent
                .models
                .get(&agent.name)
                .cloned()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| {
                    self.config
                        .agent
                        .tiers
                        .get(agent.tier)
                        .unwrap_or(&self.config.model.active)
                        .to_owned()
                });
            let shortcuts = vec![
                (KeyCode::Char('m'), Act::SetAgentModel(agent.name.clone())),
                (KeyCode::Char('d'), Act::ClearAgentModel(agent.name.clone())),
            ];
            let active = self.active_agent() == agent.name;
            push(
                format!("agent:{}", agent.name),
                Page::Agents,
                agent.name.clone(),
                format!(
                    "{}{}",
                    agent.description,
                    if active {
                        " This agent currently holds the session."
                    } else {
                        ""
                    }
                ),
                if tier_model.is_empty() {
                    "default model".into()
                } else {
                    tier_model
                },
                Kind::Action {
                    enter: Act::SwitchAgent(agent.name.clone()),
                    space: None,
                    shortcuts,
                },
            );
        }

        let comms = &self.config.agent.comms;
        let bool_kind = |key: &str| Kind::Toggle {
            config_key: key.to_owned(),
        };
        let team_rows = [
            (
                "agent.comms.enabled",
                "Enable agent teamwork",
                "Allow agents to coordinate through messages, a shared board, and review.",
                comms.enabled,
            ),
            (
                "agent.comms.messages",
                "Agent messages",
                "Allow agents working in parallel to message one another.",
                comms.messages,
            ),
            (
                "agent.comms.board",
                "Shared team board",
                "Allow agents in a run to post to and read a shared board.",
                comms.board,
            ),
            (
                "agent.comms.review",
                "Review delegated changes",
                "Have a reviewer check delegated work that changed files.",
                comms.review,
            ),
        ];
        for (key, label, desc, val) in team_rows
            .into_iter()
            .take(if comms.enabled { 4 } else { 1 })
        {
            push(
                key.into(),
                Page::Team,
                label.into(),
                desc.into(),
                if val { "on" } else { "off" }.into(),
                bool_kind(key),
            );
        }
        if comms.enabled {
            push(
                "agent.comms.review_rounds".into(),
                Page::Team,
                "Review rounds".into(),
                "Maximum correction rounds for delegated work review.".into(),
                comms.review_rounds.to_string(),
                Kind::Cycle {
                    config_key: "agent.comms.review_rounds".into(),
                    choices: (1..=5).map(|n| (n.to_string(), n.to_string())).collect(),
                },
            );
            push(
                "team:reviewer".into(),
                Page::Team,
                "Reviewer agent".into(),
                "Choose which discovered agent reviews delegated changes.".into(),
                comms.reviewer.clone(),
                action(Act::ChooseTeamReviewer),
            );
        }

        for row in self.mcp_rows() {
            match row {
                super::mcp_ui::McpRow::Server {
                    name,
                    transport,
                    scope,
                    enabled,
                    builtin,
                    configured,
                    target: _,
                } => {
                    let state = format!(
                        "{} · {} · {}{}",
                        if enabled { "enabled" } else { "disabled" },
                        transport_label(transport),
                        scope.label(),
                        if builtin && !configured {
                            " · setup required"
                        } else {
                            ""
                        }
                    );
                    let shortcuts = vec![
                        (KeyCode::Char('c'), Act::ConfigureMcp(name.clone())),
                        (KeyCode::Char('t'), Act::ShowMcpTools(name.clone())),
                    ];
                    push(format!("mcp:{name}"), Page::Mcp, name.clone(), format!("{state}. Configure connection details with c; credentials remain masked."), state, Kind::Action { enter: Act::ShowMcpTools(name.clone()), space: Some(Act::ToggleMcp(name.clone())), shortcuts });
                }
                super::mcp_ui::McpRow::AddNew => {}
            }
        }
        push(
            "mcp:add".into(),
            Page::Mcp,
            "Add MCP server".into(),
            "Add a server from a command or HTTP endpoint.".into(),
            String::new(),
            action(Act::AddMcp),
        );

        let builtin = enowx_core::builtin_mcp::BuiltinConfig::load().unwrap_or_default();
        let rag = builtin.rag.clone().unwrap_or_default();
        let dsn_set = self
            .config
            .auth
            .key(&enowx_core::builtin_mcp::rag_dsn_id(), &[])
            .is_some();
        let rag_value = format!(
            "{} · {} · {} · vector database {}",
            if builtin.is_installed("rag") {
                "configured"
            } else {
                "off"
            },
            rag.provider,
            rag.model(),
            if dsn_set {
                "configured"
            } else {
                "not configured"
            }
        );
        push("rag:configure".into(), Page::Rag, "Configure code search (RAG)".into(), "Set up code indexing, its embedding provider, and vector database; changes save together after validation.".into(), rag_value, action(Act::ConfigureRag));

        for skill in self.skill_rows() {
            let scope = skill.scope.label();
            let details = if skill.parts > 0 {
                format!("{scope} skill · {} built-in parts", skill.parts)
            } else {
                format!("{scope} skill")
            };
            let kind = Kind::Action {
                enter: Act::ReadSkill(skill.name.clone()),
                space: Some(Act::ToggleSkill(skill.name.clone())),
                shortcuts: Vec::new(),
            };
            push(
                format!("skill:{scope}:{}", skill.name),
                Page::Skills,
                skill.name.clone(),
                if skill.description.is_empty() {
                    details
                } else {
                    format!("{} · {details}", skill.description)
                },
                if skill.enabled { "enabled" } else { "disabled" }.into(),
                kind,
            );
        }

        for (key, label, desc, value) in [
            (
                "typesafe.gate_tool_results",
                "Gate tool results",
                "Use TypeSafe judgments to decide whether tool output stays in context.",
                self.config.typesafe.gate_tool_results,
            ),
            (
                "typesafe.rank_compaction",
                "Rank compaction turns",
                "Use TypeSafe judgments to prioritize live turns during compaction.",
                self.config.typesafe.rank_compaction,
            ),
        ] {
            push(
                key.into(),
                Page::TypeSafe,
                label.into(),
                desc.into(),
                if value { "on" } else { "off" }.into(),
                bool_kind(key),
            );
        }
        let key_state = if self.config.typesafe.active() {
            "set · masked"
        } else {
            "not set"
        };
        push(
            "typesafe:key".into(),
            Page::TypeSafe,
            "TypeSafe API key".into(),
            "Set or replace the masked key; the credential itself is never displayed.".into(),
            key_state.into(),
            action(Act::SetTypeSafeKey),
        );
        push(
            "typesafe:session".into(),
            Page::TypeSafe,
            "This session".into(),
            "TypeSafe context-trimming summary for the current session.".into(),
            format!(
                "{} tool results · {} characters saved",
                self.trimmed_count, self.trimmed_saved
            ),
            Kind::ReadOnly,
        );

        let workspace = std::fs::canonicalize(self.config.workspace())
            .unwrap_or_else(|_| self.config.workspace());
        if let Ok(sessions) = self.store.list(200) {
            for session in sessions {
                let owned = self
                    .store
                    .load(&session.id)
                    .map(|s| s.workspace == workspace)
                    .unwrap_or(false);
                if owned {
                    let title = if session.title.is_empty() {
                        "Untitled"
                    } else {
                        &session.title
                    };
                    let label = format!("{title} · {}", session.role.label());
                    push(
                        format!("session:{}", session.id),
                        Page::Sessions,
                        label,
                        format!(
                            "Saved session with {} messages; Enter resumes it.",
                            session.message_count
                        ),
                        format!(
                            "{} · {} messages",
                            session.role.label(),
                            session.message_count
                        ),
                        action(Act::ResumeSession(session.id)),
                    );
                }
            }
        }

        for field in DISPLAY_FIELDS {
            if let crate::modal::SettingsField::Conf(i) = field {
                let row = conf_row(i, Page::Display);
                push(
                    row.id,
                    row.category,
                    row.label,
                    row.description,
                    row.value,
                    row.kind,
                );
            }
        }
        push(
            "theme:choose".into(),
            Page::Theme,
            "Choose theme".into(),
            "Preview themes with the arrow keys; Enter saves and Escape restores the original."
                .into(),
            self.config.ui.theme.clone(),
            action(Act::ChooseTheme),
        );

        for (key, label, desc, value) in [
            (
                "update.check_on_start",
                "Check for updates at startup",
                "Check for a newer release at startup; ENX_NO_UPDATE_CHECK disables checks.",
                self.config.update.check_on_start,
            ),
            (
                "update.auto_install",
                "Install updates automatically",
                "Install a discovered update; the new version takes effect after restarting enx.",
                self.config.update.auto_install,
            ),
        ] {
            push(
                key.into(),
                Page::Updates,
                label.into(),
                desc.into(),
                if value { "on" } else { "off" }.into(),
                bool_kind(key),
            );
        }
        push(
            "update:install".into(),
            Page::Updates,
            "Check and install now".into(),
            "Run the existing update check and install action.".into(),
            format!("{:?}", self.update_shown),
            action(Act::UpdateNow),
        );
        rows
    }
}

fn transport_label(transport: enowx_core::discovery::McpTransport) -> &'static str {
    match transport {
        enowx_core::discovery::McpTransport::Stdio => "stdio",
        enowx_core::discovery::McpTransport::Http => "HTTP",
        enowx_core::discovery::McpTransport::Sse => "SSE",
    }
}
