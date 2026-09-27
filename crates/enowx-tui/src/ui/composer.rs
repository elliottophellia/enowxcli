use super::*;

/// The main column: the chat box, the command palette when one is being
/// typed, and the composer, stacked on the same two edges.
pub(super) fn draw_main_column(frame: &mut Frame, app: &mut App, area: Rect) {
    // The field starts on the text column (border, padding, the prompt and a
    // space) and stops at the padding on the right.
    let field_w = area.width.saturating_sub(COMPOSER_LEFT + 1 + PAD_X).max(1) as usize;
    let (input, row, col) = input_rows(&app.input, app.cursor, field_w);
    // The box's two edges.
    const FRAME: u16 = 2;
    // One row of text by default, growing to eight so a pasted block stays
    // readable, and never past half the column so the chat keeps its space.
    let cap = (area.height / 2).clamp(FRAME + 1, 10);
    let ih = (input.len().clamp(1, 8) as u16 + FRAME).min(cap);
    let matches = app.command_matches();
    // The palette is a box of its own between the chat and the composer, so
    // opening it shortens the chat box rather than painting over its rows.
    // It never takes the chat below three rows, and it needs at least one
    // row of its own to be worth drawing.
    let ph = if matches.is_empty() {
        0
    } else {
        let wanted = matches.len().min(10) as u16 + FRAME;
        let room = area.height.saturating_sub(ih + 3);
        if room > FRAME {
            wanted.min(room)
        } else {
            0
        }
    };
    let chat_h = area.height.saturating_sub(ih + ph);
    draw_chat_box(frame, app, Rect::new(area.x, area.y, area.width, chat_h));
    if ph > 0 {
        draw_palette(
            frame,
            app,
            Rect::new(area.x, area.y + chat_h, area.width, ph),
            &matches,
        );
    }
    draw_composer_box(
        frame,
        app,
        Rect::new(area.x, area.y + chat_h + ph, area.width, ih),
        &input,
        row,
        col,
    );
}

/// Columns from the composer's left edge to its text: the border, the padding,
/// the prompt and a space — the same text column as the transcript above it.
const COMPOSER_LEFT: u16 = 1 + PAD_X + 2;

/// The conversation, in a box titled with the project and the session.
///
/// The title replaces the header box that used to sit above everything: four
/// rows of chrome to print one word.
fn draw_chat_box(frame: &mut Frame, app: &mut App, area: Rect) {
    if area.height < 3 {
        return;
    }
    let t = app.theme;
    let inner = panel_box(frame, area, t.border, t.panel);
    let mut title: Vec<Span<'static>> = Vec::new();
    if let Some(viewing) = app.viewing.as_ref() {
        // Not the conversation: say whose work is on screen, where the
        // conversation's own name would otherwise be.
        title.push(Span::styled(
            viewing.agent.clone(),
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        ));
        title.push(Span::styled(
            " · sub-agent transcript",
            Style::default().fg(t.muted),
        ));
    } else {
        let project = app
            .workspace
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        title.push(Span::styled(
            project,
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        ));
        if !app.title.is_empty() {
            title.push(Span::styled(" · ", Style::default().fg(t.faint)));
            title.push(Span::styled(
                crate::text::trim(&app.title, 60),
                Style::default().fg(t.muted),
            ));
        }
    }
    box_title(frame, area, title, t.panel);
    // Padded on every side, so neither the first nor the last line of the
    // conversation rests against the box. Markers land on the box's column 3
    // and text on column 5, the composer's prompt and field below.
    let stream = padded(inner, true);
    if stream.width == 0 || stream.height == 0 {
        return;
    }
    if app.blocks.is_empty() {
        app.scroll = 0;
        app.max_scroll = 0;
        draw_welcome(frame, app, stream);
    } else {
        draw_transcript(frame, app, stream);
    }
}

fn draw_palette(frame: &mut Frame, app: &mut App, area: Rect, matches: &[(&str, &str)]) {
    let t = app.theme;
    app.composer_palette = Some(area);
    let inner = panel_box(frame, area, t.border, t.panel);
    box_title(
        frame,
        area,
        vec![Span::styled(
            "COMMANDS",
            Style::default().fg(t.muted).add_modifier(Modifier::BOLD),
        )],
        t.panel,
    );
    let rows = inner.height as usize;
    let start = app.palette_cursor.saturating_sub(rows.saturating_sub(1));
    let lead = " ".repeat(PAD_X as usize);
    for (offset, (index, (name, summary))) in matches
        .iter()
        .enumerate()
        .skip(start)
        .take(rows)
        .enumerate()
    {
        let selected = index == app.palette_cursor;
        // The marker on the marker column and the command on the text
        // column, like every other row in the column. The selection band
        // runs the full inner width.
        let line = Line::from(vec![
            Span::styled(
                format!("{lead}{} ", if selected { "›" } else { " " }),
                Style::default().fg(t.accent),
            ),
            Span::styled(
                format!("/{name:<11}"),
                Style::default().fg(if selected { t.accent } else { t.text }),
            ),
            Span::styled(format!(" {summary}"), Style::default().fg(t.muted)),
        ]);
        let row = Rect::new(inner.x, inner.y + offset as u16, inner.width, 1);
        frame.render_widget(
            Paragraph::new(line).style(Style::default().bg(if selected {
                t.active_tab
            } else {
                t.panel
            })),
            row,
        );
        app.composer_palette_rows.push((row, index));
    }
}

fn draw_composer_box(
    frame: &mut Frame,
    app: &mut App,
    boxed: Rect,
    input: &[String],
    row: usize,
    col: usize,
) {
    let t = app.theme;
    // A box rather than a rule. The composer is a field the user types into,
    // and a border around it says that where a line above it only said
    // "something changes here". The border colours when a command is being
    // typed, so the difference is visible before Enter decides it.
    let command = app.input.starts_with('/');
    panel_box(
        frame,
        boxed,
        if command {
            t.accent2
        } else if app.busy {
            t.yellow
        } else {
            t.border
        },
        t.subtle,
    );
    // Set into the top edge, and only when the composer is about to do
    // something other than send a message. "MESSAGE" on every frame restated
    // what the `❯` already says, and a label that is always there stops being
    // read. An attachment error outranks both: it is why the send will fail.
    let label: Option<(String, ratatui::style::Color)> =
        if let Some(error) = app.attach_error.as_deref() {
            Some((error.to_owned(), t.red))
        } else if app.busy {
            Some(("QUEUED".to_owned(), t.yellow))
        } else if !app.attachments.is_empty() {
            Some(("WITH IMAGES".to_owned(), t.accent2))
        } else {
            None
        };
    if let Some((label, colour)) = label.filter(|_| boxed.width > 30) {
        box_title(
            frame,
            boxed,
            vec![Span::styled(
                label,
                Style::default().fg(colour).add_modifier(Modifier::BOLD),
            )],
            t.subtle,
        );
    }
    if boxed.height < 3 {
        return;
    }
    let field = Rect::new(
        boxed.x + COMPOSER_LEFT,
        boxed.y + 1,
        boxed.width.saturating_sub(COMPOSER_LEFT + 1 + PAD_X),
        boxed.height.saturating_sub(2),
    );
    // The marker colours to say a command is being typed; it does not change
    // glyph. The `/` the user typed is already the first character in the
    // field, and a second one beside it reads as a typo.
    frame.render_widget(
        Paragraph::new("❯").style(
            Style::default()
                .fg(if command { t.accent2 } else { t.accent })
                .bg(t.subtle)
                .add_modifier(Modifier::BOLD),
        ),
        Rect::new(boxed.x + 1 + PAD_X, field.y, 1, 1),
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
