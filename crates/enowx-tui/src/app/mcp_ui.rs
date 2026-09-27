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
                target: s.command_or_url.clone(),
            })
            .collect();
        rows.push(McpRow::AddNew);
        rows
    }

    /// Tab on a server row: flip enabled via the overlay file and refresh.
    pub(crate) fn toggle_selected_mcp(&mut self) -> Result<()> {
        let rows = self.mcp_rows();
        let Some(McpRow::Server { name, enabled, .. }) = rows.get(self.modal_cursor).cloned()
        else {
            return Ok(());
        };
        persist::set_mcp_enabled(&name, !enabled)?;
        self.adopt(self.config.clone());
        self.status = if enabled {
            format!("mcp disabled: {name}")
        } else {
            format!("mcp enabled: {name}")
        };
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

