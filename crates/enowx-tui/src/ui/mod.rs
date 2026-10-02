use crate::{
    app::App,
    modal::{Modal, SettingsField},
    session::TranscriptKind,
    text::{input_rows, trim},
    theme::Theme,
};
use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;
mod chrome;
mod composer;
mod home;
mod mark;
mod markdown;
mod pickers;
mod popups;
mod question;
mod settings;
mod sidebar;
mod tool;
mod transcript;
use chrome::*;
use composer::*;
use home::*;
use pickers::*;
use popups::draw_popup;
use question::{draw_question, question_height};
use sidebar::*;
pub(crate) use sidebar::{AGENTS_TAB, LOG_TAB, TABS};
pub(crate) use tool::opens_by_default;
pub(crate) use transcript::FileLink;
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
    pub(crate) file_links: Vec<transcript::FileLink>,
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
    // No margin and no window frame: the boxes themselves are the layout,
    // and a frame around them drew a second edge beside every first one.
    draw_main(frame, app, area);
    draw_settings_nav(frame, app);
    if app.modal != Modal::None && !draw_popup(frame, app) {
        draw_modal(frame, app);
    }
    draw_page_tabs(frame, app);
    no_control_characters(frame.buffer_mut());
}

/// The tabs at the top right of the main column: Chat and Settings. Drawn
/// on the top edge of whatever box is there, last, so a section or a popup
/// never hides them.
fn draw_page_tabs(frame: &mut Frame, app: &mut App) {
    use crate::app::pages::Tab;
    app.page_tabs.clear();
    let Some(main) = app.main_area else {
        return;
    };
    let t = app.theme;
    let current = app.tab();
    let labels: Vec<(Tab, String)> = Tab::ALL
        .iter()
        .map(|tab| (*tab, format!(" {} ", tab.label())))
        .collect();
    let width: u16 = labels
        .iter()
        .map(|(_, l)| l.chars().count() as u16)
        .sum::<u16>()
        + labels.len().saturating_sub(1) as u16;
    // Room for the box's corner and its title on the left; on a narrow window
    // the tabs give way rather than overwrite the title.
    if main.width < width + 24 {
        return;
    }
    let mut x = main.right().saturating_sub(width + 2);
    let y = main.y;
    for (i, (tab, label)) in labels.iter().enumerate() {
        if i > 0 {
            frame.render_widget(
                Paragraph::new(Span::styled("·", Style::default().fg(t.faint).bg(t.panel))),
                Rect::new(x, y, 1, 1),
            );
            x += 1;
        }
        let w = label.chars().count() as u16;
        let style = if *tab == current {
            Style::default()
                .fg(t.panel)
                .bg(t.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(t.muted).bg(t.panel)
        };
        let rect = Rect::new(x, y, w, 1);
        frame.render_widget(Paragraph::new(Span::styled(label.clone(), style)), rect);
        app.page_tabs.push((rect, *tab));
        x += w;
    }
}

/// Columns the Settings section list takes, its border included.
const SETTINGS_NAV_WIDTH: u16 = 20;

/// The Settings section list, on the left of the main column, and the area
/// beside it where the chosen section draws. On a window too small for both
/// the section takes the whole column and the list is left out (`Ctrl+P`
/// and the section's own keys still work).
fn draw_settings_nav(frame: &mut Frame, app: &mut App) {
    use crate::app::pages::{Tab, SECTIONS};
    app.settings_sections.clear();
    app.settings_content = None;
    if app.tab() != Tab::Settings {
        return;
    }
    let Some(main) = app.main_area else {
        return;
    };
    if main.width < SETTINGS_NAV_WIDTH + 40 || main.height < (SECTIONS.len() as u16) + 4 {
        return;
    }
    let t = app.theme;
    let nav = Rect::new(main.x, main.y, SETTINGS_NAV_WIDTH, main.height);
    app.settings_content = Some(Rect::new(
        main.x + SETTINGS_NAV_WIDTH + 1,
        main.y,
        main.width - SETTINGS_NAV_WIDTH - 1,
        main.height,
    ));
    // The list and the one-column gap after it are cleared: the chat drawn
    // underneath would otherwise show through between the two boxes.
    let gap = Rect::new(nav.right(), main.y, 1, main.height);
    frame.render_widget(Clear, nav);
    frame.render_widget(Clear, gap);
    frame.render_widget(Block::default().style(Style::default().bg(t.canvas)), gap);
    let focused = app.settings_nav;
    panel_box(
        frame,
        nav,
        if focused { t.accent } else { t.border },
        t.panel,
    );
    box_title(
        frame,
        nav,
        vec![Span::styled(
            "SETTINGS",
            Style::default()
                .fg(if focused { t.accent } else { t.muted })
                .add_modifier(Modifier::BOLD),
        )],
        t.panel,
    );
    box_hint(
        frame,
        nav,
        if focused {
            "↑↓ · → open"
        } else {
            "← here"
        },
        t.muted,
        t.panel,
    );
    let current = app.section_index();
    let inner = Rect::new(nav.x + 1, nav.y + 2, nav.width - 2, nav.height - 3);
    for (i, page) in SECTIONS.iter().enumerate() {
        let y = inner.y + i as u16;
        if y >= inner.bottom() {
            break;
        }
        // One row per section, the whole width clickable.
        let row = Rect::new(nav.x + 1, y, nav.width - 2, 1);
        let selected = i == current;
        let style = match (selected, focused) {
            (true, true) => Style::default()
                .fg(t.panel)
                .bg(t.accent)
                .add_modifier(Modifier::BOLD),
            (true, false) => Style::default()
                .fg(t.accent)
                .bg(t.panel)
                .add_modifier(Modifier::BOLD),
            _ => Style::default().fg(t.text).bg(t.panel),
        };
        let marker = if selected { "›" } else { " " };
        let width = row.width as usize;
        let text = format!(
            " {marker} {:<w$}",
            page.label(),
            w = width.saturating_sub(3)
        );
        frame.render_widget(Paragraph::new(Span::styled(text, style)), row);
        app.settings_sections.push((row, *page));
    }
}

/// Blank every cell holding a control character.
///
/// Ratatui's paragraph passes everything but `\n` through to the terminal.
/// A tab there moves the cursor to the next tab stop while ratatui believes
/// it moved one cell, so the rest of the row lands columns to the right, on
/// the side panel, and stays there: ratatui never repaints cells it thinks
/// are unchanged. A model's reasoning with a tab in its latest sentence left
/// a row's `▸` in the skills list this way. Text is cleaned where it is
/// laid out; this is the net for whatever gets past.
fn no_control_characters(buffer: &mut ratatui::buffer::Buffer) {
    for cell in buffer.content.iter_mut() {
        if cell.symbol().chars().any(char::is_control) {
            cell.set_symbol(" ");
        }
    }
}

/// See `crate::preview_markdown`.
pub(crate) fn preview_markdown(
    text: &str,
    width: usize,
    theme: &crate::theme::Theme,
) -> Vec<String> {
    let mut lines: Vec<ratatui::text::Line<'static>> = Vec::new();
    markdown::render_markdown(text, width, &mut lines, theme);
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
    // The path is not drawn: in a session the tool row above the diff names
    // the file. Kept in the signature so preview callers stay unchanged.
    let _ = path;
    tool::render_diff(old, new, start_line, width, &mut lines, theme);
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
