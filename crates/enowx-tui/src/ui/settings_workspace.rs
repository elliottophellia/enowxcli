use super::*;
use crate::app::{
    pages::{self, Page},
    settings_catalog::{SettingRow, SettingRowKind},
};
use ratatui::layout::{Constraint, Direction, Layout};

/// Draw the complete Settings catalog, keeping hit regions tied to stable row ids.
pub(super) fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    app.settings_sections.clear();
    app.settings_row_rects.clear();
    app.settings_toggle_rects.clear();
    app.settings_content = None;
    frame.render_widget(Clear, area);
    if area.width == 0 || area.height == 0 {
        return;
    }

    let rows = app.settings_rows();
    let selected = app.settings_row_id.clone();
    let selected_row = rows.iter().find(|row| row.id == selected);
    let selected_page = selected_row
        .map(|row| row.category)
        .unwrap_or_else(|| pages::SECTIONS[app.page_index.min(pages::SECTIONS.len() - 1)]);
    let query = app.settings_query.to_lowercase();
    let matches = |row: &SettingRow| {
        query.is_empty()
            || [
                row.category.label(),
                row.label.as_str(),
                row.id.as_str(),
                row.description.as_str(),
                row.value.as_str(),
            ]
            .iter()
            .any(|text| text.to_lowercase().contains(&query))
            || match &row.kind {
                SettingRowKind::Cycle { choices, .. } => choices.iter().any(|(stored, label)| {
                    stored.to_lowercase().contains(&query) || label.to_lowercase().contains(&query)
                }),
                _ => false,
            }
    };

    let compact_flat = area.width < 72 || area.height < pages::SECTIONS.len() as u16 + 5;
    let detail_height = if area.height >= 9 {
        7
    } else if area.height >= 4 {
        2
    } else {
        0
    };
    let (body, detail) = if detail_height > 0 {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(detail_height)])
            .split(area);
        (chunks[0], chunks[1])
    } else {
        (area, Rect::default())
    };
    let split = !compact_flat && body.width >= 72;
    let (rail, list) = if split {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(20), Constraint::Min(52)])
            .split(body);
        (Some(chunks[0]), chunks[1])
    } else {
        (None, body)
    };
    app.settings_content = Some(list);

    if let Some(rail) = rail {
        draw_rail(frame, app, rail, selected_page);
        draw_rows(
            frame,
            app,
            list,
            &rows,
            selected_page,
            &matches,
            &selected,
            false,
        );
    } else {
        draw_rows(
            frame,
            app,
            list,
            &rows,
            selected_page,
            &matches,
            &selected,
            true,
        );
    }

    if detail_height > 0 {
        draw_detail(frame, app, detail, selected_row);
    }
}

fn draw_rail(frame: &mut Frame, app: &mut App, area: Rect, selected_page: Page) {
    let block = Block::default()
        .title(if app.settings_nav {
            " Categories • focus "
        } else {
            " Categories "
        })
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.border));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut items = Vec::with_capacity(pages::SECTIONS.len());
    for (index, page) in pages::SECTIONS.iter().copied().enumerate() {
        let chosen = page == selected_page;
        let marker = if chosen {
            if app.settings_nav {
                "◆ "
            } else {
                "▸ "
            }
        } else {
            "  "
        };
        let style = if chosen {
            Style::default()
                .fg(app.theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.muted)
        };
        items.push(ListItem::new(Line::from(Span::styled(
            format!("{marker}{}", page.label()),
            style,
        ))));
        if index < inner.height as usize {
            app.settings_sections.push((
                Rect::new(
                    inner.x,
                    inner.y.saturating_add(index as u16),
                    inner.width,
                    1,
                ),
                page,
            ));
        }
    }
    frame.render_widget(List::new(items), inner);
}

fn draw_rows<F: Fn(&SettingRow) -> bool>(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    rows: &[SettingRow],
    selected_page: Page,
    matches: &F,
    selected_id: &str,
    flat: bool,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let mut lines: Vec<(Option<Page>, Option<&SettingRow>)> = Vec::new();
    if flat {
        if !app.settings_query.is_empty() && !rows.iter().any(matches) {
            lines.push((None, None));
        } else {
            for page in pages::SECTIONS {
                lines.push((Some(page), None));
                for row in rows
                    .iter()
                    .filter(|row| row.category == page && matches(row))
                {
                    lines.push((None, Some(row)));
                }
            }
        }
    } else {
        for row in rows
            .iter()
            .filter(|row| row.category == selected_page && matches(row))
        {
            lines.push((None, Some(row)));
        }
        if !app.settings_query.is_empty() && !lines.iter().any(|(_, row)| row.is_some()) {
            lines.push((None, None));
        }
    }
    let list_title = if app.settings_query.is_empty() {
        " SETTINGS "
    } else {
        " SETTINGS · SEARCH "
    };
    let inner = Block::default()
        .title(list_title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.border))
        .inner(area);
    frame.render_widget(
        Block::default()
            .title(list_title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(app.theme.border)),
        area,
    );
    let visible_height = inner.height as usize;
    if visible_height == 0 {
        return;
    }
    let selected_line = lines
        .iter()
        .position(|(_, row)| row.is_some_and(|row| row.id == selected_id));
    let start = selected_line.map_or(0, |index| {
        index.saturating_add(1).saturating_sub(visible_height)
    });
    let mut items = Vec::new();
    for (offset, (heading, row)) in lines.iter().enumerate().skip(start).take(visible_height) {
        let y = inner.y.saturating_add((offset - start) as u16);
        if let Some(page) = heading {
            let focused = *page == selected_page;
            let marker = if focused && app.settings_nav {
                "◆ "
            } else if focused {
                "▸ "
            } else {
                "  "
            };
            let style = if focused {
                Style::default()
                    .fg(app.theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(app.theme.faint)
                    .add_modifier(Modifier::BOLD)
            };
            items.push(ListItem::new(Line::from(Span::styled(
                format!("{marker}{}", page.label()),
                style,
            ))));
            app.settings_sections
                .push((Rect::new(inner.x, y, inner.width, 1), *page));
        } else if let Some(row) = row {
            let chosen = row.id == selected_id;
            let marker = if chosen {
                if app.settings_nav {
                    "◆"
                } else {
                    "▸"
                }
            } else {
                " "
            };
            let (kind_marker, value_style) = match &row.kind {
                SettingRowKind::Toggle { .. } => (
                    "[ ]",
                    if row.value.eq_ignore_ascii_case("true")
                        || row.value.eq_ignore_ascii_case("on")
                        || row.value.eq_ignore_ascii_case("enabled")
                    {
                        app.theme.green
                    } else {
                        app.theme.muted
                    },
                ),
                SettingRowKind::Cycle { .. } => ("↻", app.theme.accent2),
                SettingRowKind::Text { .. } => ("✎", app.theme.yellow),
                SettingRowKind::Action { .. } => ("›", app.theme.accent2),
                SettingRowKind::ReadOnly => ("·", app.theme.faint),
            };
            let label_style = if chosen {
                Style::default()
                    .fg(app.theme.text)
                    .add_modifier(Modifier::BOLD)
            } else if row.category == selected_page {
                Style::default().fg(app.theme.text)
            } else {
                Style::default().fg(app.theme.faint)
            };
            let content_width = inner.width.saturating_sub(2) as usize;
            let prefix = format!("{marker} {kind_marker} ");
            let label_width =
                content_width.saturating_sub(prefix.len() + row.value.chars().count() + 2);
            let label = trim(&row.label, label_width);
            let value = trim(
                &row.value,
                content_width.saturating_sub(prefix.chars().count() + label.chars().count() + 1),
            );
            let line = Line::from(vec![
                Span::styled(
                    prefix,
                    if chosen {
                        Style::default().fg(app.theme.accent)
                    } else {
                        Style::default().fg(app.theme.faint)
                    },
                ),
                Span::styled(label, label_style),
                Span::raw(" "),
                Span::styled(value, Style::default().fg(value_style)),
            ]);
            items.push(ListItem::new(line));
            let row_rect = Rect::new(inner.x, y, inner.width, 1);
            app.settings_row_rects.push((row_rect, row.id.clone()));
            if matches!(&row.kind, SettingRowKind::Toggle { .. }) {
                let control_width = 3.min(inner.width);
                let control_x = inner.x.saturating_add(marker.chars().count() as u16 + 1);
                app.settings_toggle_rects
                    .push((Rect::new(control_x, y, control_width, 1), row.id.clone()));
            }
        } else {
            items.push(ListItem::new(Line::from(Span::styled(
                if app.settings_query.is_empty() {
                    ""
                } else {
                    "  No settings match this query"
                },
                Style::default().fg(app.theme.faint),
            ))));
        }
    }
    frame.render_widget(List::new(items), inner);
}

fn draw_detail(frame: &mut Frame, app: &App, area: Rect, row: Option<&SettingRow>) {
    let border = Style::default().fg(app.theme.border);
    let block = Block::default()
        .title(" Details ")
        .borders(Borders::ALL)
        .border_style(border);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let mut lines = Vec::new();
    if let Some(row) = row {
        lines.push(Line::from(vec![
            Span::styled(
                row.label.as_str(),
                Style::default()
                    .fg(app.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(row.value.as_str(), Style::default().fg(app.theme.text)),
        ]));
        lines.push(Line::from(Span::styled(
            row.description.as_str(),
            Style::default().fg(app.theme.muted),
        )));
        lines.push(Line::from(Span::styled(
            row.category.label(),
            Style::default().fg(app.theme.faint),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "Settings workspace",
            Style::default().fg(app.theme.accent),
        )));
    }
    let message = if !app.modal_error.is_empty() {
        &app.modal_error
    } else {
        &app.status
    };
    if !message.is_empty() && inner.height > 2 {
        let secret = ["api_key", "auth", "password", "dsn", "passphrase", "token"]
            .iter()
            .any(|name| message.to_ascii_lowercase().contains(name));
        let safe_message = if secret {
            "credential updated"
        } else {
            message
        };
        lines.push(Line::from(Span::styled(
            safe_message,
            Style::default().fg(if app.modal_error.is_empty() {
                app.theme.faint
            } else {
                app.theme.red
            }),
        )));
    }
    if inner.height > 3 {
        let hint = if app.settings_nav {
            "↑↓ category · Enter/Right settings · Tab settings · Esc rows".to_owned()
        } else {
            let actions = row.and_then(|row| match &row.kind {
                SettingRowKind::Action { shortcuts, .. }
                    if app.settings_query.is_empty() && !shortcuts.is_empty() =>
                {
                    Some(
                        shortcuts
                            .iter()
                            .map(|(key, _)| match key {
                                crossterm::event::KeyCode::Delete => "Delete disconnect",
                                crossterm::event::KeyCode::Char('d') => "d default/disconnect",
                                crossterm::event::KeyCode::Char('m') => "m model",
                                crossterm::event::KeyCode::Char('c') => "c setup",
                                crossterm::event::KeyCode::Char('t') => "t tools",
                                crossterm::event::KeyCode::Char('r') => "Ctrl+R refresh",
                                crossterm::event::KeyCode::Char('n') => "Ctrl+N add",
                                _ => "",
                            })
                            .filter(|hint| !hint.is_empty())
                            .collect::<Vec<_>>()
                            .join(" · "),
                    )
                }
                _ => None,
            });
            actions.map_or_else(|| "↑↓ move · Enter open/apply · Space toggle · PgUp/PgDn category · Tab categories · type to search · Esc chat".to_owned(), |actions| format!("↑↓ move · Enter open · {actions} · type to search · Esc chat"))
        };
        lines.push(Line::from(Span::styled(
            hint,
            Style::default().fg(app.theme.faint),
        )));
    }
    frame.render_widget(
        Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: true }),
        inner,
    );
}
