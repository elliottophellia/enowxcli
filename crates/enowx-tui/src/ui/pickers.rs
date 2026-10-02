use super::settings::draw_settings;
use super::*;
use ratatui::text::Text;

/// Keys every list overlay shares, in its bottom edge.
const LIST_HINT: &str = "↑↓ move · Enter select · Esc close";

pub(super) fn draw_modal(frame: &mut Frame, app: &mut App) {
    app.modal_rows.clear();
    let area = frame.area();
    if app.modal.is_form() {
        draw_settings(frame, app, area);
        return;
    }
    if app.modal == Modal::MessageEdit {
        draw_message_edit(frame, app);
        return;
    }
    let width = area.width.saturating_sub(4).min(
        if app.modal == Modal::Sessions || app.modal == Modal::Attach {
            96
        } else if app.modal == Modal::Message {
            52
        } else {
            72
        },
    );
    let per_row = if matches!(
        app.modal,
        Modal::Agents
            | Modal::Message
            | Modal::TypeSafe
            | Modal::Providers
            | Modal::Themes
            | Modal::Effort
            | Modal::Handoff
    ) {
        2
    } else {
        1
    };
    if app.modal == Modal::Models {
        draw_model_picker(frame, app, width);
        return;
    }
    // The roster (agents, grouped) can run long; let a tall terminal show
    // more of it. Shorter lists keep their own height; `overlay` clamps to
    // the area so this never overflows a small screen.
    let cap = if app.modal == Modal::Agents { 34 } else { 20 };
    let body = (app.modal_items.len() as u16 * per_row).min(cap);
    let hint = if app.modal == Modal::Providers {
        "Enter connect · d disconnect · Esc close"
    } else if app.modal == Modal::Agents {
        "Enter switch · m model · d default model · Esc"
    } else {
        LIST_HINT
    };
    let (_, content) = overlay(frame, app, width, body.max(1), app.modal.title(), hint);
    let t = app.theme;
    if app.modal_items.is_empty() {
        let note = match app.modal {
            Modal::Sessions => "No saved sessions in this workspace yet.",
            _ => "Nothing here yet.",
        };
        frame.render_widget(
            Paragraph::new(note).style(Style::default().fg(t.muted)),
            content,
        );
        return;
    }
    // The `›` marker takes two columns; text past the box ends in `…`
    // rather than stopping mid-word at the edge.
    let text_width = (content.width as usize).saturating_sub(2);
    let mut heights: Vec<u16> = Vec::with_capacity(app.modal_items.len());
    let items: Vec<ListItem> = app
        .modal_items
        .iter()
        .enumerate()
        .map(|(index, (id, description))| {
            let selected = index == app.modal_cursor;
            let label = Style::default().fg(if selected { t.accent } else { t.text });
            let label = if selected {
                label.add_modifier(Modifier::BOLD)
            } else {
                label
            };
            // The name on the text column and what it does under it, on the
            // same column: the marker has the two columns to their left.
            let text = if app.modal == Modal::Sessions {
                // Session picker rows hide the internal id, keeping the visible
                // list to the title and metadata the user recognises.
                Text::from(Line::styled(trim(description, text_width), label))
            } else {
                // Agents by their full name, the id `/agent` takes beside it.
                let shown = if app.modal == Modal::Agents {
                    enowx_core::agent_def::display_name(id)
                } else {
                    id.clone()
                };
                let mut name = vec![Span::styled(trim(&shown, text_width), label)];
                // The id only where it differs from the name: `fe`, not `docs`.
                if app.modal == Modal::Agents && shown.to_lowercase() != *id {
                    name.push(Span::styled(
                        format!("  {id}"),
                        Style::default().fg(t.faint),
                    ));
                }
                // An agent on a model of its own says which beside its name.
                if let Some(model) = own_model(app, id) {
                    let used: usize = name.iter().map(|span| span.content.chars().count()).sum();
                    let room = text_width.saturating_sub(used + 2);
                    name.push(Span::styled(
                        format!("  {}", trim(&model, room)),
                        Style::default().fg(t.muted),
                    ));
                }
                Text::from(vec![
                    Line::from(name),
                    Line::styled(trim(description, text_width), Style::default().fg(t.muted)),
                ])
            };
            heights.push(text.lines.len() as u16);
            ListItem::new(text)
        })
        .collect();
    let mut state = ListState::default().with_selected(Some(app.modal_cursor));
    frame.render_stateful_widget(selectable(List::new(items), &t), content, &mut state);
    register_list_rows(app, content, state.offset(), &heights);
}

/// The model `name` runs on in the agent list, when it is not the active
/// one: its own (`agent.models`) or its tier's (`agent.tiers`).
fn own_model(app: &App, name: &str) -> Option<String> {
    if app.modal != Modal::Agents {
        return None;
    }
    let agent = app.discovery.agents.iter().find(|a| a.name == name)?;
    let model = app.config.model_for(name, agent.tier);
    (!model.is_empty() && model != app.config.model.active.trim()).then_some(model)
}

/// Click targets for the rows a list drew. The list scrolls to keep the
/// selection in view, so rows are counted from its offset after drawing:
/// counted from the top, a click on a scrolled list picked the wrong row.
fn register_list_rows(app: &mut App, area: Rect, offset: usize, heights: &[u16]) {
    let mut y = area.y;
    for (index, rows) in heights.iter().enumerate().skip(offset) {
        if y + rows > area.bottom() {
            break;
        }
        app.modal_rows
            .push((Rect::new(area.x, y, area.width, *rows), index));
        y += rows;
    }
}

/// A list whose selected row carries the `›` marker in the two columns before
/// the text and a tinted band, the same way in every overlay.
///
/// The band sets only the background. A foreground in it would repaint every
/// cell of the row and flatten the name/description colours into one.
pub(super) fn selectable<'a>(list: List<'a>, t: &Theme) -> List<'a> {
    list.highlight_symbol("› ")
        .highlight_spacing(ratatui::widgets::HighlightSpacing::Always)
        .highlight_style(Style::default().bg(t.active_tab))
}

/// `/model`: a search field, then the favourite and recent models and each
/// connected provider's, under headings.
fn draw_model_picker(frame: &mut Frame, app: &mut App, width: u16) {
    use crate::app::model_picker::PickerRow;
    let t = app.theme;
    let rows = app.picker_rows();
    let area = frame.area();
    // The search row and a gap, then as many rows as fit a comfortable box.
    let body = (rows.len() as u16 + 2).clamp(5, area.height.saturating_sub(8).max(5));
    let hint = if width < 64 {
        "Enter use · ^F favourite · Esc"
    } else {
        "Enter use · ^F favourite · ^N add · ^E edit · ^R refresh · Esc"
    };
    // Choosing for one agent, the title says whose model it is.
    let title = match &app.picking_for_agent {
        Some(agent) => format!(
            " MODEL FOR {} ",
            enowx_core::agent_def::display_name(agent).to_uppercase()
        ),
        None => app.modal.title().to_owned(),
    };
    let (_, content) = overlay(frame, app, width, body, &title, hint);
    if content.height < 3 {
        return;
    }
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("search  ", Style::default().fg(t.muted)),
            Span::styled(
                format!("{}_", app.modal_search),
                Style::default().fg(t.text),
            ),
        ])),
        Rect::new(content.x, content.y, content.width, 1),
    );
    let list = Rect::new(
        content.x,
        content.y + 2,
        content.width,
        content.height.saturating_sub(2),
    );
    if rows.is_empty() {
        frame.render_widget(
            Paragraph::new("No model matches. Esc clears the search.")
                .style(Style::default().fg(t.muted)),
            list,
        );
        return;
    }
    let text_width = (list.width as usize).saturating_sub(2);
    let items: Vec<ListItem> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| match row {
            PickerRow::Heading(name) => ListItem::new(Line::styled(
                trim(name, text_width),
                Style::default().fg(t.muted).add_modifier(Modifier::BOLD),
            )),
            PickerRow::Note(note) => ListItem::new(Line::styled(
                trim(note, text_width),
                Style::default().fg(t.faint),
            )),
            PickerRow::Model { label, detail, .. } => {
                let selected = index == app.modal_cursor;
                let label_style = if selected {
                    Style::default().fg(t.accent).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(t.text)
                };
                let label = trim(label, text_width);
                let room = text_width.saturating_sub(label.chars().count() + 2);
                let mut spans = vec![Span::styled(label, label_style)];
                if !detail.is_empty() && room > 3 {
                    spans.push(Span::styled(
                        format!("  {}", trim(detail, room)),
                        Style::default().fg(t.muted),
                    ));
                }
                ListItem::new(Line::from(spans))
            }
        })
        .collect();
    let count = items.len();
    let mut state = ListState::default().with_selected(Some(app.modal_cursor));
    frame.render_stateful_widget(selectable(List::new(items), &t), list, &mut state);
    // Only model rows take a click; a heading or a note is not a choice.
    let offset = state.offset();
    for (y, (index, row)) in
        (list.y..list.bottom()).zip(rows.iter().enumerate().skip(offset).take(count))
    {
        if matches!(row, PickerRow::Model { .. }) {
            app.modal_rows
                .push((Rect::new(list.x, y, list.width, 1), index));
        }
    }
}

/// The prompt being edited before it is sent again. A plain field rather than
/// the settings form: there is one value, and Enter sends it.
fn draw_message_edit(frame: &mut Frame, app: &App) {
    let t = app.theme;
    let area = frame.area();
    let width = area.width.saturating_sub(4).min(80);
    // Room for the draft as it grows, without swallowing the screen. The
    // text is narrower than the box by its border and padding.
    let text_w = width.saturating_sub(2 + 2 * PAD_X).max(1) as usize;
    let text_rows = (app.message_draft.chars().count() / text_w + 1).clamp(1, 8) as u16;
    let (_, content) = overlay(
        frame,
        app,
        width,
        text_rows,
        app.modal.title(),
        "Enter sends from here · Esc cancels",
    );
    frame.render_widget(
        Paragraph::new(app.message_draft.clone())
            .wrap(ratatui::widgets::Wrap { trim: false })
            .style(Style::default().fg(t.text)),
        content,
    );
    // Put the caret where typing will land.
    let before = &app.message_draft[..app.message_draft_cursor];
    let w = content.width.max(1);
    let col = (before.chars().count() as u16) % w;
    let row = (before.chars().count() as u16) / w;
    if row < content.height {
        frame.set_cursor_position((content.x + col, content.y + row));
    }
}
