//! Six sidebar layouts, for choosing one.
//!
//!     cargo run -p enowx-tui --example sidebarlayouts
//!     cargo run -p enowx-tui --example sidebarlayouts -- 3
//!
//! Rendered in `chrome_void`, which is the theme actually in use, rather than
//! in the default one. That matters more than it sounds: `chrome_void` puts
//! `border` at RGB(36,36,54) over a `panel` of RGB(13,13,20) — about 1.3:1,
//! which is a line you can measure but not see. A layout that separates with
//! rules disappears in this theme while looking correct in a test.
//!
//! So each of these is judged twice: does it organise the information, and
//! does it survive a border nobody can see.

use enowx_tui::theme::{Theme, THEMES};
use ratatui::{
    prelude::*,
    widgets::{Block, BorderType, Borders, Paragraph},
};

/// The theme in use, not the default.
fn theme() -> &'static Theme {
    &THEMES[2]
}

struct Sample {
    name: &'static str,
    note: &'static str,
    /// What it costs in rows before the first real value.
    cost: &'static str,
    draw: fn(&mut Frame, Rect, &Theme),
}

const STYLES: [Sample; 6] = [
    Sample {
        name: "1 · Sekarang",
        note: "yang ada: rule ber-judul tiap seksi, label kiri nilai kanan.",
        cost: "3 baris",
        draw: draw_current,
    },
    Sample {
        name: "2 · Kartu",
        note: "tiap seksi jadi blok berlatar, tanpa garis — warna yang memisah.",
        cost: "2 baris",
        draw: draw_cards,
    },
    Sample {
        name: "3 · Satu ringkas",
        note: "angka penting saja, besar dan di depan; sisanya dibuang.",
        cost: "1 baris",
        draw: draw_digest,
    },
    Sample {
        name: "4 · Judul tebal",
        note: "judul seksi pakai warna aksen, bukan rule — terbaca walau border pudar.",
        cost: "2 baris",
        draw: draw_bold_headings,
    },
    Sample {
        name: "5 · Dua kolom",
        note: "label dan nilai berdampingan rapat, seksi dipisah satu baris kosong.",
        cost: "2 baris",
        draw: draw_two_column,
    },
    Sample {
        name: "6 · Meteran",
        note: "yang punya proporsi digambar sebagai bar; angka jadi pendamping.",
        cost: "2 baris",
        draw: draw_meters,
    },
];

const W: u16 = 46;

fn fill(frame: &mut Frame, area: Rect, colour: Color) {
    frame.render_widget(Block::default().style(Style::default().bg(colour)), area);
}

fn put(frame: &mut Frame, x: u16, y: u16, w: u16, line: Line) {
    frame.render_widget(Paragraph::new(line), Rect::new(x, y, w, 1));
}

/// The box every sample is drawn inside, so the frame is not what differs.
fn boxed(frame: &mut Frame, area: Rect, t: &Theme, title: &str) -> Rect {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.border))
        .style(Style::default().bg(t.panel));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    put(
        frame,
        area.x + 2,
        area.y,
        area.width.saturating_sub(4),
        Line::styled(
            format!(" {title} "),
            Style::default()
                .fg(t.accent)
                .bg(t.panel)
                .add_modifier(Modifier::BOLD),
        ),
    );
    inner
}

/// The tab row, shared by every sample that keeps one.
fn tabs(frame: &mut Frame, area: Rect, t: &Theme, active: usize) {
    let names = ["1", "2", "3", "4", "5"];
    let labels = ["TOKENS", "TOOLS", "SKILLS", "AGENT", "LOGS"];
    let mut x = area.x;
    for (i, (n, l)) in names.iter().zip(labels).enumerate() {
        let text = if i == active {
            format!(" {n} {l} ")
        } else {
            format!(" {n} ")
        };
        let w = text.chars().count() as u16;
        if x + w > area.x + area.width {
            break;
        }
        put(
            frame,
            x,
            area.y,
            w,
            Line::styled(
                text,
                if i == active {
                    Style::default()
                        .fg(t.accent)
                        .bg(t.active_tab)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(t.muted).bg(t.panel)
                },
            ),
        );
        x += w;
    }
}

/// `1 · Sekarang` — the before picture, drawn from the real code's shape.
fn draw_current(frame: &mut Frame, area: Rect, t: &Theme) {
    let inner = boxed(frame, area, t, "TOKENS");
    tabs(frame, Rect::new(inner.x, inner.y, inner.width, 1), t, 0);
    let x = inner.x + 1;
    let w = inner.width - 2;
    let mut y = inner.y + 2;
    for (title, rows) in [
        ("CONTEXT", vec![("Used", "0 / 1,000,000")]),
        ("LAST MODEL CALL", vec![("Input", "0"), ("Output", "0")]),
        (
            "SESSION",
            vec![("Tool calls", "0"), ("Messages", "0"), ("State", "ready")],
        ),
        (
            "COST",
            vec![
                ("Session", "$0.0000"),
                ("Per 1M in", "$0.1550"),
                ("Per 1M out", "$0.6200"),
            ],
        ),
    ] {
        // The rule carries the title — invisible in this theme.
        let rule = (w as usize).saturating_sub(title.len() + 3);
        put(
            frame,
            x,
            y,
            w,
            Line::from(vec![
                Span::styled("─ ", Style::default().fg(t.border)),
                Span::styled(
                    title,
                    Style::default().fg(t.faint).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" {}", "─".repeat(rule)),
                    Style::default().fg(t.border),
                ),
            ]),
        );
        y += 1;
        for (label, value) in rows {
            let pad = (w as usize).saturating_sub(label.len() + value.len());
            put(
                frame,
                x,
                y,
                w,
                Line::from(vec![
                    Span::styled(label, Style::default().fg(t.muted)),
                    Span::raw(" ".repeat(pad)),
                    Span::styled(value, Style::default().fg(t.text)),
                ]),
            );
            y += 1;
        }
        y += 1;
        if y >= inner.bottom() {
            return;
        }
    }
}

/// `2 · Kartu` — each section is a filled block. No rules at all, so nothing
/// depends on a border being visible.
fn draw_cards(frame: &mut Frame, area: Rect, t: &Theme) {
    let inner = boxed(frame, area, t, "TOKENS");
    tabs(frame, Rect::new(inner.x, inner.y, inner.width, 1), t, 0);
    let x = inner.x + 1;
    let w = inner.width - 2;
    let mut y = inner.y + 2;
    for (title, rows) in [
        ("CONTEXT", vec![("Used", "0 / 1,000,000")]),
        ("LAST CALL", vec![("Input", "0"), ("Output", "0")]),
        (
            "SESSION",
            vec![("Tool calls", "0"), ("Messages", "0"), ("State", "ready")],
        ),
        ("COST", vec![("Session", "$0.0000")]),
    ] {
        if y + rows.len() as u16 + 1 >= inner.bottom() {
            return;
        }
        // The heading sits on the raised surface, which reads as a card's lip
        // even when every line in the theme is invisible.
        fill(frame, Rect::new(x, y, w, 1), t.active_tab);
        put(
            frame,
            x,
            y,
            w,
            Line::styled(
                format!(" {title}"),
                Style::default()
                    .fg(t.accent)
                    .bg(t.active_tab)
                    .add_modifier(Modifier::BOLD),
            ),
        );
        y += 1;
        for (label, value) in rows {
            let pad = (w as usize).saturating_sub(label.len() + value.len() + 2);
            put(
                frame,
                x,
                y,
                w,
                Line::from(vec![
                    Span::raw(" "),
                    Span::styled(label, Style::default().fg(t.muted)),
                    Span::raw(" ".repeat(pad)),
                    Span::styled(value, Style::default().fg(t.text)),
                    Span::raw(" "),
                ]),
            );
            y += 1;
        }
        y += 1;
    }
}

/// `3 · Satu ringkas` — the two or three numbers anyone acts on, set large,
/// and nothing else.
fn draw_digest(frame: &mut Frame, area: Rect, t: &Theme) {
    let inner = boxed(frame, area, t, "TOKENS");
    tabs(frame, Rect::new(inner.x, inner.y, inner.width, 1), t, 0);
    let x = inner.x + 2;
    let w = inner.width - 3;
    let mut y = inner.y + 2;

    // Context is the one number that changes what you do next.
    put(
        frame,
        x,
        y,
        w,
        Line::from(vec![
            Span::styled(
                "0.0%",
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  context terpakai", Style::default().fg(t.muted)),
        ]),
    );
    y += 1;
    let track = (w as usize).saturating_sub(2);
    put(
        frame,
        x,
        y,
        w,
        Line::styled("░".repeat(track), Style::default().fg(t.faint)),
    );
    y += 1;
    put(
        frame,
        x,
        y,
        w,
        Line::styled("0 / 1,000,000 token", Style::default().fg(t.faint)),
    );
    y += 2;

    for (value, label) in [
        ("$0.0000", "biaya sesi"),
        ("0", "panggilan tool"),
        ("ready", "status"),
    ] {
        put(
            frame,
            x,
            y,
            w,
            Line::from(vec![
                Span::styled(
                    format!("{value:<10}"),
                    Style::default().fg(t.text).add_modifier(Modifier::BOLD),
                ),
                Span::styled(label, Style::default().fg(t.muted)),
            ]),
        );
        y += 1;
    }
    y += 1;
    put(
        frame,
        x,
        y,
        w,
        Line::styled("tekan 1–5 untuk tab lain", Style::default().fg(t.faint)),
    );
}

/// `4 · Judul tebal` — headings in the accent colour with no rule at all.
fn draw_bold_headings(frame: &mut Frame, area: Rect, t: &Theme) {
    let inner = boxed(frame, area, t, "TOKENS");
    tabs(frame, Rect::new(inner.x, inner.y, inner.width, 1), t, 0);
    let x = inner.x + 1;
    let w = inner.width - 2;
    let mut y = inner.y + 2;
    for (title, rows) in [
        ("CONTEXT", vec![("Used", "0 / 1,000,000")]),
        ("LAST MODEL CALL", vec![("Input", "0"), ("Output", "0")]),
        (
            "SESSION",
            vec![("Tool calls", "0"), ("Messages", "0"), ("State", "ready")],
        ),
        (
            "COST",
            vec![("Session", "$0.0000"), ("Per 1M in", "$0.1550")],
        ),
    ] {
        if y + rows.len() as u16 + 1 >= inner.bottom() {
            return;
        }
        put(
            frame,
            x,
            y,
            w,
            Line::styled(
                title,
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            ),
        );
        y += 1;
        for (label, value) in rows {
            let pad = (w as usize).saturating_sub(label.len() + value.len() + 2);
            put(
                frame,
                x,
                y,
                w,
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(label, Style::default().fg(t.muted)),
                    Span::raw(" ".repeat(pad)),
                    Span::styled(value, Style::default().fg(t.text)),
                ]),
            );
            y += 1;
        }
        y += 1;
    }
}

/// `5 · Dua kolom` — a tight label/value grid, sections separated by one blank
/// row and nothing else.
fn draw_two_column(frame: &mut Frame, area: Rect, t: &Theme) {
    let inner = boxed(frame, area, t, "TOKENS");
    tabs(frame, Rect::new(inner.x, inner.y, inner.width, 1), t, 0);
    let x = inner.x + 1;
    let w = inner.width - 2;
    let mut y = inner.y + 2;
    let label_w = 14usize;
    for group in [
        vec![
            ("context", "0 / 1,000,000"),
            ("terpakai", "0.0%"),
        ],
        vec![("input", "0"), ("output", "0")],
        vec![("tool calls", "0"), ("messages", "0"), ("state", "ready")],
        vec![("biaya sesi", "$0.0000"), ("per 1M in", "$0.1550")],
    ] {
        if y + group.len() as u16 + 1 >= inner.bottom() {
            return;
        }
        for (label, value) in group {
            put(
                frame,
                x,
                y,
                w,
                Line::from(vec![
                    Span::styled(format!("{label:<label_w$}"), Style::default().fg(t.muted)),
                    Span::styled(value, Style::default().fg(t.text)),
                ]),
            );
            y += 1;
        }
        y += 1;
    }
}

/// `6 · Meteran` — anything with a proportion is drawn as a bar first.
fn draw_meters(frame: &mut Frame, area: Rect, t: &Theme) {
    let inner = boxed(frame, area, t, "TOKENS");
    tabs(frame, Rect::new(inner.x, inner.y, inner.width, 1), t, 0);
    let x = inner.x + 1;
    let w = inner.width - 2;
    let mut y = inner.y + 2;

    for (label, value, pct) in [
        ("context", "0 / 1,000,000", 0.0f64),
        ("biaya sesi", "$0.0000", 0.0),
    ] {
        put(
            frame,
            x,
            y,
            w,
            Line::from(vec![
                Span::styled(label, Style::default().fg(t.muted)),
                Span::styled(
                    format!("  {value}"),
                    Style::default().fg(t.text).add_modifier(Modifier::BOLD),
                ),
            ]),
        );
        y += 1;
        let track = (w as usize).saturating_sub(1);
        let filled = ((track as f64) * pct / 100.0).round() as usize;
        put(
            frame,
            x,
            y,
            w,
            Line::from(vec![
                Span::styled("█".repeat(filled), Style::default().fg(t.accent)),
                Span::styled(
                    "░".repeat(track.saturating_sub(filled)),
                    Style::default().fg(t.faint),
                ),
            ]),
        );
        y += 2;
    }

    for (label, value) in [("input", "0"), ("output", "0"), ("tool calls", "0")] {
        let pad = (w as usize).saturating_sub(label.len() + value.len());
        put(
            frame,
            x,
            y,
            w,
            Line::from(vec![
                Span::styled(label, Style::default().fg(t.muted)),
                Span::raw(" ".repeat(pad)),
                Span::styled(value, Style::default().fg(t.text)),
            ]),
        );
        y += 1;
    }
}

pub fn render(width: u16, height: u16, only: Option<usize>) -> Vec<String> {
    use ratatui::backend::TestBackend;
    let t = theme();
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("backend");
    terminal.draw(|frame| paint(frame, t, only)).expect("draw");
    terminal
        .backend()
        .buffer()
        .content()
        .chunks(width as usize)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect())
        .collect()
}

fn paint(frame: &mut Frame, t: &Theme, only: Option<usize>) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(t.canvas).fg(t.text)),
        area,
    );
    match only {
        Some(n) if n >= 1 && n <= STYLES.len() => draw_one(frame, area, t, n - 1),
        _ => draw_all(frame, area, t),
    }
}

fn main() -> std::io::Result<()> {
    let mut index: Option<usize> = std::env::args().nth(1).and_then(|a| a.parse().ok());
    let t = theme();
    let mut terminal = ratatui::init();
    let result = (|| -> std::io::Result<()> {
        loop {
            terminal.draw(|frame| paint(frame, t, index))?;
            if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                use crossterm::event::KeyCode;
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char(c @ '1'..='6') => index = Some(c as usize - '0' as usize),
                    KeyCode::Right | KeyCode::Char('n') => {
                        index = Some(index.unwrap_or(0) % STYLES.len() + 1)
                    }
                    KeyCode::Left | KeyCode::Char('p') => {
                        index = Some(match index.unwrap_or(1) {
                            1 => STYLES.len(),
                            n => n - 1,
                        })
                    }
                    KeyCode::Backspace => index = None,
                    _ => {}
                }
            }
        }
    })();
    ratatui::restore();
    result
}

/// Two across, so they can be compared without paging.
fn draw_all(frame: &mut Frame, area: Rect, t: &Theme) {
    frame.render_widget(
        Paragraph::new(Line::styled(
            "  sidebar · tema chrome_void · 1–6 lihat satu · q keluar",
            Style::default().fg(t.faint),
        )),
        Rect::new(area.x, area.y, area.width, 1),
    );
    let h = 16u16;
    let mut y = area.y + 2;
    for pair in STYLES.chunks(2) {
        if y + h + 1 > area.bottom() {
            return;
        }
        for (i, style) in pair.iter().enumerate() {
            let x = area.x + 2 + i as u16 * (W + 4);
            if x + W > area.right() {
                break;
            }
            frame.render_widget(
                Paragraph::new(Line::styled(
                    style.name,
                    Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
                )),
                Rect::new(x, y, W, 1),
            );
            (style.draw)(frame, Rect::new(x, y + 1, W, h - 1), t);
        }
        y += h + 1;
    }
}

fn draw_one(frame: &mut Frame, area: Rect, t: &Theme, index: usize) {
    let style = &STYLES[index];
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("  {}  ", style.name),
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("{}  ", style.cost), Style::default().fg(t.muted)),
            Span::styled(style.note, Style::default().fg(t.faint)),
        ])),
        Rect::new(area.x, area.y, area.width, 1),
    );
    let h = area.height.saturating_sub(4).min(24);
    (style.draw)(frame, Rect::new(area.x + 2, area.y + 2, W, h), t);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_layout_draws() {
        for n in 1..=STYLES.len() {
            assert_eq!(render(120, 30, Some(n)).len(), 30, "layout {n}");
        }
    }

    /// Printed for reading, since choosing between them means seeing them.
    ///
    ///     cargo test -p enowx-tui --example sidebarlayouts -- --ignored --nocapture
    #[test]
    #[ignore = "prints the layouts"]
    fn show() {
        for n in 1..=STYLES.len() {
            println!("\n=== {} ===", STYLES[n - 1].name);
            for row in render(70, 24, Some(n)) {
                println!("{}", row.trim_end());
            }
        }
    }
}
