use crate::{
    app::App,
    modal::{Modal, SettingsField, SETTINGS_FIELDS},
    session::TranscriptKind,
    text::{input_rows, trim},
    theme::Theme,
};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;
mod chrome;
mod composer;
mod pickers;
mod popups;
mod settings;
mod sidebar;
mod tool;
mod transcript;
use chrome::*;
use composer::*;
use pickers::*;
use popups::draw_popup;
use sidebar::*;
use transcript::*;

/// One transcript block's rendered output, cached between frames.
///
/// Markers are stored RELATIVE to the block's first line. The absolute index
/// a click handler needs depends on how many lines the blocks above produced,
/// which changes as the transcript grows — storing absolute offsets would
/// invalidate every block below an edit. The assembly step rebases them.
#[derive(Clone)]
pub(crate) struct BlockRender {
    /// What the block looked like when this was produced. A mismatch on the
    /// next frame is what forces a re-render.
    pub(crate) key: BlockKey,
    pub(crate) lines: Vec<ratatui::text::Line<'static>>,
    /// (tool_id, line_offset_within_block)
    pub(crate) tool_headers: Vec<(String, usize)>,
    /// (line_offset_within_block, path)
    pub(crate) file_links: Vec<(usize, String)>,
    /// Whether this block is a tool call. Consecutive tool blocks render
    /// without a blank line between them, so assembly needs to know.
    pub(crate) is_tool: bool,
    /// Reasoning block hidden by the current toggle: cached as empty so the
    /// key still tracks the toggle, and skipped at assembly.
    pub(crate) skipped: bool,
}

/// Everything a block's rendering depends on. Two blocks that agree on this
/// produce identical lines, so the cached copy can be reused.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct BlockKey {
    /// Cheap content fingerprint. A hash rather than the text itself so the
    /// key stays small for a long assistant turn.
    pub(crate) content: u64,
    pub(crate) width: usize,
    pub(crate) theme: &'static str,
    pub(crate) expanded: bool,
    pub(crate) show_reasoning: bool,
}

pub(crate) fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(app.theme.canvas).fg(app.theme.text)),
        area,
    );
    if area.width < 20 || area.height < 8 {
        return;
    }
    draw_main(
        frame,
        app,
        area.inner(Margin {
            horizontal: if area.width >= 60 { 2 } else { 1 },
            vertical: 0,
        }),
    );
    if app.modal != Modal::None && !draw_popup(frame, app) {
        draw_modal(frame, app);
    }
}

/// See `crate::preview_markdown`.
pub(crate) fn preview_markdown(
    text: &str,
    width: usize,
    theme: &crate::theme::Theme,
) -> Vec<String> {
    let mut lines: Vec<ratatui::text::Line<'static>> = Vec::new();
    transcript::render_markdown(text, width, &mut lines, theme);
    lines_to_ansi(lines)
}

/// Render one `edit` tool diff to ANSI strings. Companion to
/// `preview_markdown` for inspecting the diff gutter outside a session.
pub(crate) fn preview_diff(
    path: &str,
    old: &str,
    new: &str,
    start_line: usize,
    width: usize,
    theme: &crate::theme::Theme,
) -> Vec<String> {
    let mut lines: Vec<ratatui::text::Line<'static>> = Vec::new();
    let mut markers: Vec<(usize, String)> = Vec::new();
    tool::render_diff(
        path,
        old,
        new,
        start_line,
        width,
        &mut lines,
        theme,
        &mut markers,
    );
    lines_to_ansi(lines)
}

fn lines_to_ansi(lines: Vec<ratatui::text::Line<'static>>) -> Vec<String> {
    use ratatui::style::Color;
    fn sgr(color: Color, layer: u8) -> String {
        match color {
            Color::Rgb(r, g, b) => format!("\x1b[{layer};2;{r};{g};{b}m"),
            _ => String::new(),
        }
    }
    lines
        .into_iter()
        .map(|line| {
            let mut out = String::new();
            for span in line.spans {
                if let Some(bg) = span.style.bg {
                    out.push_str(&sgr(bg, 48));
                }
                if let Some(fg) = span.style.fg {
                    out.push_str(&sgr(fg, 38));
                }
                if span
                    .style
                    .add_modifier
                    .contains(ratatui::style::Modifier::BOLD)
                {
                    out.push_str("\x1b[1m");
                }
                if span
                    .style
                    .add_modifier
                    .contains(ratatui::style::Modifier::ITALIC)
                {
                    out.push_str("\x1b[3m");
                }
                out.push_str(&span.content);
                out.push_str("\x1b[0m");
            }
            out
        })
        .collect()
}
