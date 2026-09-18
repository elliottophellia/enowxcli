//! Public shim for integration tests. Exposes the small slice of `App` state
//! the mouse-dispatch tests need without leaking every internal.

use crate::app::App;
use crate::modal::Modal;
use crate::session::TranscriptKind;

use crossterm::event::MouseEvent;
use enowx_core::Config;
use ratatui::layout::Rect;

pub struct TestApp {
    inner: App,
}

impl Default for TestApp {
    fn default() -> Self {
        Self::new()
    }
}

impl TestApp {
    pub fn new() -> Self {
        let tmp =
            std::env::temp_dir().join(format!("enx-test-{}-{}", std::process::id(), fastrand()));
        let home_dir = tmp.join("home");
        let enx_home = tmp.join("enx");
        let workspace = tmp.join("ws");
        for p in [&home_dir, &enx_home, &workspace] {
            let _ = std::fs::create_dir_all(p);
        }
        // Isolate from the developer's real ~/.enx and ~/.agents so discovery
        // only sees files the test seeded.
        std::env::set_var("HOME", &home_dir);
        std::env::set_var("ENX_HOME", &enx_home);
        std::env::set_current_dir(&workspace).ok();
        let mut config = Config::default();
        config.provider.name = "test".into();
        config.provider.base_url = "http://127.0.0.1:1".into();
        Self {
            inner: App::new(config),
        }
    }

    pub fn new_with_skills(names: &[&str]) -> Self {
        let mut app = Self::new();
        let ws = app.inner.config.workspace();
        for name in names {
            let dir = ws.join(".agents/skills").join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("SKILL.md"),
                format!(
                    "---\nname: {name}\ndescription: test skill {name}\nallowed-tools: read\n---\n\nbody-of-{name}\n"
                ),
            )
            .unwrap();
        }
        app.inner.adopt(app.inner.config.clone());
        app
    }

    pub fn mouse(&mut self, event: MouseEvent) -> anyhow::Result<()> {
        self.inner.mouse(event)
    }

    pub fn set_max_scroll(&mut self, value: u16) {
        self.inner.max_scroll = value;
    }
    pub fn scroll(&self) -> u16 {
        self.inner.scroll
    }
    pub fn modal_cursor(&self) -> usize {
        self.inner.modal_cursor
    }
    pub fn mcp_field(&self) -> usize {
        self.inner.mcp_field
    }
    pub fn is_modal_open(&self) -> bool {
        self.inner.modal != Modal::None
    }
    pub fn config_disabled_skills(&self) -> Vec<String> {
        self.inner.config.ui.disabled_skills.clone()
    }
    pub fn transcript_contains(&self, needle: &str) -> bool {
        self.inner
            .blocks
            .iter()
            .any(|b| matches!(b.kind, TranscriptKind::Notice) && b.text.contains(needle))
    }

    pub fn set_popup_rows(&mut self, rows: Vec<(Rect, Rect, usize)>) {
        self.inner.popup_rows = rows;
    }
    pub fn set_popup_body(&mut self, rect: Rect) {
        self.inner.popup_body = Some(rect);
    }
    pub fn set_mcp_field_rows(&mut self, rows: Vec<(Rect, usize)>) {
        self.inner.mcp_field_rows = rows;
    }

    pub fn enter_skills_modal(&mut self) {
        self.inner.open_skills();
    }
    pub fn enter_mcp_form(&mut self) {
        self.inner.open_mcp_form();
    }

    /// Index of a discovered skill by name so tests can seed the popup row
    /// for the exact entry they want to click.
    pub fn skill_index(&self, name: &str) -> Option<usize> {
        self.inner
            .discovery
            .skills
            .iter()
            .position(|s| s.name == name)
    }
}

fn fastrand() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0)
}

/// Draw the app into an off-screen buffer and return it as plain text, one
/// string per row. Used to assert that the render cache produces byte-identical
/// output to a cold render — a stale cache shows up as wrong pixels, which no
/// unit test of the key alone would catch.
impl TestApp {
    pub fn render_to_text(&mut self, width: u16, height: u16) -> Vec<String> {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
        term.draw(|f| crate::ui::draw(f, &mut self.inner)).unwrap();
        let buffer = term.backend().buffer().clone();
        // A double-width glyph occupies two cells: the first carries the
        // symbol, the second is left empty by ratatui. Skipping those empty
        // continuation cells reproduces what the terminal actually shows —
        // joining every cell would insert a phantom space after each wide
        // character and overstate the row's width.
        (0..height)
            .map(|y| {
                let mut row = String::new();
                let mut x = 0u16;
                while x < width {
                    let cell = &buffer[(x, y)];
                    let symbol = cell.symbol();
                    row.push_str(symbol);
                    let w = unicode_width::UnicodeWidthStr::width(symbol).max(1) as u16;
                    x += w;
                }
                row.trim_end().to_string()
            })
            .collect()
    }

    /// Drop every cached block so the next draw re-renders from scratch.
    pub fn clear_render_cache(&mut self) {
        self.inner.render_cache.clear();
    }

    /// How many blocks currently hold a cached rendering.
    pub fn cached_block_count(&self) -> usize {
        self.inner.render_cache.iter().filter(|c| c.is_some()).count()
    }

    pub fn push_user(&mut self, text: &str) {
        self.inner.push(TranscriptKind::User, text);
    }

    pub fn push_assistant(&mut self, text: &str) {
        self.inner.push(TranscriptKind::Assistant, text);
    }

    /// Append to the last block, the way a streamed token does.
    pub fn append_to_last(&mut self, delta: &str) {
        if let Some(last) = self.inner.blocks.last_mut() {
            last.text.push_str(delta);
        }
    }

    pub fn set_show_reasoning(&mut self, on: bool) {
        self.inner.show_reasoning = on;
    }

    pub fn push_reasoning(&mut self, text: &str) {
        self.inner.push(TranscriptKind::Reasoning, text);
    }
}

impl TestApp {
    /// Scroll to an absolute row and stop following the tail, so a render
    /// exercises the mid-transcript window rather than the bottom.
    pub fn scroll_to(&mut self, row: u16) {
        self.inner.auto_scroll = false;
        self.inner.scroll = row;
    }

    pub fn max_scroll(&self) -> u16 {
        self.inner.max_scroll
    }

    /// Rows of the transcript body that a click handler currently maps to a
    /// tool header, as (screen_y, tool_id).
    pub fn tool_header_rows(&self) -> Vec<(u16, String)> {
        self.inner
            .tool_header_rects
            .iter()
            .map(|(rect, id)| (rect.y, id.clone()))
            .collect()
    }

    pub fn push_tool(&mut self, id: &str, name: &str, args: &str, result: &str) {
        self.push_tool_with_status(id, name, args, result, false);
    }

    /// `error` mirrors what the agent sets from `ToolOutput::error`, so a
    /// fixture can reproduce a failed call rather than always looking green.
    pub fn push_tool_with_status(
        &mut self,
        id: &str,
        name: &str,
        args: &str,
        result: &str,
        error: bool,
    ) {
        self.inner.blocks.push(crate::session::TranscriptBlock {
            kind: TranscriptKind::Tool {
                id: id.into(),
                name: name.into(),
                args: args.into(),
                result: result.into(),
                running: false,
                error,
                started: None,
            },
            text: String::new(),
        });
    }
}

impl TestApp {
    pub fn set_show_tool_output(&mut self, on: bool) {
        self.inner.show_tool_output = on;
    }
}

impl TestApp {
    /// Whether the transcript is following new output. Flipped off when the
    /// user scrolls up, back on when they return to the last line.
    pub fn auto_scroll(&self) -> bool {
        self.inner.auto_scroll
    }
}

impl TestApp {
    pub fn push_error(&mut self, text: &str) {
        self.inner.push(TranscriptKind::Error, text);
    }

    pub fn block_count(&self) -> usize {
        self.inner.blocks.len()
    }

    pub fn error_block_count(&self) -> usize {
        self.inner
            .blocks
            .iter()
            .filter(|b| matches!(b.kind, TranscriptKind::Error))
            .count()
    }
}

impl TestApp {
    /// Rendered rows paired with whether they carry the theme's red, so a test
    /// can assert colour without hard-coding escape sequences.
    pub fn render_to_styled(&mut self, width: u16, height: u16) -> Vec<(String, bool)> {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let red = self.inner.theme.red;
        let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
        term.draw(|f| crate::ui::draw(f, &mut self.inner)).unwrap();
        let buffer = term.backend().buffer().clone();
        (0..height)
            .map(|y| {
                let mut row = String::new();
                let mut is_red = false;
                let mut x = 0u16;
                while x < width {
                    let cell = &buffer[(x, y)];
                    let symbol = cell.symbol();
                    if cell.fg == red && !symbol.trim().is_empty() {
                        is_red = true;
                    }
                    row.push_str(symbol);
                    x += unicode_width::UnicodeWidthStr::width(symbol).max(1) as u16;
                }
                (row.trim_end().to_string(), is_red)
            })
            .collect()
    }
}

impl TestApp {
    /// Feed a retry the way the agent's backoff loop does.
    pub fn push_retry(&mut self, message: &str, attempt: u32, max: u32) {
        self.inner.retry_attempt = attempt;
        self.inner.retry_max = max;
        self.inner.push(TranscriptKind::Retry, message);
    }

    pub fn retry_block_count(&self) -> usize {
        self.inner
            .blocks
            .iter()
            .filter(|b| matches!(b.kind, TranscriptKind::Retry))
            .count()
    }
}

/// Provider/settings form access, so the config-preservation rules can be
/// asserted without driving the modal through raw key events.
impl TestApp {
    #[allow(clippy::too_many_arguments)]
    pub fn seed_provider(
        &mut self,
        name: &str,
        preset: &str,
        base_url: &str,
        models_url: &str,
        api_key: &str,
        model: &str,
        context_window: u32,
    ) {
        let mut config = self.inner.config.clone();
        config.provider.name = name.into();
        config.provider.preset = preset.into();
        config.provider.base_url = base_url.into();
        config.provider.models_url = models_url.into();
        config.provider.api_key = api_key.into();
        config.model.default = model.into();
        config.model.context_window = context_window;
        self.inner.adopt(config);
    }

    pub fn open_settings(&mut self) {
        self.inner.open_settings();
    }

    pub fn open_providers(&mut self) {
        self.inner.open_providers();
    }

    /// Choose the Nth entry in the provider list, as Enter on that row does.
    pub fn select_provider_at(&mut self, index: usize) {
        self.inner.modal_cursor = index;
        self.inner.select_provider();
    }

    pub fn set_settings_field(&mut self, field: &str, value: &str) {
        let draft = &mut self.inner.settings;
        match field {
            "provider" => draft.provider = value.into(),
            "base_url" => draft.base_url = value.into(),
            "api_key" => draft.api_key = value.into(),
            "models_url" => draft.models_url = value.into(),
            "model" => draft.model = value.into(),
            "context_window" => draft.context_window = value.into(),
            "theme" => draft.theme = value.into(),
            other => panic!("unknown settings field {other}"),
        }
    }

    pub fn settings_field(&self, field: &str) -> String {
        let draft = &self.inner.settings;
        match field {
            "provider" => draft.provider.clone(),
            "base_url" => draft.base_url.clone(),
            "api_key" => draft.api_key.clone(),
            "models_url" => draft.models_url.clone(),
            "model" => draft.model.clone(),
            "context_window" => draft.context_window.clone(),
            "theme" => draft.theme.clone(),
            other => panic!("unknown settings field {other}"),
        }
    }

    pub fn save_settings(&mut self) -> anyhow::Result<()> {
        self.inner.save_settings()
    }

    pub fn config_provider(&self) -> (String, String, String, String, String) {
        let p = &self.inner.config.provider;
        (
            p.name.clone(),
            p.preset.clone(),
            p.base_url.clone(),
            p.models_url.clone(),
            p.api_key.clone(),
        )
    }

    pub fn config_model(&self) -> (String, u32) {
        (
            self.inner.config.model.default.clone(),
            self.inner.config.model.context_window,
        )
    }

    pub fn config_agent_max_steps(&self) -> u32 {
        self.inner.config.agent.max_steps
    }
}

impl TestApp {
    pub fn connect_preset(&mut self) -> anyhow::Result<()> {
        self.inner.connect_preset()
    }
}

impl TestApp {
    /// Simulate editing a settings field the way typing does: mutate the
    /// value, then fire the change hook.
    pub fn edit_settings_field(&mut self, field: &str, value: &str) {
        use crate::modal::SettingsField;
        self.set_settings_field(field, value);
        let which = match field {
            "provider" => SettingsField::Provider,
            "base_url" => SettingsField::BaseUrl,
            "api_key" => SettingsField::ApiKey,
            "models_url" => SettingsField::ModelsUrl,
            "model" => SettingsField::Model,
            "context_window" => SettingsField::ContextWindow,
            "theme" => SettingsField::Theme,
            other => panic!("unknown settings field {other}"),
        };
        self.inner.settings_changed(which);
    }
}

impl TestApp {
    /// True when the settings modal has closed, i.e. a save finished without
    /// pushing the user into another step.
    pub fn modal_closed(&self) -> bool {
        self.inner.modal == Modal::None
    }
}

impl TestApp {
    /// Open a specific tool block, as clicking its header does.
    pub fn expand_tool(&mut self, id: &str) {
        self.inner.tool_expanded.insert(id.to_string(), true);
    }
}
