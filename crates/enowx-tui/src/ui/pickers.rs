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
        Modal::Roles
            | Modal::Agents
            | Modal::Message
            | Modal::TypeSafe
            | Modal::ModelSource
            | Modal::Providers
            | Modal::Themes
    ) {
        2
    } else {
        1
    };
    let body = if app.modal == Modal::Models {
        (app.modal_items.len() as u16).max(3)
    } else {
        (app.modal_items.len() as u16 * per_row).min(20)
    };
    let hint = if app.modal != Modal::Models {
        LIST_HINT
    } else if app.modal_items.is_empty() {
        "F5 retry · F2 enter an ID · Esc cancel"
    } else {
        "Enter uses it now · F5 refresh · Esc cancel"
    };
    let (_, content) = overlay(frame, app, width, body, app.modal.title(), hint);
    if app.modal == Modal::Models {
        draw_model_list(frame, app, content);
        return;
    }
    let t = app.theme;
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
                Text::from(Line::styled(description.clone(), label))
            } else {
                Text::from(vec![
                    Line::styled(id.clone(), label),
                    Line::styled(description.clone(), Style::default().fg(t.muted)),
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

pub(super) fn draw_model_list(frame: &mut Frame, app: &mut App, area: Rect) {
    if app.discovering_models {
        frame.render_widget(
            Paragraph::new(format!(
                "Asking {} for its model list…",
                app.settings.provider
            ))
            .style(Style::default().fg(app.theme.muted)),
            area,
        );
        return;
    }
    if app.modal_items.is_empty() {
        frame.render_widget(
            Paragraph::new(if app.modal_error.is_empty() {
                "No models detected. Enter a model-list URL or use manual entry."
            } else {
                &app.modal_error
            })
            .wrap(ratatui::widgets::Wrap { trim: false })
            .style(Style::default().fg(app.theme.red)),
            area,
        );
        return;
    }
    let t = app.theme;
    let items: Vec<ListItem> = app
        .modal_items
        .iter()
        .enumerate()
        .map(|(index, (id, description))| {
            let selected = index == app.modal_cursor;
            let mut spans = vec![Span::styled(
                id.clone(),
                if selected {
                    Style::default().fg(t.accent).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(t.text)
                },
            )];
            if !description.is_empty() {
                spans.push(Span::styled(
                    format!("  {description}"),
                    Style::default().fg(t.muted),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();
    let count = items.len();
    let mut state = ListState::default().with_selected(Some(app.modal_cursor));
    frame.render_stateful_widget(selectable(List::new(items), &t), area, &mut state);
    register_list_rows(app, area, state.offset(), &vec![1; count]);
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
