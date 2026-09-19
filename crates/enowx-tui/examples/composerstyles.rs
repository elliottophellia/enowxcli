//! Ten composer styles, side by side, for choosing one.
//!
//! Run it in another terminal and look:
//!
//!     cargo run -p enowx-tui --example composerstyles
//!     cargo run -p enowx-tui --example composerstyles -- 3   # just one, big
//!
//! Each is drawn at the real width with the real theme, in the three states
//! that matter: empty, holding a message, and holding a command. A style that
//! only looks right when full is not a style, and the empty state is what the
//! user sees most.

use enowx_tui::theme::{Theme, THEMES};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

struct Style_ {
    name: &'static str,
    /// What it is trading away, so the choice is between costs and not just
    /// looks.
    note: &'static str,
    rows: u16,
    draw: fn(&mut Frame, Rect, &Theme, &str, bool),
}

const STYLES: [Style_; 10] = [
    Style_ {
        name: "1 · Rule",
        note: "what enx has now: a rule, a marker, a gap. 3 rows.",
        rows: 3,
        draw: draw_rule,
    },
    Style_ {
        name: "2 · Bare",
        note: "no rule at all — the blank row above is the separation. 2 rows.",
        rows: 2,
        draw: draw_bare,
    },
    Style_ {
        name: "3 · Boxed",
        note: "a full border, like resterm's editor pane. 3 rows, reads as a field.",
        rows: 3,
        draw: draw_boxed,
    },
    Style_ {
        name: "4 · Bar",
        note: "a bar down the left, matching how a user message is marked. 2 rows.",
        rows: 2,
        draw: draw_bar,
    },
    Style_ {
        name: "5 · Filled",
        note: "a lighter band, no rule. The surface is the separation. 2 rows.",
        rows: 2,
        draw: draw_filled,
    },
    Style_ {
        name: "6 · Badge",
        note: "a reversed agent badge in front, so who answers is at the cursor.",
        rows: 2,
        draw: draw_badge,
    },
    Style_ {
        name: "7 · Corner",
        note: "rule with a corner, so the composer reads as attached below the chat.",
        rows: 3,
        draw: draw_corner,
    },
    Style_ {
        name: "8 · Inline",
        note: "no separation at all: the composer is the next line of transcript.",
        rows: 1,
        draw: draw_inline,
    },
    Style_ {
        name: "9 · Double",
        note: "a heavier rule, the way vim splits panes. 3 rows.",
        rows: 3,
        draw: draw_double,
    },
    Style_ {
        name: "10 · Status-hug",
        note: "composer and status bar share one surface, with no gap between.",
        rows: 2,
        draw: draw_hug,
    },
];

/// Everything below draws into `area`, whose height is the style's `rows`.
/// `text` is the input; `command` says whether it starts with `/`.
fn marker(t: &Theme, command: bool) -> Span<'static> {
    Span::styled(
        "❯ ",
        Style::default()
            .fg(if command { t.accent2 } else { t.accent })
            .add_modifier(Modifier::BOLD),
    )
}

fn typed(t: &Theme, text: &str) -> Span<'static> {
    if text.is_empty() {
        Span::styled(
            "Ask anything, or / for a command",
            Style::default().fg(t.faint),
        )
    } else {
        Span::styled(text.to_owned(), Style::default().fg(t.text))
    }
}

fn fill(frame: &mut Frame, area: Rect, colour: Color) {
    frame.render_widget(Block::default().style(Style::default().bg(colour)), area);
}

fn draw_rule(frame: &mut Frame, area: Rect, t: &Theme, text: &str, command: bool) {
    fill(frame, area, t.subtle);
    frame.render_widget(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(t.border)),
        area,
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![marker(t, command), typed(t, text)]))
            .style(Style::default().bg(t.subtle)),
        Rect::new(area.x, area.y + 1, area.width, 1),
    );
}

fn draw_bare(frame: &mut Frame, area: Rect, t: &Theme, text: &str, command: bool) {
    frame.render_widget(
        Paragraph::new(Line::from(vec![marker(t, command), typed(t, text)])),
        Rect::new(area.x, area.y + 1, area.width, 1),
    );
}

fn draw_boxed(frame: &mut Frame, area: Rect, t: &Theme, text: &str, command: bool) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(if command { t.accent2 } else { t.border }));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![marker(t, command), typed(t, text)])),
        inner,
    );
}

fn draw_bar(frame: &mut Frame, area: Rect, t: &Theme, text: &str, command: bool) {
    let row = Rect::new(area.x, area.y + 1, area.width, 1);
    fill(frame, row, t.subtle);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "▌ ",
                Style::default().fg(if command { t.accent2 } else { t.accent }),
            ),
            typed(t, text),
        ]))
        .style(Style::default().bg(t.subtle)),
        row,
    );
}

fn draw_filled(frame: &mut Frame, area: Rect, t: &Theme, text: &str, command: bool) {
    let row = Rect::new(area.x, area.y + 1, area.width, 1);
    fill(frame, row, t.active_tab);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            marker(t, command),
            typed(t, text),
        ]))
        .style(Style::default().bg(t.active_tab)),
        row,
    );
}

fn draw_badge(frame: &mut Frame, area: Rect, t: &Theme, text: &str, command: bool) {
    let row = Rect::new(area.x, area.y + 1, area.width, 1);
    fill(frame, row, t.subtle);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " router ",
                Style::default()
                    .fg(t.panel)
                    .bg(if command { t.accent2 } else { t.accent })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            typed(t, text),
        ]))
        .style(Style::default().bg(t.subtle)),
        row,
    );
}

fn draw_corner(frame: &mut Frame, area: Rect, t: &Theme, text: &str, command: bool) {
    fill(frame, area, t.subtle);
    let rule = format!("╰{}", "─".repeat(area.width.saturating_sub(1) as usize));
    frame.render_widget(
        Paragraph::new(Line::styled(rule, Style::default().fg(t.border)))
            .style(Style::default().bg(t.subtle)),
        Rect::new(area.x, area.y, area.width, 1),
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            marker(t, command),
            typed(t, text),
        ]))
        .style(Style::default().bg(t.subtle)),
        Rect::new(area.x, area.y + 1, area.width, 1),
    );
}

fn draw_inline(frame: &mut Frame, area: Rect, t: &Theme, text: &str, command: bool) {
    frame.render_widget(
        Paragraph::new(Line::from(vec![marker(t, command), typed(t, text)])),
        Rect::new(area.x, area.y, area.width, 1),
    );
}

fn draw_double(frame: &mut Frame, area: Rect, t: &Theme, text: &str, command: bool) {
    fill(frame, area, t.subtle);
    frame.render_widget(
        Paragraph::new(Line::styled(
            "━".repeat(area.width as usize),
            Style::default().fg(t.border),
        ))
        .style(Style::default().bg(t.subtle)),
        Rect::new(area.x, area.y, area.width, 1),
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![marker(t, command), typed(t, text)]))
            .style(Style::default().bg(t.subtle)),
        Rect::new(area.x, area.y + 1, area.width, 1),
    );
}

fn draw_hug(frame: &mut Frame, area: Rect, t: &Theme, text: &str, command: bool) {
    fill(frame, area, t.subtle);
    frame.render_widget(
        Paragraph::new(Line::from(vec![marker(t, command), typed(t, text)]))
            .style(Style::default().bg(t.subtle)),
        Rect::new(area.x, area.y, area.width, 1),
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " READY ",
                Style::default()
                    .fg(t.panel)
                    .bg(t.green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" router", Style::default().fg(t.accent)),
        ]))
        .style(Style::default().bg(t.subtle)),
        Rect::new(area.x, area.y + 1, area.width, 1),
    );
}

/// A few lines of conversation above each sample, because a composer is
/// judged by how it separates from what is above it.
fn draw_context(frame: &mut Frame, area: Rect, t: &Theme) {
    let lines = vec![
        Line::from(vec![
            Span::styled("▌ ", Style::default().fg(t.accent).bg(t.subtle)),
            Span::styled(
                "perbagus portofolio saya",
                Style::default()
                    .fg(t.text)
                    .bg(t.subtle)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::default(),
        Line::styled(
            "Portofolio statis satu halaman. Murni kerja frontend.",
            Style::default().fg(t.text),
        ),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

/// Render into a fixed-size buffer and return it as text. Used by the tests
/// that check these samples draw what they claim, since the example itself
/// needs a real terminal.
pub fn render(width: u16, height: u16, only: Option<usize>) -> Vec<String> {
    use ratatui::backend::TestBackend;
    let theme = &THEMES[0];
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("backend");
    terminal
        .draw(|frame| {
            let area = frame.area();
            frame.render_widget(
                Block::default().style(Style::default().bg(theme.panel).fg(theme.text)),
                area,
            );
            match only {
                Some(n) if n >= 1 && n <= STYLES.len() => draw_one(frame, area, theme, n - 1),
                _ => draw_all(frame, area, theme),
            }
        })
        .expect("draw");
    terminal
        .backend()
        .buffer()
        .content()
        .chunks(width as usize)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect())
        .collect()
}

fn main() -> std::io::Result<()> {
    let only: Option<usize> = std::env::args().nth(1).and_then(|a| a.parse().ok());
    let theme = &THEMES[0];

    let mut terminal = ratatui::init();
    let result = (|| -> std::io::Result<()> {
        loop {
            terminal.draw(|frame| {
                let area = frame.area();
                frame.render_widget(
                    Block::default().style(Style::default().bg(theme.panel).fg(theme.text)),
                    area,
                );
                match only {
                    Some(n) if n >= 1 && n <= STYLES.len() => draw_one(frame, area, theme, n - 1),
                    _ => draw_all(frame, area, theme),
                }
            })?;
            if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                use crossterm::event::KeyCode;
                if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                    return Ok(());
                }
            }
        }
    })();
    ratatui::restore();
    result
}

/// All ten, stacked, each with a caption.
fn draw_all(frame: &mut Frame, area: Rect, theme: &Theme) {
    let width = area.width.min(74);
    let mut y = area.y;
    frame.render_widget(
        Paragraph::new(Line::styled(
            "  composer styles · number to see one full size · q to quit",
            Style::default().fg(theme.faint),
        )),
        Rect::new(area.x, y, area.width, 1),
    );
    y += 2;

    for (index, style) in STYLES.iter().enumerate() {
        let needed = style.rows + 2;
        if y + needed > area.bottom() {
            frame.render_widget(
                Paragraph::new(Line::styled(
                    "  … make the window taller, or pass a number",
                    Style::default().fg(theme.faint),
                )),
                Rect::new(area.x, y, area.width, 1),
            );
            return;
        }
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!("  {:<16}", style.name),
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(style.note, Style::default().fg(theme.faint)),
            ])),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;
        let sample = Rect::new(area.x + 2, y, width, style.rows);
        let _ = index;
        (style.draw)(frame, sample, theme, "lanjutkan", false);
        y += style.rows + 1;
    }
}

/// One style, full width, in its three states.
fn draw_one(frame: &mut Frame, area: Rect, theme: &Theme, index: usize) {
    let style = &STYLES[index];
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("  {}  ", style.name),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(style.note, Style::default().fg(theme.faint)),
        ])),
        Rect::new(area.x, area.y, area.width, 1),
    );

    let width = area.width.saturating_sub(4);
    let mut y = area.y + 2;
    for (label, text, command) in [
        ("empty", "", false),
        ("a message", "perbaiki bug kontras di footer", false),
        ("a command", "/model", true),
    ] {
        frame.render_widget(
            Paragraph::new(Line::styled(
                format!("  {label}"),
                Style::default().fg(theme.faint),
            )),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;
        // Conversation above it, because that is what it separates from.
        draw_context(frame, Rect::new(area.x + 2, y, width, 3), theme);
        y += 4;
        (style.draw)(
            frame,
            Rect::new(area.x + 2, y, width, style.rows),
            theme,
            text,
            command,
        );
        y += style.rows + 1;
        if y >= area.bottom() {
            return;
        }
    }
}
