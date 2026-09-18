use crate::{
    commands::COMMANDS,
    modal::{Modal, SettingsDraft, SettingsField, SETTINGS_FIELDS},
    session::{Activity, TranscriptBlock, TranscriptKind, SPINNER},
    theme::{Theme, THEMES},
};
use anyhow::Result;
mod attach;
use enowx_core::{
    discovery::Discovery,
    provider::{ModelInfo, Provider},
    Agent, Config, Event, MessageRole, Role, RunRequest, SessionStore, PROVIDER_PRESETS, ROLES,
};
use ratatui::layout::Rect;
use std::{collections::HashMap, sync::Arc, time::Instant};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
mod actions;
mod connection;
mod events;
mod keys;
pub(crate) mod mcp_ui;
mod navigation;
mod sessions;
mod settings_keys;
mod skills;

pub(crate) struct App {
    pub(crate) agent: Arc<Agent>,
    pub(crate) config: Config,
    pub(crate) store: SessionStore,
    pub(crate) blocks: Vec<TranscriptBlock>,
    pub(crate) input: String,
    pub(crate) cursor: usize,
    pub(crate) session_id: Option<String>,
    pub(crate) title: String,
    pub(crate) role: Role,
    /// Which agent currently holds the session. Empty means "whatever the
    /// legacy role maps to" — the loop has not been migrated yet, so this is
    /// resolved through `App::active_agent` rather than being assumed set.
    pub(crate) agent_name: String,
    /// Handovers to mark in the transcript, each pinned to the block it
    /// precedes rather than to `AgentSwitch::at_turn`. A turn expands into
    /// several blocks — reasoning, prose, one per tool call — so a turn index
    /// alone cannot say where the rule belongs once the transcript is built.
    pub(crate) switch_markers: Vec<(usize, enowx_core::session::AgentSwitch)>,
    pub(crate) busy: bool,
    pub(crate) cancel: Option<CancellationToken>,
    pub(crate) events: Option<mpsc::Receiver<Event>>,
    pub(crate) task: Option<tokio::task::JoinHandle<()>>,
    pub(crate) show_reasoning: bool,
    pub(crate) show_tool_output: bool,
    /// Per-block override of `show_tool_output`. Keyed by the tool call id so
    /// re-renders keep the same open/closed state after a scroll or resize.
    pub(crate) tool_expanded: std::collections::HashMap<String, bool>,
    /// (tool_id, line_index_in_wrapped_transcript) captured by the renderer
    /// each frame so a click on the header line can toggle expansion.
    pub(crate) tool_header_markers: Vec<(String, usize)>,
    /// Screen rects (already scroll-adjusted) of tool header lines this frame,
    /// so a mouse click can find which tool block to toggle.
    pub(crate) tool_header_rects: Vec<(Rect, String)>,
    /// Transcript body area, used to route wheel events to scroll only when
    /// the pointer is inside it.
    pub(crate) transcript_area: Option<Rect>,
    /// Composer field rect + viewport offset in visual rows, so a mouse
    /// click on the field can be translated back into a byte offset in
    /// `input`.
    pub(crate) composer_field: Option<Rect>,
    pub(crate) composer_offset: usize,
    pub(crate) composer_width: usize,
    /// Mouse drag selection over the transcript. Filled on Down(Left) inside
    /// the transcript area, extended on Drag, extracted+cleared on Up.
    pub(crate) selection: Option<TextSelection>,
    /// Snapshot of every wrapped visual line rendered this frame with the
    /// screen row it occupies. Populated by `draw_transcript` so a drag
    /// selection can extract exactly what the user saw.
    pub(crate) wrapped_snapshot: Vec<(u16, String)>,
    pub(crate) scroll: u16,
    pub(crate) max_scroll: u16,
    pub(crate) auto_scroll: bool,
    pub(crate) should_quit: bool,
    pub(crate) status: String,
    pub(crate) tokens_in: u32,
    pub(crate) tokens_out: u32,
    pub(crate) modal: Modal,
    pub(crate) modal_cursor: usize,
    pub(crate) modal_items: Vec<(String, String)>,
    pub(crate) palette_cursor: usize,
    pub(crate) settings: SettingsDraft,
    pub(crate) field_cursor: usize,
    pub(crate) discovering_models: bool,
    pub(crate) model_events: Option<mpsc::Receiver<Result<Vec<ModelInfo>, String>>>,
    pub(crate) models: Vec<ModelInfo>,
    pub(crate) modal_error: String,
    pub(crate) activity: Activity,
    pub(crate) activity_since: Instant,
    pub(crate) turn_started: Instant,
    pub(crate) theme: Theme,
    pub(crate) sidebar_tab: usize,
    pub(crate) sidebar_page: usize,
    pub(crate) sidebar_pages: usize,
    pub(crate) workspace: std::path::PathBuf,
    pub(crate) show_sidebar: bool,
    pub(crate) attachments: Vec<enowx_core::message::Attachment>,
    pub(crate) attach_error: Option<String>,
    pub(crate) tool_counts: HashMap<String, usize>,
    pub(crate) context_tokens: u32,
    pub(crate) context_window: u32,
    pub(crate) sidebar_area: Option<Rect>,
    pub(crate) sidebar_tabs: Vec<(Rect, usize)>,
    pub(crate) modal_rows: Vec<(Rect, usize)>,
    pub(crate) sidebar_pages_area: Option<Rect>,
    pub(crate) discovery: Arc<Discovery>,
    /// Search query typed in `/skills` and `/mcp` popups.
    pub(crate) modal_search: String,
    /// Draft for the Add-MCP form popup.
    pub(crate) mcp_draft: crate::modal::McpDraft,
    pub(crate) mcp_field: usize,
    /// Popup click regions: `(row_rect, mark_rect, row_index)`. Filled by the
    /// popup renderer each frame so a click can hit either the toggle mark
    /// (Tab equivalent) or the row body (Enter equivalent).
    pub(crate) popup_rows: Vec<(Rect, Rect, usize)>,
    /// Body rect of the currently open popup, used to route ScrollUp/Down
    /// events to the popup cursor instead of the transcript.
    pub(crate) popup_body: Option<Rect>,
    /// Rects for the MCP form fields, so the user can click a row to focus it.
    pub(crate) mcp_field_rows: Vec<(Rect, usize)>,
    /// Timestamp of the last accepted wheel step. Terminals emit wheel events
    /// at burst rates (dozens per second on macOS trackpads); we throttle so
    /// one physical scroll = one selector step.
    pub(crate) last_wheel: Option<Instant>,
    /// Selection state for the QuitConfirm popup. `true` = Yes highlighted,
    /// `false` = No. Defaults to `false` so hitting Enter accidentally does
    /// not quit the session.
    pub(crate) quit_confirm_yes: bool,
    /// Screen rects of the two QuitConfirm buttons this frame, so a click
    /// can trigger the matching action without keyboard.
    pub(crate) quit_confirm_rects: [(Rect, bool); 2],
    /// (rect, path) markers for file paths in the transcript. Populated by
    /// tool renderers each frame; a click on `rect` opens `path` with the
    /// OS default app.
    pub(crate) file_link_markers: Vec<(usize, String)>,
    pub(crate) file_link_rects: Vec<(Rect, String)>,
    /// Rendered lines per transcript block, so a frame only re-parses the
    /// blocks that actually changed. Without it every keystroke and every
    /// streamed token re-ran the markdown parser over the whole session, so
    /// the cost of drawing a frame grew with the length of the conversation.
    pub(crate) render_cache: Vec<Option<crate::ui::BlockRender>>,
    /// Which attempt the trailing retry block is on, and the cap. Shown as
    /// `retry N/M` so collapsing the sequence still tells the user the agent
    /// is working through its budget rather than stuck.
    pub(crate) retry_attempt: u32,
    pub(crate) retry_max: u32,
    /// How many times the trailing error block's message has arrived in a row.
    /// Shown as `×N` so a silent collapse does not hide that it is still
    /// happening. Reset whenever a non-error block lands.
    pub(crate) error_repeats: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct TextSelection {
    pub anchor: (u16, u16), // (row, col)
    pub head: (u16, u16),
}

impl App {
    pub(crate) fn new(config: Config) -> Self {
        let theme = Theme::find(&config.ui.theme);
        let show_sidebar = config.ui.show_sidebar;
        let context_window = config.model.context_window;
        let workspace = config.workspace();
        let agent = Arc::new(Agent::new(config.clone()));
        let discovery = agent.discovery();
        Self {
            agent,
            settings: SettingsDraft::from_config(&config),
            config,
            store: SessionStore::default(),
            blocks: Vec::new(),
            input: String::new(),
            cursor: 0,
            session_id: None,
            title: String::new(),
            role: Role::Orchestrator,
            // A fresh session has no agent recorded yet, so the starting
            // agent is whatever core resolves the default role to.
            agent_name: enowx_core::Session::new(Role::Orchestrator).agent_or_default(),
            switch_markers: Vec::new(),
            busy: false,
            cancel: None,
            events: None,
            task: None,
            show_reasoning: true,
            show_tool_output: true,
            tool_expanded: std::collections::HashMap::new(),
            tool_header_markers: Vec::new(),
            tool_header_rects: Vec::new(),
            transcript_area: None,
            composer_field: None,
            composer_offset: 0,
            composer_width: 0,
            scroll: 0,
            max_scroll: 0,
            auto_scroll: true,
            should_quit: false,
            status: "ready".into(),
            tokens_in: 0,
            tokens_out: 0,
            modal: Modal::None,
            modal_cursor: 0,
            modal_items: Vec::new(),
            palette_cursor: 0,
            field_cursor: 0,
            discovering_models: false,
            model_events: None,
            models: Vec::new(),
            modal_error: String::new(),
            activity: Activity::Idle,
            activity_since: Instant::now(),
            turn_started: Instant::now(),
            theme,
            sidebar_tab: 0,
            sidebar_page: 0,
            sidebar_pages: 1,
            workspace,
            show_sidebar,
            tool_counts: HashMap::new(),
            attachments: Vec::new(),
            attach_error: None,
            sidebar_pages_area: None,
            sidebar_area: None,
            context_tokens: 0,
            context_window,
            mcp_field: 0,
            popup_rows: Vec::new(),
            mcp_field_rows: Vec::new(),
            last_wheel: None,
            quit_confirm_yes: false,
            quit_confirm_rects: [(Rect::default(), false), (Rect::default(), false)],
            file_link_markers: Vec::new(),
            render_cache: Vec::new(),
            retry_attempt: 0,
            retry_max: 0,
            error_repeats: 0,
            file_link_rects: Vec::new(),
            selection: None,
            wrapped_snapshot: Vec::new(),
            popup_body: None,
            sidebar_tabs: Vec::new(),
            modal_rows: Vec::new(),
            discovery,
            modal_search: String::new(),
            mcp_draft: crate::modal::McpDraft::default(),
        }
    }

    pub(crate) fn push(&mut self, kind: TranscriptKind, text: impl Into<String>) {
        let text = text.into();
        // Errors collapse instead of stacking. A provider that is down, a bad
        // key, or a retry loop otherwise fills the transcript with the same
        // line over and over and pushes the real conversation off screen.
        if matches!(kind, TranscriptKind::Error | TranscriptKind::Retry) {
            self.push_failure(kind, text);
            return;
        }
        self.blocks.push(TranscriptBlock { kind, text });
    }

    /// Record an error as ONE block.
    ///
    /// A repeat of the message already showing updates that block in place and
    /// bumps its counter; a different message replaces it, because the newest
    /// failure is the one worth reading and the previous one is usually its
    /// cause rather than separate news. Any other block arriving in between
    /// ends the run, so an error from an earlier turn stays where it happened.
    fn push_failure(&mut self, kind: TranscriptKind, text: String) {
        // A retry that ends in failure should leave ONE block behind, not a
        // retry line plus an error line, so a terminal error takes over the
        // retry block it grew out of.
        if let Some(last) = self.blocks.last_mut() {
            if matches!(last.kind, TranscriptKind::Error | TranscriptKind::Retry) {
                let same_kind = std::mem::discriminant(&last.kind) == std::mem::discriminant(&kind);
                if same_kind && last.text == text {
                    self.error_repeats = self.error_repeats.saturating_add(1);
                } else {
                    last.kind = kind;
                    last.text = text;
                    self.error_repeats = 1;
                }
                return;
            }
        }
        self.error_repeats = 1;
        self.blocks.push(TranscriptBlock { kind, text });
    }

    pub(crate) fn set_activity(&mut self, activity: Activity) {
        if self.activity != activity {
            self.activity = activity;
            self.activity_since = Instant::now();
        }
    }

    pub(crate) fn spinner(&self) -> &'static str {
        let frame = self.activity_since.elapsed().as_millis() / 90;
        SPINNER[frame as usize % SPINNER.len()]
    }

    /// Which agent holds the session right now.
    ///
    /// A plain read: `agent_name` is resolved through core's
    /// `Session::agent_or_default` whenever it can change, so the footer and
    /// sidebar do not each pay for that resolution on every frame.
    pub(crate) fn active_agent(&self) -> &str {
        &self.agent_name
    }

    /// Switch agents on the user's say-so.
    ///
    /// A forced switch overrides whatever the router decided: the user asking
    /// for a specialist by name is a stronger signal than the model's
    /// classification, and `auto_switch` does not gate it — it governs the
    /// router's own switches, not the user's.
    pub(crate) fn force_agent(&mut self, name: &str) -> anyhow::Result<()> {
        let name = name.trim().to_ascii_lowercase();
        if !self.discovery.agents.iter().any(|a| a.name == name) {
            let known: Vec<&str> = self
                .discovery
                .agents
                .iter()
                .filter(|a| a.name != "compactor")
                .map(|a| a.name.as_str())
                .collect();
            anyhow::bail!("no agent named `{name}`. Available: {}", known.join(", "));
        }
        if name == self.agent_name {
            self.status = format!("already {name}");
            return Ok(());
        }
        // Persist when there is a session to persist to; before the first
        // message there is none, and the choice still has to hold for when
        // the session is created.
        if let Some(id) = self.session_id.clone() {
            if let Ok(mut session) = self.store.load(&id) {
                session.switch_agent(&name, "switched by the user");
                let _ = self.store.save(&session);
            }
        }
        self.push(
            crate::session::TranscriptKind::Notice,
            format!("→ {name} · switched by the user"),
        );
        self.agent_name = name.clone();
        self.status = format!("agent: {name}");
        Ok(())
    }

    /// Adopt a session's agent and handover history.
    ///
    /// The resolution from the legacy `role` stays in core so there is one
    /// copy of it to keep right once the loop stops setting `role` at all.
    pub(crate) fn adopt_agent(&mut self, session: &enowx_core::Session) {
        self.agent_name = session.agent_or_default();
    }

    /// Change the legacy role, keeping the displayed agent in step.
    ///
    /// While the loop still selects work by role, the role IS the agent; the
    /// two would otherwise disagree in the footer the moment someone ran
    /// `/role`.
    pub(crate) fn set_role(&mut self, role: Role) {
        self.role = role;
        self.adopt_agent(&enowx_core::Session::new(role));
    }

    /// Model name shown in the composer footer.
    pub(crate) fn model_label(&self) -> String {
        let m = self.config.model.default.trim();
        if m.is_empty() {
            "no model".to_string()
        } else {
            m.to_string()
        }
    }

    /// Rotating tip for the idle composer footer so it never reads as blank.
    pub(crate) fn footer_tip(&self) -> &'static str {
        // Cycle every 6 seconds so a curious user sees more than one hint
        // without the text flickering during a burst of activity.
        const TIPS: [&str; 7] = [
            "/help for commands",
            "Ctrl+P opens the command palette",
            "Ctrl+B toggles sidebar",
            "/skills to browse skills",
            "/mcp to manage MCP servers",
            "/compact frees context",
            "F1–F5 switch sidebar tabs",
        ];
        let slot = (self.activity_since.elapsed().as_secs() / 6) as usize;
        TIPS[slot % TIPS.len()]
    }
}

/// Format an elapsed second count as `s` / `m s` / `h m` so a long turn does
/// not display `3612s`. Never shows leading zeros; anything under a minute
/// stays raw seconds.
pub(crate) fn fmt_elapsed(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}

// Restore the original App impl block; the fmt_elapsed + fmt_tests split it.
impl App {
    pub(crate) fn refresh_discovery(&mut self) {
        self.discovery = self.agent.discovery();
    }
    pub(crate) fn adopt(&mut self, config: Config) {
        self.theme = Theme::find(&config.ui.theme);
        self.show_sidebar = config.ui.show_sidebar;
        self.settings = SettingsDraft::from_config(&config);
        self.context_window = config.model.context_window;
        self.config = config.clone();
        self.agent = Arc::new(Agent::new(config));
        self.refresh_discovery();
    }

    pub(crate) fn new_session(&mut self) {
        self.blocks.clear();
        self.switch_markers.clear();
        self.adopt_agent(&enowx_core::Session::new(self.role));
        self.session_id = None;
        self.title.clear();
        self.tokens_in = 0;
        self.tokens_out = 0;
        self.context_tokens = 0;
        self.tool_counts.clear();
        self.events = None;
        self.auto_scroll = true;
        self.scroll = 0;
        self.status = "new session".into();
    }
}

#[cfg(test)]
mod fmt_tests {
    use super::fmt_elapsed;
    #[test]
    fn rolls_over_at_minutes_and_hours() {
        assert_eq!(fmt_elapsed(0), "0s");
        assert_eq!(fmt_elapsed(45), "45s");
        assert_eq!(fmt_elapsed(60), "1m 0s");
        assert_eq!(fmt_elapsed(135), "2m 15s");
        assert_eq!(fmt_elapsed(3600), "1h 0m");
        assert_eq!(fmt_elapsed(4923), "1h 22m");
    }
}
