use super::*;

pub(super) fn draw_settings(frame: &mut Frame, app: &App, area: Rect) {
    let single = matches!(
        app.modal,
        Modal::ModelUrl | Modal::ProviderKey | Modal::TypeSafeKey
    );
    let width = area.width.saturating_sub(2).min(72);
    // Content rows: a single field is its label, its value, a blank row and
    // the note; the full form shows six fields of three rows and the note.
    let height = if single { 4 } else { 20 };
    // Keys go in the box's bottom edge; the sentence explaining the field, or
    // the error when there is one, keeps one row inside.
    let (keys, note) = if app.modal == Modal::ModelUrl {
        (
            "Enter detect · Ctrl+U clear · Esc cancel",
            "Full JSON endpoint URL; metadata depends on what it returns.",
        )
    } else if app.modal == Modal::TypeSafeKey {
        (
            "Enter save · Ctrl+U clear · Esc cancel",
            "An empty key turns TypeSafe off. TYPESAFE_API_KEY is read too.",
        )
    } else if app.modal == Modal::ProviderKey {
        (
            "Enter connect · Ctrl+U clear · Esc cancel",
            "Provider endpoints are preset. Add a model after connecting.",
        )
    } else if SETTINGS_FIELDS.get(app.modal_cursor) == Some(&SettingsField::Theme) {
        (
            "Enter / F2 choose theme · Tab field · Esc cancel",
            "Switch between the five palettes.",
        )
    } else if width < 44 {
        ("Tab field · Enter save · Esc cancel", "F5 adds a model.")
    } else {
        (
            "Tab / ↑↓ field · Ctrl+U clear · Enter save · Esc cancel",
            "F5 adds a model: auto detect from a URL, or enter one by hand.",
        )
    };
    let title = match app.modal {
        Modal::ModelUrl => "MODEL-LIST URL".to_owned(),
        Modal::TypeSafeKey => "TYPESAFE KEY".to_owned(),
        Modal::ProviderKey => format!("{} KEY", app.settings.provider.to_uppercase()),
        _ => "SETTINGS".to_owned(),
    };
    let (_, content) = overlay(frame, app, width, height, &title, keys);
    if content.height == 0 || content.width < 4 {
        return;
    }
    // The last content row holds the note; a blank row keeps it off the
    // fields above.
    let note_row = Rect::new(content.x, content.bottom() - 1, content.width, 1);
    let fields = Rect::new(
        content.x,
        content.y,
        content.width,
        content.height.saturating_sub(2),
    );

    let visible = if single {
        1
    } else {
        (fields.height / 3).max(1) as usize
    };
    let start = if app.modal == Modal::ModelUrl {
        3
    } else if matches!(app.modal, Modal::ProviderKey | Modal::TypeSafeKey) {
        2
    } else {
        app.modal_cursor.saturating_sub(visible - 1)
    };
    let labels = crate::modal::SETTINGS_LABELS;
    for (index, field) in SETTINGS_FIELDS
        .iter()
        .copied()
        .enumerate()
        .skip(start)
        .take(visible)
    {
        let active = index == app.modal_cursor;
        let raw = app.settings.value(field);
        let shown = if field == SettingsField::ApiKey {
            "•".repeat(raw.chars().count())
        } else if field == SettingsField::Theme {
            format!("{} (Enter/F2 to change)", Theme::find(raw).label)
        } else {
            raw.to_owned()
        };
        let y = fields.y + ((index - start) * 3) as u16;
        // Label and value both have to fit: a label with its value cut off
        // below the box reads as an empty field.
        if y + 1 >= fields.bottom() {
            break;
        }
        // Marker on the overlay's column 2, the label and its value both on
        // column 4, so a field reads as one aligned block.
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    if active { "› " } else { "  " },
                    Style::default().fg(app.theme.accent),
                ),
                Span::styled(
                    labels[index],
                    if active {
                        Style::default()
                            .fg(app.theme.accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(app.theme.muted)
                    },
                ),
            ])),
            Rect::new(fields.x, y, fields.width, 1),
        );
        let field_area = Rect::new(fields.x + 2, y + 1, fields.width.saturating_sub(2), 1);
        let cursor_column = if !active {
            0
        } else if field == SettingsField::ApiKey {
            raw[..app.field_cursor].chars().count()
        } else {
            raw[..app.field_cursor].width()
        };
        let offset = if active {
            cursor_column.saturating_sub(field_area.width.saturating_sub(1) as usize)
        } else {
            0
        };
        let content_text = if shown.is_empty() { "(empty)" } else { &shown };
        frame.render_widget(
            Paragraph::new(content_text)
                .scroll((0, offset.min(u16::MAX as usize) as u16))
                .style(Style::default().fg(if shown.is_empty() {
                    app.theme.faint
                } else {
                    app.theme.text
                })),
            field_area,
        );
        if active && field_area.width > 0 {
            frame.set_cursor_position((
                field_area.x + (cursor_column - offset) as u16,
                field_area.y,
            ));
        }
    }
    let (note, colour) = if app.modal_error.is_empty() {
        (note, app.theme.faint)
    } else {
        (app.modal_error.as_str(), app.theme.red)
    };
    frame.render_widget(
        Paragraph::new(trim(note, note_row.width as usize)).style(Style::default().fg(colour)),
        note_row,
    );
}
