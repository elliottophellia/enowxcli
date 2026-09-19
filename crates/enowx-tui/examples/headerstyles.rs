//! Ten header styles, for choosing one.
//!
//!     cargo run -p enowx-tui --example headerstyles
//!     cargo run -p enowx-tui --example headerstyles -- 3
//!
//! A header is read at a glance and then ignored, so what matters is which
//! facts survive being ignored. Each sample is drawn at the real width in the
//! real theme, with conversation under it, because a header is judged by how
//! it sits against what follows.
//!
//! The status bar already carries the agent, the model, the state and the
//! keys — these do not repeat it.

use enowx_tui::theme::{Theme, THEMES};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

struct Sample {
    name: &'static str,
    note: &'static str,
    rows: u16,
    draw: fn(&mut Frame, Rect, &Theme, &Facts),
}

/// What a header could show. Every style picks from these.
struct Facts {
    workspace: &'static str,
    path: &'static str,
    title: &'static str,
    branch: &'static str,
    session: &'static str,
}

const FACTS: Facts = Facts {
    workspace: "porto",
    path: "/Volumes/SSD/Brainstorm/porto",
    title: "perbagus portofolio saya",
    branch: "main",
    session: "4add8042",
};

const STYLES: [Sample; 10] = [
    Sample {
        name: "1 · Badge",
        note: "what enx has now: a reversed badge, workspace, title.",
        rows: 2,
        draw: draw_badge,
    },
    Sample {
        name: "2 · Plain",
        note: "no badge — just the workspace in the accent colour.",
        rows: 2,
        draw: draw_plain,
    },
    Sample {
        name: "3 · Path",
        note: "the full path instead of the folder name, dimmed but findable.",
        rows: 2,
        draw: draw_path,
    },
    Sample {
        name: "4 · Tabs",
        note: "the sidebar's tabs move up here, so F1–F5 is visible.",
        rows: 2,
        draw: draw_tabs,
    },
    Sample {
        name: "5 · Title-first",
        note: "the session's own name leads; the workspace follows it, quiet.",
        rows: 2,
        draw: draw_title_first,
    },
    Sample {
        name: "6 · Rule-only",
        note: "no header row at all — the pane label rule carries the name.",
        rows: 1,
        draw: draw_rule_only,
    },
    Sample {
        name: "7 · Boxed",
        note: "a box, matching the composer below it.",
        rows: 3,
        draw: draw_boxed,
    },
    Sample {
        name: "8 · Git",
        note: "branch and session id on the right, the way a vim statusline does.",
        rows: 2,
        draw: draw_git,
    },
    Sample {
        name: "9 · Underline",
        note: "an accent rule under the name instead of a full-width border.",
        rows: 2,
        draw: draw_underline,
    },
    Sample {
        name: "10 · Centred",
        note: "the title centred, workspace left — a document, not a tool.",
        rows: 2,
        draw: draw_centred,
    },
];

fn rule(frame: &mut Frame, area: Rect, t: &Theme, y: u16) {
    frame.render_widget(
        Paragraph::new("─".repeat(area.width as usize))
            .style(Style::default().fg(t.border).bg(t.subtle)),
        Rect::new(area.x, y, area.width, 1),
    );
}

fn fill(frame: &mut Frame, area: Rect, colour: Color) {
    frame.render_widget(Block::default().style(Style::default().bg(colour)), area);
}

fn draw_badge(frame: &mut Frame, area: Rect, t: &Theme, f: &Facts) {
    fill(frame, area, t.subtle);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " ❯_ ENX ",
                Style::default()
                    .fg(t.panel)
                    .bg(t.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(f.workspace, Style::default().fg(t.text)),
            Span::styled("  ›  ", Style::default().fg(t.faint)),
            Span::styled(f.title, Style::default().fg(t.muted)),
        ]))
        .style(Style::default().bg(t.subtle)),
        Rect::new(area.x + 1, area.y, area.width.saturating_sub(2), 1),
    );
    rule(frame, area, t, area.y + 1);
}

fn draw_plain(frame: &mut Frame, area: Rect, t: &Theme, f: &Facts) {
    fill(frame, area, t.subtle);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                f.workspace,
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ·  ", Style::default().fg(t.faint)),
            Span::styled(f.title, Style::default().fg(t.muted)),
        ]))
        .style(Style::default().bg(t.subtle)),
        Rect::new(area.x + 2, area.y, area.width.saturating_sub(3), 1),
    );
    rule(frame, area, t, area.y + 1);
}

fn draw_path(frame: &mut Frame, area: Rect, t: &Theme, f: &Facts) {
    fill(frame, area, t.subtle);
    let (parent, name) = f.path.rsplit_once('/').unwrap_or(("", f.path));
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!("{parent}/"), Style::default().fg(t.faint)),
            Span::styled(
                name,
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            ),
        ]))
        .style(Style::default().bg(t.subtle)),
        Rect::new(area.x + 2, area.y, area.width.saturating_sub(3), 1),
    );
    rule(frame, area, t, area.y + 1);
}

fn draw_tabs(frame: &mut Frame, area: Rect, t: &Theme, f: &Facts) {
    fill(frame, area, t.subtle);
    let mut spans = vec![
        Span::styled(
            f.workspace,
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled("   ", Style::default().fg(t.faint)),
    ];
    for (index, name) in ["TOKENS", "TOOLS", "SKILLS", "AGENT", "LOGS"]
        .into_iter()
        .enumerate()
    {
        let active = index == 3;
        spans.push(Span::styled(
            format!(" {name} "),
            if active {
                Style::default()
                    .fg(t.accent)
                    .bg(t.active_tab)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(t.muted)
            },
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(t.subtle)),
        Rect::new(area.x + 2, area.y, area.width.saturating_sub(3), 1),
    );
    rule(frame, area, t, area.y + 1);
}

fn draw_title_first(frame: &mut Frame, area: Rect, t: &Theme, f: &Facts) {
    fill(frame, area, t.subtle);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                f.title,
                Style::default().fg(t.text).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("   in {}", f.workspace),
                Style::default().fg(t.faint),
            ),
        ]))
        .style(Style::default().bg(t.subtle)),
        Rect::new(area.x + 2, area.y, area.width.saturating_sub(3), 1),
    );
    rule(frame, area, t, area.y + 1);
}

fn draw_rule_only(frame: &mut Frame, area: Rect, t: &Theme, f: &Facts) {
    fill(frame, area, t.subtle);
    rule(frame, area, t, area.y);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {} ", f.workspace),
                Style::default().fg(t.accent).bg(t.subtle),
            ),
            Span::styled(
                format!("· {} ", f.title),
                Style::default().fg(t.faint).bg(t.subtle),
            ),
        ])),
        Rect::new(area.x + 2, area.y, area.width.saturating_sub(4), 1),
    );
}

fn draw_boxed(frame: &mut Frame, area: Rect, t: &Theme, f: &Facts) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(t.border))
        .style(Style::default().bg(t.subtle));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                f.workspace,
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("   {}", f.title), Style::default().fg(t.muted)),
        ])),
        inner,
    );
}

fn draw_git(frame: &mut Frame, area: Rect, t: &Theme, f: &Facts) {
    fill(frame, area, t.subtle);
    let left = Line::from(vec![
        Span::styled(
            format!(" {} ", f.workspace),
            Style::default()
                .fg(t.panel)
                .bg(t.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("  {}", f.title), Style::default().fg(t.muted)),
    ]);
    let right = Line::from(vec![
        Span::styled(format!("⎇ {}  ", f.branch), Style::default().fg(t.green)),
        Span::styled(f.session, Style::default().fg(t.faint)),
    ]);
    let row = Rect::new(area.x + 1, area.y, area.width.saturating_sub(2), 1);
    frame.render_widget(
        Paragraph::new(left).style(Style::default().bg(t.subtle)),
        row,
    );
    let w = right.width() as u16;
    if w < row.width {
        frame.render_widget(
            Paragraph::new(right).style(Style::default().bg(t.subtle)),
            Rect::new(row.x + row.width - w, row.y, w, 1),
        );
    }
    rule(frame, area, t, area.y + 1);
}

fn draw_underline(frame: &mut Frame, area: Rect, t: &Theme, f: &Facts) {
    fill(frame, area, t.subtle);
    let name = format!("  {}  ", f.workspace);
    let w = name.chars().count() as u16;
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                name.clone(),
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(f.title, Style::default().fg(t.muted)),
        ]))
        .style(Style::default().bg(t.subtle)),
        Rect::new(area.x, area.y, area.width, 1),
    );
    rule(frame, area, t, area.y + 1);
    frame.render_widget(
        Paragraph::new("━".repeat(w as usize)).style(Style::default().fg(t.accent).bg(t.subtle)),
        Rect::new(area.x, area.y + 1, w.min(area.width), 1),
    );
}

fn draw_centred(frame: &mut Frame, area: Rect, t: &Theme, f: &Facts) {
    fill(frame, area, t.subtle);
    frame.render_widget(
        Paragraph::new(Line::styled(f.workspace, Style::default().fg(t.faint)))
            .style(Style::default().bg(t.subtle)),
        Rect::new(area.x + 2, area.y, area.width.saturating_sub(3), 1),
    );
    frame.render_widget(
        Paragraph::new(Line::styled(
            f.title,
            Style::default().fg(t.text).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center)
        .style(Style::default().bg(t.subtle)),
        Rect::new(area.x, area.y, area.width, 1),
    );
    rule(frame, area, t, area.y + 1);
}

/// A little conversation under each sample, since a header is judged by how
/// it sits against what follows rather than on its own.
fn draw_below(frame: &mut Frame, area: Rect, t: &Theme) {
    frame.render_widget(
        Paragraph::new(vec![
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
        ]),
        area,
    );
}

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

fn draw_all(frame: &mut Frame, area: Rect, theme: &Theme) {
    let width = area.width.min(78);
    let mut y = area.y;
    frame.render_widget(
        Paragraph::new(Line::styled(
            "  header styles · number to see one full size · q to quit",
            Style::default().fg(theme.faint),
        )),
        Rect::new(area.x, y, area.width, 1),
    );
    y += 2;
    for style in STYLES.iter() {
        if y + style.rows + 2 > area.bottom() {
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
        (style.draw)(
            frame,
            Rect::new(area.x + 2, y, width, style.rows),
            theme,
            &FACTS,
        );
        y += style.rows + 1;
    }
}

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
    let y = area.y + 2;
    (style.draw)(
        frame,
        Rect::new(area.x + 2, y, width, style.rows),
        theme,
        &FACTS,
    );
    draw_below(
        frame,
        Rect::new(area.x + 3, y + style.rows + 1, width, 3),
        theme,
    );
    // The status bar, so the header can be judged against what it must not
    // repeat.
    let bar = area.bottom().saturating_sub(2);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " READY ",
                Style::default()
                    .fg(theme.panel)
                    .bg(theme.green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " router ",
                Style::default()
                    .fg(theme.accent)
                    .bg(theme.active_tab)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "· cbc/deepseek-v4.1-flash ",
                Style::default().fg(theme.muted).bg(theme.active_tab),
            ),
        ]))
        .style(Style::default().bg(theme.subtle)),
        Rect::new(area.x + 2, bar, width, 1),
    );
}
