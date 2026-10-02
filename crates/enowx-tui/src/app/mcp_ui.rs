//! `/mcp` popup logic: list, search, toggle, add-new, and the small helpers
//! backing the two MCP modals.

use anyhow::Result;
use enowx_core::discovery::{McpServer, McpTransport, SkillScope};
use enowx_core::persist;

use crate::modal::{Modal, MCP_FORM_FIELDS};

use super::App;

/// One row in the MCP popup, plus the sentinel "+ Add" row.
#[derive(Clone)]
pub(crate) enum McpRow {
    Server {
        name: String,
        transport: McpTransport,
        scope: SkillScope,
        enabled: bool,
        /// A server enx serves itself (coolify, dokploy, vps).
        builtin: bool,
        /// Whether a built-in server has its credentials yet.
        configured: bool,
        /// What the server runs or where it lives: the one thing that tells
        /// two servers apart once the name is not enough.
        target: String,
    },
    AddNew,
}

impl App {
    pub(crate) fn open_mcp(&mut self) {
        self.modal = Modal::Mcp;
        self.modal_cursor = 0;
        self.modal_search.clear();
        self.modal_error.clear();
    }

    pub(crate) fn open_mcp_form(&mut self) {
        self.modal = Modal::McpForm;
        self.mcp_draft = crate::modal::McpDraft::default();
        self.mcp_field = 0;
        self.modal_error.clear();
    }

    /// Open the add/edit form prefilled with an existing server.
    ///
    /// Entries discovered from another tool's config (`.claude/`, `.cursor/`)
    /// are shown here too. Saving writes to OUR file rather than editing
    /// theirs: rewriting a file the user maintains for a different tool would
    /// be a surprise, and our entry takes precedence anyway. The notice says
    /// so, since otherwise the original would appear not to have changed.
    pub(crate) fn open_mcp_editor(&mut self, name: &str) {
        let Some(server) = self
            .discovery
            .mcp_servers
            .iter()
            .find(|s| s.name == name)
            .cloned()
        else {
            return;
        };
        self.mcp_draft = crate::modal::McpDraft {
            name: server.name.clone(),
            command: server.command_or_url.clone(),
            args: server.args.join(", "),
            env: server
                .env
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("\n"),
            transport: server.transport,
        };
        self.mcp_field = 0;
        self.modal_error.clear();
        self.modal = Modal::McpForm;
        self.status = if server.source == enowx_core::discovery::user_mcp_path() {
            format!("editing {name}")
        } else {
            format!("editing {name} — saving copies it into your own config",)
        };
    }

    pub(crate) fn mcp_rows(&self) -> Vec<McpRow> {
        let needle = self.modal_search.trim().to_ascii_lowercase();
        let mut rows: Vec<McpRow> = self
            .discovery
            .mcp_servers
            .iter()
            .filter(|s| needle.is_empty() || s.name.to_ascii_lowercase().contains(&needle))
            .map(|s: &McpServer| McpRow::Server {
                name: s.name.clone(),
                transport: s.transport,
                scope: s.scope,
                enabled: s.enabled,
                builtin: s.builtin,
                configured: s.configured,
                target: if s.builtin {
                    enowx_core::builtin_mcp::summary(&s.name).to_string()
                } else {
                    s.command_or_url.clone()
                },
            })
            .collect();
        // The user's own servers first, then the ones enx serves itself, so
        // what they configured is not pushed down by what ships with enx.
        rows.sort_by_key(|row| matches!(row, McpRow::Server { builtin: true, .. }));
        rows.push(McpRow::AddNew);
        rows
    }

    /// Tab on a server row: flip enabled via the overlay file and refresh.
    pub(crate) fn toggle_selected_mcp(&mut self) -> Result<()> {
        let rows = self.mcp_rows();
        let Some(McpRow::Server {
            name,
            enabled,
            builtin,
            configured,
            ..
        }) = rows.get(self.modal_cursor).cloned()
        else {
            return Ok(());
        };
        // A built-in server with no credentials cannot be turned on: offer to
        // fill them in instead of enabling something that would only error.
        if builtin && !configured && !enabled {
            self.open_builtin_mcp(&name);
            return Ok(());
        }
        persist::set_mcp_enabled(&name, !enabled)?;
        self.adopt(self.config.clone());
        self.status = if enabled {
            format!("mcp off: {name}")
        } else {
            format!("mcp on: {name}")
        };
        Ok(())
    }

    /// `c` on a built-in server row: open the config form, prefilled with what
    /// is already set (the token or password is never shown).
    pub(crate) fn config_selected_mcp(&mut self) {
        let rows = self.mcp_rows();
        if let Some(McpRow::Server { name, builtin, .. }) = rows.get(self.modal_cursor).cloned() {
            if builtin {
                self.open_builtin_mcp(&name);
            } else {
                self.status = format!("{name} is not a built-in server; press Enter to edit it");
            }
        }
    }

    fn open_builtin_mcp(&mut self, name: &str) {
        use enowx_core::builtin_mcp::BuiltinConfig;
        // RAG has a section of its own, with more to set than a form here.
        if name == "rag" {
            let _ = self.open_page(crate::app::pages::Page::Rag);
            return;
        }
        let config = BuiltinConfig::load().unwrap_or_default();
        let mut draft = crate::modal::SettingsDraft {
            provider_id: name.to_owned(),
            ..Default::default()
        };
        match name {
            "coolify" => {
                if let Some(e) = config.coolify {
                    draft.base_url = e.base_url;
                }
            }
            "dokploy" => {
                if let Some(e) = config.dokploy {
                    draft.base_url = e.base_url;
                }
            }
            "vps" => {
                // One VPS at a time here: the first, or a fresh one.
                if let Some((vps_name, host)) = config.vps.iter().next() {
                    draft.name = vps_name.clone();
                    draft.base_url = host.host.clone();
                    draft.models_url = host.user.clone();
                    draft.context_window = host.port.to_string();
                    draft.key_file = host.key_path.clone().unwrap_or_default();
                }
            }
            _ => {}
        }
        self.settings = draft;
        self.modal_cursor = 0;
        self.modal_error.clear();
        self.modal = crate::modal::Modal::BuiltinMcp;
    }

    /// Save a built-in server's credentials and turn it on, then reload MCP in
    /// place: the running session keeps going, the server starts in the
    /// background. No restart.
    pub(crate) fn save_builtin_mcp(&mut self) -> Result<()> {
        use enowx_core::auth::Auth;
        use enowx_core::builtin_mcp::{secret_id, vps, BuiltinConfig, Endpoint};
        let name = self.settings.provider_id.clone();
        let mut config = BuiltinConfig::load().unwrap_or_default();
        let mut auth = Auth::load()?;
        let token = self.settings.api_key.trim().to_owned();
        match name.as_str() {
            "coolify" | "dokploy" => {
                let url = self
                    .settings
                    .base_url
                    .trim()
                    .trim_end_matches('/')
                    .to_owned();
                if !(url.starts_with("http://") || url.starts_with("https://")) {
                    self.modal_error = "the URL must start with http:// or https://".into();
                    return Ok(());
                }
                // Keep an existing token when the field is left blank.
                let had = auth.key(&secret_id(&name), &[]).is_some();
                if token.is_empty() && !had {
                    self.modal_error = "an API token is required".into();
                    return Ok(());
                }
                let endpoint = Some(Endpoint { base_url: url });
                if name == "coolify" {
                    config.coolify = endpoint;
                } else {
                    config.dokploy = endpoint;
                }
                if !token.is_empty() {
                    auth.store(&secret_id(&name), &token)?;
                }
            }
            "rag" => {
                use enowx_core::builtin_mcp::rag_dsn_id;
                let dsn = self.settings.dsn.trim().to_owned();
                let had_dsn = auth.key(&rag_dsn_id(), &[]).is_some();
                let had_key = auth.key(&secret_id("rag"), &[]).is_some();
                if dsn.is_empty() && !had_dsn {
                    self.modal_error = "a database connection string is required".into();
                    return Ok(());
                }
                if !dsn.is_empty()
                    && !dsn.starts_with("postgres://")
                    && !dsn.starts_with("postgresql://")
                {
                    self.modal_error =
                        "the database must be a postgres:// connection string".into();
                    return Ok(());
                }
                if token.is_empty() && !had_key {
                    self.modal_error = "a Voyage AI API key is required".into();
                    return Ok(());
                }
                if !dsn.is_empty() {
                    auth.store(&rag_dsn_id(), &dsn)?;
                }
                if !token.is_empty() {
                    auth.store(&secret_id("rag"), &token)?;
                }
                config.rag = Some(config.rag.take().unwrap_or_default());
            }
            "vps" => {
                let vps_name = self.settings.name.trim().to_owned();
                let host = self.settings.base_url.trim().to_owned();
                let user = self.settings.models_url.trim().to_owned();
                if !vps::valid_name(&vps_name) || host.is_empty() {
                    self.modal_error = "a name and a host are required".into();
                    return Ok(());
                }
                let port: u16 = self.settings.context_window.trim().parse().unwrap_or(22);
                // A key file is checked now: one that is encrypted needs a
                // passphrase that opens it.
                let key_file = self.settings.key_file.trim().to_owned();
                let passphrase = self.settings.passphrase.clone();
                if !key_file.is_empty() {
                    let path = match key_file.strip_prefix("~/") {
                        Some(rest) => std::path::PathBuf::from(
                            std::env::var("HOME")
                                .or_else(|_| std::env::var("USERPROFILE"))
                                .unwrap_or_default(),
                        )
                        .join(rest),
                        None => std::path::PathBuf::from(&key_file),
                    };
                    match vps::key_is_encrypted(&path) {
                        Err(error) => {
                            self.modal_error = format!("the key file: {error:#}");
                            return Ok(());
                        }
                        Ok(false) => {
                            auth.forget(&vps::passphrase_id(&vps_name))?;
                        }
                        Ok(true) => {
                            let had = auth.key(&vps::passphrase_id(&vps_name), &[]).is_some();
                            if passphrase.is_empty() && !had {
                                self.modal_error =
                                    "that key is encrypted: enter its passphrase".into();
                                return Ok(());
                            }
                            if !passphrase.is_empty() {
                                if !vps::key_opens(&path, &passphrase) {
                                    self.modal_error =
                                        "that passphrase does not open the key".into();
                                    return Ok(());
                                }
                                auth.store(&vps::passphrase_id(&vps_name), &passphrase)?;
                            }
                        }
                    }
                }
                // No password is fine: ssh-agent and ~/.ssh keys are tried.
                if !token.is_empty() {
                    auth.store(&vps::password_id(&vps_name), &token)?;
                }
                config.vps.insert(
                    vps_name,
                    vps::Host {
                        host,
                        port,
                        user,
                        key_path: Some(key_file).filter(|k| !k.is_empty()),
                    },
                );
            }
            _ => {}
        }
        config.save()?;
        persist::set_mcp_enabled(&name, true)?;
        self.modal = crate::modal::Modal::Mcp;
        self.modal_error.clear();
        self.adopt(self.config.clone());
        self.status = format!("{name} set up and turned on");
        Ok(())
    }

    /// Enter: on server row show its proxied tools in the transcript, on the
    /// add-new row open the form modal.
    pub(crate) fn accept_mcp_row(&mut self) -> Result<()> {
        let rows = self.mcp_rows();
        let Some(row) = rows.get(self.modal_cursor).cloned() else {
            return Ok(());
        };
        match row {
            McpRow::AddNew => {
                self.open_mcp_form();
            }
            McpRow::Server { name, .. } => {
                // Enter opens the entry for editing. It used to dump the
                // server's tool list into the transcript, which is a thing
                // you read once and then have to scroll past — and it left no
                // way to correct a command or an env var short of editing
                // JSON by hand.
                self.open_mcp_editor(&name);
            }
        }
        Ok(())
    }

    /// Print the highlighted server's tools into the transcript.
    ///
    /// This is what Enter used to do; it is worth keeping, just not as the
    /// primary action on a row you are more often trying to correct.
    pub(crate) fn show_mcp_tools(&mut self) {
        let rows = self.mcp_rows();
        let Some(McpRow::Server { name, .. }) = rows.get(self.modal_cursor).cloned() else {
            return;
        };
        let text = self.describe_mcp_tools(&name);
        self.push(crate::session::TranscriptKind::Notice, text);
        self.modal = Modal::None;
    }

    fn describe_mcp_tools(&self, server: &str) -> String {
        let mut lines = vec![format!("# MCP server `{server}`")];
        // Snapshot of proxied tools currently in the registry.
        let tools = self.agent.mcp_tools(server);
        if tools.is_empty() {
            lines.push("No tools reported (server may be disabled or still starting).".into());
        } else {
            for t in tools {
                lines.push(format!("- `{}` — {}", t.name, t.description));
            }
        }
        lines.join("\n")
    }

    // ---------- Add-MCP form ----------

    pub(crate) fn mcp_form_next(&mut self) {
        self.mcp_field = (self.mcp_field + 1) % MCP_FORM_FIELDS.len();
    }
    pub(crate) fn mcp_form_prev(&mut self) {
        self.mcp_field = (self.mcp_field + MCP_FORM_FIELDS.len() - 1) % MCP_FORM_FIELDS.len();
    }

    /// Value slot the form is currently editing.
    pub(crate) fn mcp_field_mut(&mut self) -> &mut String {
        use crate::modal::McpFormField as F;
        match MCP_FORM_FIELDS[self.mcp_field] {
            F::Name => &mut self.mcp_draft.name,
            F::Command => &mut self.mcp_draft.command,
            F::Args => &mut self.mcp_draft.args,
            F::Env => &mut self.mcp_draft.env,
            // Transport is edited as a string; we normalize on save.
            F::Transport => &mut self.mcp_draft.args, // placeholder, transport uses separate handler
        }
    }

    /// Cycle transport with Space/Enter on the transport row.
    pub(crate) fn cycle_transport(&mut self) {
        self.mcp_draft.transport = match self.mcp_draft.transport {
            McpTransport::Stdio => McpTransport::Http,
            McpTransport::Http => McpTransport::Sse,
            McpTransport::Sse => McpTransport::Stdio,
        };
    }

    pub(crate) fn submit_mcp_form(&mut self) -> Result<()> {
        match persist::add_user_mcp(&self.mcp_draft, &self.discovery.mcp_servers) {
            Ok(path) => {
                self.status = format!("added mcp: {}", path.display());
                self.modal = Modal::None;
                self.modal_error.clear();
                self.adopt(self.config.clone());
                Ok(())
            }
            Err(err) => {
                self.modal_error = err.message.clone();
                Ok(())
            }
        }
    }

    pub(crate) fn mcp_form_key(&mut self, key: crossterm::event::KeyEvent) -> anyhow::Result<()> {
        use crate::modal::McpFormField as F;
        use crossterm::event::{KeyCode, KeyModifiers};
        match key.code {
            KeyCode::Esc => {
                self.modal = Modal::None;
                self.modal_error.clear();
                Ok(())
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.modal = Modal::None;
                self.modal_error.clear();
                Ok(())
            }
            KeyCode::Tab | KeyCode::Down => {
                self.mcp_form_next();
                Ok(())
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.mcp_form_prev();
                Ok(())
            }
            KeyCode::Enter => self.submit_mcp_form(),
            KeyCode::Char(' ') if crate::modal::MCP_FORM_FIELDS[self.mcp_field] == F::Transport => {
                self.cycle_transport();
                Ok(())
            }
            KeyCode::Backspace => {
                if crate::modal::MCP_FORM_FIELDS[self.mcp_field] != F::Transport {
                    self.mcp_field_mut().pop();
                }
                Ok(())
            }
            KeyCode::Char(c)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                if crate::modal::MCP_FORM_FIELDS[self.mcp_field] != F::Transport {
                    self.mcp_field_mut().push(c);
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}
