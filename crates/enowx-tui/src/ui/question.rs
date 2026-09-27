//! The questions the agent is waiting on, in a box fixed above the composer
//! where the command list goes: which question of how many, the question,
//! its options with any notes under them, the "Other" row, and in the bottom
//! edge, the keys that apply right now.

use super::*;
use crate::app::question::PendingQuestion;

/// Rows the box takes in a column `width` wide.
pub(super) fn question_height(app: &App, width: u16) -> u16 {
    let Some(pending) = app.question.as_ref() else {
        return 0;
    };
    lines(pending, text_width(width)).len() as u16 + 2 + 2 * PAD_Y
}

/// Columns for text inside the box: its edges, the padding, and the marker
/// column with its space.
fn text_width(width: u16) -> usize {
    width.saturating_sub(2 + 2 * PAD_X + 2).max(1) as usize
}

/// What one row of the box holds, before it is styled.
enum Row {
    /// Which question this is, when there are several.
    Strip,
    Blank,
    Question(String, bool),
    Option(usize),
    Note(usize),
    Other,
}

fn lines(pending: &PendingQuestion, text_w: usize) -> Vec<Row> {
    let mut rows = Vec::new();
    if pending.questions.len() > 1 {
        rows.push(Row::Strip);
        rows.push(Row::Blank);
    }
    for (index, line) in textwrap::wrap(&pending.question().question, text_w)
        .into_iter()
        .enumerate()
    {
        rows.push(Row::Question(line.into_owned(), index == 0));
    }
    rows.push(Row::Blank);
    for option in 0..pending.question().options.len() {
        rows.push(Row::Option(option));
        let note = &pending.notes[pending.current][option];
        let editing = pending.editing.as_ref().is_some_and(|(o, _)| *o == option);
        if editing || !note.trim().is_empty() {
            rows.push(Row::Note(option));
        }
    }
    rows.push(Row::Other);
    rows
}

pub(super) fn draw_question(frame: &mut Frame, app: &mut App, area: Rect) {
    app.question_rows.clear();
    let Some(pending) = app.question.as_ref() else {
        return;
    };
    let t = app.theme;
    let count = pending.questions.len();
    // The accent edge of a window that has the keyboard, like every overlay.
    let inner = panel_box(frame, area, t.accent2, t.panel);
    let mut title = vec![Span::styled(
        if count > 1 {
            format!("QUESTION {} OF {count}", pending.current + 1)
        } else {
            "QUESTION".to_owned()
        },
        Style::default().fg(t.accent2).add_modifier(Modifier::BOLD),
    )];
    title.push(Span::styled(
        format!(" · {}", pending.agent),
        Style::default().fg(t.muted),
    ));
    box_title(frame, area, title, t.panel);
    box_hint(frame, area, &hint(pending), t.muted, t.panel);

    let content = padded(inner, true);
    if content.width < 6 || content.height == 0 {
        return;
    }
    let text_w = text_width(area.width);
    let question = pending.question();
    // Descriptions start on one column, past the longest label, so the
    // options read as a table rather than a ragged list.
    let label_w = question
        .options
        .iter()
        .map(|choice| choice.label.chars().count())
        .max()
        .unwrap_or(0)
        .max("Other".len())
        .min(text_w / 2);
    let tick_w = if question.multiple { 4 } else { 0 };
    let current = pending.current;
    let cursor = pending.cursor();
    let mut rows = Vec::new();
    let mut caret: Option<(u16, u16)> = None;
    // Rows past the box's bottom are left out.
    for (y, row) in (content.y..content.bottom()).zip(lines(pending, text_w)) {
        let at = Rect::new(content.x, y, content.width, 1);
        match row {
            Row::Strip => {
                let mut spans = Vec::new();
                for index in 0..count {
                    let name = match pending.questions[index].header.trim() {
                        "" => format!("Question {}", index + 1),
                        header => header.to_owned(),
                    };
                    let answered = !pending.reply(index).is_empty();
                    let style = if index == current {
                        Style::default().fg(t.accent2).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(t.muted)
                    };
                    if index > 0 {
                        spans.push(Span::styled("   ", style));
                    }
                    spans.push(Span::styled(format!("{} {name}", index + 1), style));
                    if answered {
                        spans.push(Span::styled(" ✓", Style::default().fg(t.green)));
                    }
                }
                // On the text column: the marker column is left blank.
                frame.render_widget(
                    Paragraph::new(Line::from(spans)),
                    Rect::new(at.x + 2, y, at.width.saturating_sub(2), 1),
                );
            }
            Row::Blank => {}
            Row::Question(text, first) => {
                frame.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled(
                            if first { "? " } else { "  " },
                            Style::default().fg(t.accent2).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            text,
                            Style::default().fg(t.text).add_modifier(Modifier::BOLD),
                        ),
                    ])),
                    at,
                );
            }
            Row::Option(option) => {
                let choice = &question.options[option];
                let highlighted = option == cursor;
                let tick = if question.multiple {
                    if pending.ticked[current][option] {
                        "[x] "
                    } else {
                        "[ ] "
                    }
                } else {
                    ""
                };
                let label = format!(
                    "{}  {tick}{:<label_w$}",
                    option + 1,
                    trim(&choice.label, label_w.max(1))
                );
                let room = text_w.saturating_sub(label.chars().count() + 3);
                let mut spans = vec![
                    Span::styled(
                        if highlighted { "› " } else { "  " },
                        Style::default().fg(t.accent2),
                    ),
                    Span::styled(
                        label,
                        if highlighted {
                            Style::default().fg(t.accent2).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(t.text)
                        },
                    ),
                ];
                if !choice.description.trim().is_empty() && room > 8 {
                    spans.push(Span::styled(
                        format!("   {}", trim(choice.description.trim(), room)),
                        Style::default().fg(t.muted),
                    ));
                }
                band(frame, at, spans, highlighted, &t);
                rows.push((Rect::new(inner.x, y, inner.width, 1), option));
            }
            Row::Note(option) => {
                let editing = pending.editing.as_ref().is_some_and(|(o, _)| *o == option);
                let note = &pending.notes[current][option];
                // Under the option's label: past the number and the tick.
                let indent = 2 + 3 + tick_w;
                let shown = trim(note, text_w.saturating_sub(indent + 6));
                frame.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::raw(" ".repeat(indent)),
                        Span::styled(
                            "note: ",
                            Style::default().fg(if editing { t.accent2 } else { t.muted }),
                        ),
                        Span::styled(
                            shown.clone(),
                            Style::default().fg(if editing { t.text } else { t.muted }),
                        ),
                    ])),
                    at,
                );
                if editing {
                    let x = at.x + (indent + 6 + shown.chars().count()) as u16;
                    caret = Some((x.min(at.right().saturating_sub(1)), y));
                }
            }
            Row::Other => {
                let row = pending.other_row();
                let highlighted = cursor == row;
                let typed = &pending.other[current];
                let number = format!("{}  {}", row + 1, " ".repeat(tick_w));
                let mut spans = vec![
                    Span::styled(
                        if highlighted { "› " } else { "  " },
                        Style::default().fg(t.accent2),
                    ),
                    Span::styled(
                        number.clone(),
                        if highlighted {
                            Style::default().fg(t.accent2).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(t.text)
                        },
                    ),
                ];
                let lead = 2 + number.chars().count();
                if typed.is_empty() {
                    spans.push(Span::styled(
                        format!("{:<label_w$}", "Other"),
                        if highlighted {
                            Style::default().fg(t.accent2).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(t.text)
                        },
                    ));
                    spans.push(Span::styled(
                        "   type your own answer",
                        Style::default().fg(t.muted),
                    ));
                    if highlighted {
                        caret = Some((at.x + lead as u16, y));
                    }
                } else {
                    let shown = tail(typed, text_w.saturating_sub(lead + 7));
                    spans.push(Span::styled("Other: ", Style::default().fg(t.muted)));
                    spans.push(Span::styled(shown.clone(), Style::default().fg(t.text)));
                    if highlighted {
                        let x = at.x + (lead + 7 + shown.chars().count()) as u16;
                        caret = Some((x.min(at.right().saturating_sub(1)), y));
                    }
                }
                band(frame, at, spans, highlighted, &t);
                rows.push((Rect::new(inner.x, y, inner.width, 1), row));
            }
        }
    }
    app.question_rows = rows;
    if app.modal == Modal::None {
        if let Some(position) = caret {
            frame.set_cursor_position(position);
        }
    }
}

/// A row with the selection band when highlighted.
fn band(frame: &mut Frame, at: Rect, spans: Vec<Span<'static>>, on: bool, t: &Theme) {
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(if on {
            t.active_tab
        } else {
            t.panel
        })),
        at,
    );
}

/// The end of `text` in `width` columns: what is being typed is at its end.
fn tail(text: &str, width: usize) -> String {
    let count = text.chars().count();
    if count <= width {
        return text.to_owned();
    }
    let keep: String = text.chars().skip(count - width.saturating_sub(1)).collect();
    format!("…{keep}")
}

/// The keys that apply right now, for the box's bottom edge.
fn hint(pending: &PendingQuestion) -> String {
    if pending.editing.is_some() {
        return "type the note · Enter save · Esc cancel".into();
    }
    let several = pending.questions.len() > 1;
    let last = pending.current + 1 == pending.questions.len();
    let enter = if !several || last {
        "Enter answer"
    } else {
        "Enter next"
    };
    let mut keys: Vec<&str> = Vec::new();
    if pending.on_other() {
        keys.push("type your answer");
        keys.push("↑↓ choose");
    } else if pending.question().multiple {
        keys.push("↑↓ move");
        keys.push("Space tick");
        keys.push("n note");
    } else {
        keys.push("↑↓ choose");
        keys.push("n note");
    }
    if several {
        keys.push("←→ question");
    }
    keys.push(enter);
    keys.push("Esc stop");
    keys.join(" · ")
}
