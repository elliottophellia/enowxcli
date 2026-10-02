//! Skills / MCP / MCP-form popups. Split from `pickers.rs` so the search box,
//! toggle styling, and add-new form stay legible instead of squeezed into the
//! existing generic list picker.

use super::*;
use crate::app::mcp_ui::McpRow;
use crate::modal::{McpFormField, MCP_FORM_FIELDS, MCP_FORM_LABELS};
use enowx_core::discovery::McpTransport;

/// Dispatch for the three new popups. `pickers::draw_modal` still owns every
/// legacy modal; this only handles the ones the composer commands opened.
pub(super) fn draw_popup(frame: &mut Frame, app: &mut App) -> bool {
    app.popup_rows.clear();
    app.popup_body = None;
    app.mcp_field_rows.clear();
    match app.modal {
        Modal::Skills => {
            draw_skills(frame, app);
            true
        }
        Modal::Commands => {
            draw_commands(frame, app);
            true
        }
        Modal::Mcp => {
            draw_mcp(frame, app);
            true
        }
        Modal::McpForm => {
            draw_mcp_form(frame, app);
            true
        }
        Modal::QuitConfirm => {
            draw_quit_confirm(frame, app);
            true
        }
        _ => false,
    }
}

/// The search field an overlay's list filters by: a label, then the query and
/// a caret, on the content's first row.
fn search_row(app: &App, area: Rect, frame: &mut Frame) {
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("search  ", Style::default().fg(app.theme.muted)),
            Span::styled(
                format!("{}_", app.modal_search),
                Style::default().fg(app.theme.text),
            ),
        ])),
        Rect::new(area.x, area.y, area.width, 1),
    );
}

/// The selected row's band, for lists whose rows carry their own marker (the
/// on/off dot) in the marker column instead of the `›`.
fn highlight_style(app: &App) -> Style {
    Style::default()
        .bg(app.theme.active_tab)
        .add_modifier(Modifier::BOLD)
}

/// Content split into the search row, a blank row, and the list.
fn search_layout(content: Rect) -> (Rect, Rect) {
    let search = Rect::new(content.x, content.y, content.width, 1);
    let list = Rect::new(
        content.x,
        content.y + 2,
        content.width,
        content.height.saturating_sub(2),
    );
    (search, list)
}

/// Click targets for a list's visible rows: the whole row, and the first
/// three columns where the on/off dot sits.
fn register_rows(app: &mut App, list_area: Rect, count: usize) {
    let visible = list_area.height as usize;
    let start = app.modal_cursor.saturating_sub(visible.saturating_sub(1));
    for (offset, row_idx) in (start..(start + visible).min(count)).enumerate() {
        let y = list_area.y + offset as u16;
        let row_rect = Rect::new(list_area.x, y, list_area.width, 1);
        let mark_rect = Rect::new(list_area.x, y, 3.min(list_area.width), 1);
        app.popup_rows.push((row_rect, mark_rect, row_idx));
    }
}

// ------------------------------ Skills ------------------------------

fn draw_skills(frame: &mut Frame, app: &mut App) {
    let rows = app.skill_rows();
    if !rows.is_empty() && app.modal_cursor >= rows.len() {
        app.modal_cursor = rows.len() - 1;
    }
    // The search row, a blank row, then the list.
    let height = (rows.len() as u16 + 2).max(4);
    let (_, content) = overlay(
        frame,
        app,
        92,
        height,
        app.modal.title(),
        "Enter read · Tab enable/disable · Esc close",
    );
    let (search, list_area) = search_layout(content);
    search_row(app, search, frame);

    if rows.is_empty() {
        frame.render_widget(
            Paragraph::new("No skills discovered. Drop a SKILL.md under .agents/skills/.")
                .style(Style::default().fg(app.theme.muted)),
            list_area,
        );
        return;
    }

    let t = app.theme;
    let items: Vec<ListItem> = rows
        .iter()
        .map(|row| {
            let (mark, colour, name_style) = if row.enabled {
                (
                    "●",
                    t.accent,
                    Style::default().fg(t.text).add_modifier(Modifier::BOLD),
                )
            } else {
                ("○", t.muted, Style::default().fg(t.muted))
            };
            let scope = row.scope.label();
            let desc = row.description.split('\n').next().unwrap_or("");
            // A built-in skill is offered only to the agents that carry it,
            // so the row says which.
            let carriers: Vec<&str> = if row.scope == enowx_core::discovery::SkillScope::Builtin {
                app.discovery
                    .agents
                    .iter()
                    .filter(|agent| agent.skills.iter().any(|s| s == &row.name))
                    .map(|agent| agent.name.as_str())
                    .collect()
            } else {
                Vec::new()
            };
            let desc = if carriers.is_empty() {
                desc.to_owned()
            } else {
                format!("for {} · {desc}", carriers.join(", "))
            };
            // A parent says how many parts it carries with it.
            let parts = if row.parts > 0 {
                format!(" +{} parts", row.parts)
            } else {
                String::new()
            };
            let pad = 28usize.saturating_sub(row.name.chars().count() + parts.chars().count());
            // The dot is this row's marker, on the marker column; the name
            // starts on the text column like every other list.
            ListItem::new(Line::from(vec![
                Span::styled(format!("{mark} "), Style::default().fg(colour)),
                Span::styled(row.name.clone(), name_style),
                Span::styled(parts, Style::default().fg(t.muted)),
                Span::raw(" ".repeat(pad)),
                Span::styled(format!(" {scope:<9}"), Style::default().fg(t.muted)),
                Span::styled(
                    trim(&desc, list_area.width.saturating_sub(41) as usize),
                    Style::default().fg(t.text),
                ),
            ]))
        })
        .collect();
    let mut state = ListState::default().with_selected(Some(app.modal_cursor));
    app.popup_body = Some(list_area);
    register_rows(app, list_area, rows.len());
    frame.render_stateful_widget(
        List::new(items).highlight_style(highlight_style(app)),
        list_area,
        &mut state,
    );
}

// ------------------------------ MCP list ------------------------------

fn draw_mcp(frame: &mut Frame, app: &mut App) {
    let rows = app.mcp_rows();
    if !rows.is_empty() && app.modal_cursor >= rows.len() {
        app.modal_cursor = rows.len() - 1;
    }
    // The search row, a blank row, then the list.
    let height = (rows.len() as u16 + 2).max(4);
    let (_, content) = overlay(
        frame,
        app,
        100,
        height,
        app.modal.title(),
        "Tab on/off · c configure · t tools · Enter edit · Esc",
    );
    let (search, list_area) = search_layout(content);
    search_row(app, search, frame);

    let t = app.theme;
    let items: Vec<ListItem> = rows
        .iter()
        .map(|row| match row {
            McpRow::Server {
                name,
                transport,
                scope,
                enabled,
                builtin,
                configured,
                target,
            } => {
                let (mark, colour, name_style) = if *enabled {
                    (
                        "●",
                        t.accent,
                        Style::default().fg(t.text).add_modifier(Modifier::BOLD),
                    )
                } else {
                    ("○", t.muted, Style::default().fg(t.muted))
                };
                let transport = match transport {
                    McpTransport::Stdio => "stdio",
                    McpTransport::Http => "http",
                    McpTransport::Sse => "sse",
                };
                // A built-in server says it is built in, and whether it is set
                // up; a discovered one shows its scope.
                let (tag, tag_colour) = if *builtin {
                    if *configured {
                        ("built-in", t.muted)
                    } else {
                        ("set up with c", t.yellow)
                    }
                } else {
                    (scope.label(), t.muted)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{mark} "), Style::default().fg(colour)),
                    Span::styled(format!("{name:<22}"), name_style),
                    Span::styled(format!(" {transport:<6}"), Style::default().fg(t.muted)),
                    Span::styled(format!(" {tag:<14}"), Style::default().fg(tag_colour)),
                    Span::styled(
                        trim(target, list_area.width.saturating_sub(48) as usize),
                        Style::default().fg(t.faint),
                    ),
                ]))
            }
            McpRow::AddNew => ListItem::new(Line::from(vec![
                Span::styled("+ ", Style::default().fg(t.accent)),
                Span::styled(
                    "Add new MCP server",
                    Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
                ),
            ])),
        })
        .collect();
    let mut state = ListState::default().with_selected(Some(app.modal_cursor));
    app.popup_body = Some(list_area);
    register_rows(app, list_area, rows.len());
    frame.render_stateful_widget(
        List::new(items).highlight_style(highlight_style(app)),
        list_area,
        &mut state,
    );
}

// ------------------------------ MCP form ------------------------------

fn draw_mcp_form(frame: &mut Frame, app: &mut App) {
    // The same form adds and edits; say which, or an edit looks like it is
    // about to create a second entry.
    let title = if app.mcp_draft.name.trim().is_empty() {
        "ADD MCP SERVER"
    } else {
        "EDIT MCP SERVER"
    };
    let (_, content) = overlay(
        frame,
        app,
        78,
        // The fields, a blank row, and the row an error goes in.
        MCP_FORM_FIELDS.len() as u16 + 2,
        title,
        "Tab/↓ next · ↑ prev · Space transport · Enter save · Esc cancel",
    );
    let t = app.theme;
    for (i, field) in MCP_FORM_FIELDS.iter().enumerate() {
        let y = content.y + i as u16;
        if y >= content.bottom() {
            break;
        }
        let selected = i == app.mcp_field;
        let label_style = if selected {
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(t.muted)
        };
        let value = match field {
            McpFormField::Name => app.mcp_draft.name.clone(),
            McpFormField::Command => app.mcp_draft.command.clone(),
            McpFormField::Args => app.mcp_draft.args.clone(),
            McpFormField::Env => app.mcp_draft.env.chars().take(60).collect(),
            McpFormField::Transport => match app.mcp_draft.transport {
                McpTransport::Stdio => "stdio".into(),
                McpTransport::Http => "http".into(),
                McpTransport::Sse => "sse".into(),
            },
        };
        let empty = value_is_empty(field, app);
        let (value, value_style) = if empty {
            (
                field.placeholder().to_string(),
                Style::default().fg(t.muted),
            )
        } else {
            (value, Style::default().fg(t.text))
        };
        // Marker on column 2, the label on column 4 in a fixed column, and
        // the value after it: every field's value starts on one column.
        let line = Line::from(vec![
            Span::styled(
                if selected { "› " } else { "  " },
                Style::default().fg(t.accent),
            ),
            Span::styled(format!("{:<26}", MCP_FORM_LABELS[i]), label_style),
            Span::styled(value, value_style),
        ]);
        let row = Rect::new(content.x, y, content.width, 1);
        app.mcp_field_rows.push((row, i));
        frame.render_widget(Paragraph::new(line), row);
    }
    if !app.modal_error.is_empty() && content.height > 0 {
        frame.render_widget(
            Paragraph::new(trim(&app.modal_error, content.width as usize))
                .style(Style::default().fg(t.red)),
            Rect::new(content.x, content.bottom() - 1, content.width, 1),
        );
    }
}

fn value_is_empty(field: &McpFormField, app: &App) -> bool {
    match field {
        McpFormField::Name => app.mcp_draft.name.is_empty(),
        McpFormField::Command => app.mcp_draft.command.is_empty(),
        McpFormField::Args => app.mcp_draft.args.is_empty(),
        McpFormField::Env => app.mcp_draft.env.is_empty(),
        McpFormField::Transport => false,
    }
}

/// Simple confirm dialog for Ctrl+C on an empty composer. Y/Enter quits,
/// N/Esc cancels. Kept small so it never covers the transcript.
fn draw_quit_confirm(frame: &mut Frame, app: &mut App) {
    // The question, a blank row, and the buttons.
    let (_, content) = overlay(
        frame,
        app,
        46,
        3,
        app.modal.title(),
        "Y quit · N stay · Enter confirm",
    );
    let t = app.theme;
    frame.render_widget(
        Paragraph::new("Quit Enx? Unsent input will be lost.").style(Style::default().fg(t.text)),
        Rect::new(content.x, content.y, content.width, 1),
    );

    // Two buttons centred on the content's last row: `[  Yes  ]  [  No  ]`.
    // The active one uses the selection band and the accent so keyboard
    // focus is obvious; the idle one is muted. Both are click targets.
    let yes_label = "  Yes  ";
    let no_label = "  No  ";
    let gap = 2usize;
    let total_w = yes_label.chars().count() + gap + no_label.chars().count() + 4;
    let start = content.x + (content.width.saturating_sub(total_w as u16)) / 2;
    let y = content.y + 2;
    let yes_rect = Rect::new(start, y, (yes_label.chars().count() + 2) as u16, 1);
    let no_rect = Rect::new(
        yes_rect.x + yes_rect.width + gap as u16,
        y,
        (no_label.chars().count() + 2) as u16,
        1,
    );
    app.quit_confirm_rects = [(yes_rect, true), (no_rect, false)];

    let active_style = Style::default()
        .fg(t.accent)
        .bg(t.active_tab)
        .add_modifier(Modifier::BOLD);
    let idle_style = Style::default().fg(t.muted);
    let (yes_style, no_style) = if app.quit_confirm_yes {
        (active_style, idle_style)
    } else {
        (idle_style, active_style)
    };
    frame.render_widget(
        Paragraph::new(Line::styled(format!("[{yes_label}]"), yes_style)),
        yes_rect,
    );
    frame.render_widget(
        Paragraph::new(Line::styled(format!("[{no_label}]"), no_style)),
        no_rect,
    );
}

/// The command palette: every command, searchable.
///
/// Separate from the inline list above the composer. That one serves someone
/// typing a name they already know; this one is for looking, so it searches
/// summaries too and gets the room to show them.
fn draw_commands(frame: &mut Frame, app: &mut App) {
    app.modal_rows.clear();
    let rows = app.palette_rows();
    if !rows.is_empty() && app.modal_cursor >= rows.len() {
        app.modal_cursor = rows.len() - 1;
    }
    // Browsing, the commands come in their groups, each under a heading and
    // a blank line apart. Searching, they come best match first, and a
    // heading would only split a ranking.
    enum Entry {
        Heading(&'static str),
        Gap,
        Row(usize),
    }
    let grouped = app.modal_search.trim().is_empty();
    let mut entries: Vec<Entry> = Vec::new();
    let mut group = "";
    for (index, row) in rows.iter().enumerate() {
        if grouped && row.group != group {
            if !group.is_empty() {
                entries.push(Entry::Gap);
            }
            entries.push(Entry::Heading(row.group));
            group = row.group;
        }
        entries.push(Entry::Row(index));
    }
    // The search row, a blank row, then the list.
    let height = (entries.len() as u16 + 2).max(4);
    let (_, content) = overlay(
        frame,
        app,
        72,
        height,
        app.modal.title(),
        "↑↓ move · Enter run · Esc close",
    );
    let (search, list_area) = search_layout(content);
    search_row(app, search, frame);

    let t = app.theme;
    // How many there are, on the search row's right: a long list runs past
    // the window, and nothing else says so.
    let all = crate::commands::COMMANDS.len();
    let count = if rows.len() == all {
        format!("{all} commands")
    } else {
        format!("{} of {all}", rows.len())
    };
    frame.render_widget(
        Paragraph::new(count)
            .alignment(Alignment::Right)
            .style(Style::default().fg(t.faint)),
        search,
    );
    if rows.is_empty() {
        frame.render_widget(
            Paragraph::new("No command matches.").style(Style::default().fg(t.muted)),
            list_area,
        );
        return;
    }

    // Scroll only as far as it takes to keep the selection on screen, with
    // its heading when it is the first of its group.
    let visible = list_area.height as usize;
    let at = entries
        .iter()
        .position(|entry| matches!(entry, Entry::Row(index) if *index == app.modal_cursor))
        .unwrap_or(0);
    let top = if at > 0 && matches!(entries[at - 1], Entry::Heading(_)) {
        at - 1
    } else {
        at
    };
    let mut offset = app
        .palette_offset
        .min(entries.len().saturating_sub(visible));
    if top < offset {
        offset = top;
    }
    if visible > 0 && at >= offset + visible {
        offset = at + 1 - visible;
    }
    app.palette_offset = offset;

    let label_width = rows.iter().map(|row| row.label.width()).max().unwrap_or(0);
    for (line, entry) in entries.iter().enumerate().skip(offset).take(visible) {
        let area = Rect::new(
            list_area.x,
            list_area.y + (line - offset) as u16,
            list_area.width,
            1,
        );
        match entry {
            Entry::Gap => {}
            // On the text column, like the side cards' section labels.
            Entry::Heading(name) => frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        name.to_uppercase(),
                        Style::default().fg(t.muted).add_modifier(Modifier::BOLD),
                    ),
                ])),
                area,
            ),
            Entry::Row(index) => {
                let row = rows[*index];
                let selected = *index == app.modal_cursor;
                let summary_width = (area.width as usize).saturating_sub(2 + label_width + 2);
                let label = if selected {
                    Style::default().fg(t.accent).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(t.text)
                };
                frame.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled(
                            if selected { "› " } else { "  " },
                            Style::default().fg(t.accent),
                        ),
                        Span::styled(format!("{:<label_width$}  ", row.label), label),
                        Span::styled(
                            trim(row.summary, summary_width),
                            Style::default().fg(t.muted),
                        ),
                    ]))
                    .style(Style::default().bg(if selected {
                        t.active_tab
                    } else {
                        t.panel
                    })),
                    area,
                );
                app.modal_rows.push((area, *index));
            }
        }
    }
}
