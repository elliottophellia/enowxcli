use super::*;

pub(super) fn draw_composer_pane(frame: &mut Frame, app: &mut App, area: Rect) {
    let t = app.theme;
    // The field spans the pane minus the prompt marker and one trailing column.
    let width = area.width.saturating_sub(3).max(1) as usize;
    let (input, row, col) = input_rows(&app.input, app.cursor, width);
    // Two rows of breathing room by default, growing to eight so a pasted block
    // stays readable, and never past half the pane so the chat keeps its space.
    let cap = (area.height / 2).clamp(5, 11);
    // The box's two edges. The blank row that keeps its lower edge off the
    // status bar is added to the pane below, not inside the box, or the box
    // grows an empty line under the text.
    const FRAME: u16 = 2;
    /// One row between the box and the status bar: a border resting directly
    /// on the bar reads as part of it rather than as the edge of a field.
    const GAP: u16 = 1;
    let ih = (input.len().clamp(1, 8) as u16 + FRAME).min(cap);
    // Chip row above the input surfaces pasted or dropped images and their errors.
    // Attachments now render as inline `[Image N]` chips inside the field.
    let ah = if app.attach_error.is_some() { 1 } else { 0 };
    let matches = app.command_matches();
    let ph = if matches.is_empty() {
        0
    } else {
        (matches.len().min(10) as u16).min(area.height.saturating_sub(ih + ah + 1))
    };
    let rows = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(ph),
        Constraint::Length(ah),
        Constraint::Length(ih),
        Constraint::Length(GAP),
    ])
    .split(area);
    let stream = rows[0].inner(Margin {
        horizontal: 1,
        vertical: 0,
    });
    if app.blocks.is_empty() {
        app.scroll = 0;
        app.max_scroll = 0;
        draw_welcome(frame, app, stream);
    } else {
        draw_transcript(frame, app, stream);
    }
    if ph > 0 {
        let start = app
            .palette_cursor
            .saturating_sub(ph.saturating_sub(1) as usize);
        // Paint the palette background across the whole row first so the
        // highlight and idle rows both extend to the pane edge.
        frame.render_widget(
            Block::default().style(Style::default().bg(t.subtle)),
            rows[1],
        );
        let visible = matches.iter().enumerate().skip(start).take(ph as usize);
        for (offset, (index, (name, summary))) in visible.enumerate() {
            let selected = index == app.palette_cursor;
            let row = Rect::new(rows[1].x, rows[1].y + offset as u16, rows[1].width, 1);
            let style = Style::default()
                .fg(if selected { t.accent } else { t.muted })
                .bg(if selected { t.active_tab } else { t.subtle });
            frame.render_widget(
                Paragraph::new(format!(
                    "  {marker} /{name:<10} {summary}",
                    marker = if selected { "›" } else { " " },
                ))
                .style(style),
                row,
            );
        }
    }
    if let Some(error) = app.attach_error.as_deref() {
        frame.render_widget(
            Paragraph::new(error.to_owned()).style(Style::default().fg(t.red).bg(t.subtle)),
            Rect::new(area.x + 2, rows[2].y, area.width.saturating_sub(4), 1),
        );
    }
    // A box rather than a rule. The composer is a field the user types into,
    // and a border around it says that where a line above it only said
    // "something changes here". The border colours when a command is being
    // typed, so the difference is visible before Enter decides it.
    let boxed = Rect::new(
        rows[3].x + 1,
        rows[3].y,
        rows[3].width.saturating_sub(2),
        rows[3].height,
    );
    let command = app.input.starts_with('/');
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(Style::default().fg(if command {
                t.accent2
            } else if app.busy {
                t.yellow
            } else {
                t.border
            }))
            .style(Style::default().bg(t.subtle)),
        boxed,
    );
    // Only when the composer is about to do something other than send a
    // message. "MESSAGE" on every frame restated what the `❯` already says,
    // and a label that is always there stops being read.
    let label = if app.busy {
        Some((" QUEUED ", t.yellow))
    } else if !app.attachments.is_empty() {
        Some((" WITH IMAGES ", t.accent2))
    } else {
        None
    };
    if let Some((label, colour)) = label.filter(|_| boxed.width > 30) {
        frame.render_widget(
            Paragraph::new(Line::styled(
                label,
                Style::default().fg(colour).bg(t.subtle),
            )),
            Rect::new(
                boxed.x + 2,
                boxed.y,
                (label.chars().count() as u16).min(boxed.width.saturating_sub(4)),
                1,
            ),
        );
    }
    // Field spans from just after the prompt marker to one column shy of the
    // right edge so long lines land inside a visible bg strip on both sides.
    let field = Rect::new(
        boxed.x + 4,
        boxed.y + 1,
        boxed.width.saturating_sub(6),
        ih.saturating_sub(FRAME),
    );
    // The marker colours to say a command is being typed; it does not change
    // glyph. The `/` the user typed is already the first character in the
    // field, and a second one beside it reads as a typo.
    frame.render_widget(
        Paragraph::new("❯").style(
            Style::default()
                .fg(if app.input.starts_with('/') {
                    t.accent2
                } else {
                    t.accent
                })
                .bg(t.subtle)
                .add_modifier(Modifier::BOLD),
        ),
        Rect::new(boxed.x + 2, field.y, 1, field.height),
    );
    let field_w = field.width as usize;
    let offset = row.saturating_sub(field.height.saturating_sub(1) as usize);
    app.composer_field = Some(field);
    app.composer_offset = offset;
    app.composer_width = field_w;
    let painted: Vec<Line> = if app.input.is_empty() {
        vec![Line::default()]
    } else {
        input
            .iter()
            .skip(offset)
            .take(field.height as usize)
            .map(|source| {
                let clipped = crate::text::trim(source, field_w);
                let mut line = colour_chips(&clipped, &t);
                for span in &mut line.spans {
                    span.style = span.style.bg(t.subtle);
                }
                line
            })
            .collect()
    };
    frame.render_widget(Paragraph::new(painted), field);
    if app.modal == Modal::None && field.height > 0 && field.width > 0 {
        frame.set_cursor_position((
            field.x + (col as u16).min(field.width - 1),
            field.y + ((row - offset) as u16).min(field.height - 1),
        ));
    }
}

fn colour_chips(source: &str, theme: &Theme) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut rest = source;
    while let Some(start) = rest.find("[Image ") {
        if start > 0 {
            spans.push(Span::styled(
                rest[..start].to_owned(),
                Style::default().fg(theme.text),
            ));
        }
        let tail = &rest[start..];
        if let Some(end) = tail.find(']') {
            let inner = &tail[7..end];
            if !inner.is_empty() && inner.chars().all(|c| c.is_ascii_digit()) {
                spans.push(Span::styled(
                    tail[..end + 1].to_owned(),
                    Style::default()
                        .fg(theme.accent)
                        .bg(theme.active_tab)
                        .add_modifier(Modifier::BOLD),
                ));
                rest = &tail[end + 1..];
                continue;
            }
        }
        spans.push(Span::styled(
            tail[..1].to_owned(),
            Style::default().fg(theme.text),
        ));
        rest = &tail[1..];
    }
    if !rest.is_empty() {
        spans.push(Span::styled(
            rest.to_owned(),
            Style::default().fg(theme.text),
        ));
    }
    Line::from(spans)
}
