//! Ten sidebar and navigation styles, for choosing one.
//!
//!     cargo run -p enowx-tui --example sidebarstyles
//!     cargo run -p enowx-tui --example sidebarstyles -- 3
//!
//! The question these answer is the join. The header is a rounded box inset one
//! column from the window frame; the sidebar is a left border that starts two
//! rows below it. Between them sits the blank gap row, so the sidebar reads as
//! a strip cut off from the chrome above rather than as part of it.
//!
//! Every sample is drawn whole — header box, chat pane, sidebar, footer — at
//! one width, because a join can only be judged with both sides on screen. The
//! chat side is identical in all ten; only the header's lower edge and the
//! sidebar change.

use enowx_tui::theme::{Theme, THEMES};
use ratatui::{
    prelude::*,
    widgets::{Block, BorderType, Borders, Paragraph},
};

struct Sample {
    name: &'static str,
    note: &'static str,
    /// What it costs in rows before the first line of a tab's contents, counted
    /// from the top of the window. Chrome comes out of the conversation.
    cost: &'static str,
    draw: fn(&mut Frame, Rect, &Theme),
}

const TABS: [(&str, &str); 5] = [
    ("1", "TOKENS"),
    ("2", "TOOLS"),
    ("3", "SKILLS"),
    ("4", "AGENT"),
    ("5", "LOGS"),
];

const ACTIVE: usize = 3;

const STYLES: [Sample; 10] = [
    Sample {
        name: "1 · Current",
        note: "what enx has now: header box, a gap row, then a severed strip.",
        cost: "6 rows",
        draw: draw_current,
    },
    Sample {
        name: "2 · Welded",
        note: "the header box's right edge drops into the sidebar's border — one frame.",
        cost: "6 rows",
        draw: draw_welded,
    },
    Sample {
        name: "3 · Split box",
        note: "the header is two boxes sharing a wall; the sidebar hangs off the right one.",
        cost: "6 rows",
        draw: draw_split_box,
    },
    Sample {
        name: "4 · Tabs in header",
        note: "F1–F5 move onto the header's own row, the sidebar keeps only contents.",
        cost: "5 rows",
        draw: draw_tabs_in_header,
    },
    Sample {
        name: "5 · Full box",
        note: "the sidebar is its own rounded box, inset to match the header.",
        cost: "7 rows",
        draw: draw_full_box,
    },
    Sample {
        name: "6 · No gap",
        note: "the strip starts on the header's lower edge — the gap row stays chat-side only.",
        cost: "5 rows",
        draw: draw_no_gap,
    },
    Sample {
        name: "7 · Vertical tabs",
        note: "tab names run down the left of the strip, so the contents get the full height.",
        cost: "5 rows",
        draw: draw_vertical_tabs,
    },
    Sample {
        name: "8 · Header spans",
        note: "one header box over both panes, with the divider dropping out of its floor.",
        cost: "6 rows",
        draw: draw_header_spans,
    },
    Sample {
        name: "9 · Filled strip",
        note: "no border at all: the sidebar is a darker column, tabs on its own surface.",
        cost: "6 rows",
        draw: draw_filled,
    },
    Sample {
        name: "10 · Rail",
        note: "a three-column icon rail; the name shows only for the tab you are on.",
        cost: "5 rows",
        draw: draw_rail,
    },
];

const SIDEBAR: u16 = 40;
const WORKSPACE: &str = "porto";
const TITLE: &str = "perbagus portofolio saya";

fn fill(frame: &mut Frame, area: Rect, colour: Color) {
    frame.render_widget(Block::default().style(Style::default().bg(colour)), area);
}

fn text(frame: &mut Frame, x: u16, y: u16, w: u16, line: Line) {
    frame.render_widget(Paragraph::new(line), Rect::new(x, y, w, 1));
}

/// The window frame every sample sits inside, returning its interior.
///
/// Drawn once here rather than in each style, so a difference on screen is a
/// difference in the sample and not in how it was set up.
fn window(frame: &mut Frame, area: Rect, t: &Theme) -> Rect {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.border))
        .style(Style::default().bg(t.panel));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

/// The header box as `chrome.rs` draws it: inset one column, three rows.
fn header_box(frame: &mut Frame, area: Rect, t: &Theme, width: u16) {
    let boxed = Rect::new(area.x + 1, area.y, width.saturating_sub(2), 3);
    fill(frame, Rect::new(area.x, area.y, width, 3), t.subtle);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.border))
        .style(Style::default().bg(t.subtle));
    let inner = block.inner(boxed);
    frame.render_widget(block, boxed);
    text(
        frame,
        inner.x,
        inner.y,
        inner.width,
        Line::from(vec![
            Span::raw(" "),
            Span::styled(
                WORKSPACE,
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("   {TITLE}"), Style::default().fg(t.muted)),
        ]),
    );
}

/// The pane name written onto a rule, the way `pane_label` does it.
fn pane_label(frame: &mut Frame, x: u16, y: u16, t: &Theme, label: &str) {
    let label = format!(" {label} ");
    let w = label.chars().count() as u16;
    text(
        frame,
        x,
        y,
        w,
        Line::styled(
            label,
            Style::default()
                .fg(t.faint)
                .bg(t.subtle)
                .add_modifier(Modifier::BOLD),
        ),
    );
}

/// Conversation on the chat side. The same in every sample — the sidebar is
/// what is being chosen, and a chat pane that moved would hide that.
fn chat(frame: &mut Frame, area: Rect, t: &Theme) {
    let lines = vec![
        Line::from(vec![
            Span::styled("▌ ", Style::default().fg(t.accent)),
            Span::styled(
                "perbagus portofolio saya",
                Style::default().fg(t.text).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::default(),
        Line::styled(
            "Portofolio statis satu halaman.",
            Style::default().fg(t.text),
        ),
        Line::default(),
        Line::from(vec![
            Span::styled("▸ ", Style::default().fg(t.faint)),
            Span::styled("◆ ", Style::default().fg(t.yellow)),
            Span::styled("delegate    ", Style::default().fg(t.muted)),
            Span::styled("fe", Style::default().fg(t.accent)),
            Span::styled("        21 line brief", Style::default().fg(t.faint)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(lines),
        Rect::new(
            area.x + 1,
            area.y,
            area.width.saturating_sub(1),
            area.height,
        ),
    );
}

/// The AGENT tab's contents, so each style is judged carrying something.
fn agent_rows(t: &Theme) -> Vec<Line<'static>> {
    vec![
        Line::from(vec![
            Span::styled("◆ ", Style::default().fg(t.yellow)),
            Span::styled(
                "fe",
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  running  1m12s", Style::default().fg(t.faint)),
        ]),
        Line::styled("   hero + nav, no deps", Style::default().fg(t.muted)),
        Line::default(),
        Line::from(vec![
            Span::styled("✓ ", Style::default().fg(t.green)),
            Span::styled(
                "review",
                Style::default().fg(t.text).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  done  38s", Style::default().fg(t.faint)),
        ]),
        Line::styled("   VERIFIED · 2 files", Style::default().fg(t.muted)),
        Line::default(),
        Line::from(vec![
            Span::styled("✗ ", Style::default().fg(t.red)),
            Span::styled(
                "test",
                Style::default().fg(t.text).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  failed  6s", Style::default().fg(t.faint)),
        ]),
        Line::styled("   NO REPORT", Style::default().fg(t.muted)),
    ]
}

/// A horizontal tab strip, one row of labels and an underline under the active
/// one. `bg` lets a style put it on the sidebar's surface or the chrome's.
fn tab_strip(frame: &mut Frame, area: Rect, t: &Theme, bg: Color, underline: bool) {
    fill(frame, Rect::new(area.x, area.y, area.width, 2), bg);
    let compact = area.width < 44;
    for (index, (key, name)) in TABS.iter().enumerate() {
        let start = area.x + area.width * index as u16 / 5;
        let end = area.x + area.width * (index as u16 + 1) / 5;
        let w = end.saturating_sub(start);
        let active = index == ACTIVE;
        let style = Style::default()
            .fg(if active { t.accent } else { t.muted })
            .bg(if active { t.active_tab } else { bg });
        let label = if compact {
            (*key).to_owned()
        } else {
            format!("{key} {name}")
        };
        frame.render_widget(
            Paragraph::new(Line::styled(
                label,
                if active {
                    style.add_modifier(Modifier::BOLD)
                } else {
                    style
                },
            ))
            .alignment(Alignment::Center)
            .style(style),
            Rect::new(start, area.y, w, 1),
        );
        if active && underline && w > 0 {
            frame.render_widget(
                Paragraph::new("─".repeat(w as usize))
                    .style(Style::default().fg(t.accent).bg(t.active_tab)),
                Rect::new(start, area.y + 1, w, 1),
            );
        }
    }
}

fn body(frame: &mut Frame, area: Rect, t: &Theme) {
    frame.render_widget(
        Paragraph::new(agent_rows(t)),
        Rect::new(area.x, area.y, area.width, area.height),
    );
}

/// The status bar, stopping at the divider as the real one does.
fn footer(frame: &mut Frame, area: Rect, t: &Theme, chat_width: u16) {
    fill(frame, area, t.panel);
    fill(frame, Rect::new(area.x, area.y, chat_width, 1), t.subtle);
    text(
        frame,
        area.x,
        area.y,
        chat_width,
        Line::from(vec![
            Span::styled(
                " READY ",
                Style::default()
                    .fg(t.panel)
                    .bg(t.green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " router ",
                Style::default()
                    .fg(t.accent)
                    .bg(t.active_tab)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "· deepseek-v4.1-flash ",
                Style::default().fg(t.muted).bg(t.active_tab),
            ),
        ]),
    );
}

/// `1 · Current` — the shape on screen today, drawn so the rest have something
/// to be compared against.
///
/// The header box ends at its own rounded corner, a blank row follows, and the
/// sidebar's left border starts below that with nothing above it. The strip
/// reads as severed because it is: three separate pieces of chrome, none of
/// them touching.
fn draw_current(frame: &mut Frame, area: Rect, t: &Theme) {
    let inner = window(frame, area, t);
    header_box(frame, inner, t, inner.width);
    let x = inner.x + inner.width - SIDEBAR;
    // The gap row, chat-side and sidebar-side alike.
    fill(
        frame,
        Rect::new(inner.x, inner.y + 3, inner.width, 1),
        t.panel,
    );
    let top = inner.y + 4;
    let h = inner.height - 5;
    frame.render_widget(
        Block::default()
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(t.border))
            .style(Style::default().bg(t.panel)),
        Rect::new(x, top, SIDEBAR, h),
    );
    pane_label(frame, inner.x + 3, inner.y + 2, t, "CHAT");
    pane_label(frame, x + 2, inner.y + 2, t, TABS[ACTIVE].1);
    tab_strip(
        frame,
        Rect::new(x + 1, top, SIDEBAR - 1, 2),
        t,
        t.subtle,
        true,
    );
    frame.render_widget(
        Paragraph::new("─".repeat(SIDEBAR as usize - 1))
            .style(Style::default().fg(t.border).bg(t.subtle)),
        Rect::new(x + 1, top + 1, SIDEBAR - 1, 1),
    );
    body(frame, Rect::new(x + 3, top + 3, SIDEBAR - 4, h - 4), t);
    chat(frame, Rect::new(inner.x, top, inner.width - SIDEBAR, h), t);
    footer(
        frame,
        Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
        t,
        inner.width - SIDEBAR,
    );
    text(
        frame,
        x,
        inner.bottom() - 1,
        1,
        Line::styled("│", Style::default().fg(t.border).bg(t.panel)),
    );
}

/// `2 · Welded` — the header box is cut at the divider column and its own
/// right wall becomes the sidebar's border, running unbroken to the footer.
///
/// One `┬` where the divider leaves the header's floor is the whole trick: the
/// eye follows a continuous line and reads the two as one frame. The gap row
/// stays on the chat side only, so the sidebar's contents start higher.
fn draw_welded(frame: &mut Frame, area: Rect, t: &Theme) {
    let inner = window(frame, area, t);
    header_box(frame, inner, t, inner.width);
    let x = inner.x + inner.width - SIDEBAR;
    fill(
        frame,
        Rect::new(inner.x, inner.y + 3, inner.width - SIDEBAR, 1),
        t.panel,
    );
    // The divider leaving the header's lower edge.
    text(
        frame,
        x,
        inner.y + 2,
        1,
        Line::styled("┬", Style::default().fg(t.border).bg(t.subtle)),
    );
    let top = inner.y + 3;
    let h = inner.bottom() - top - 1;
    fill(frame, Rect::new(x, top, SIDEBAR, h), t.panel);
    for y in top..top + h {
        text(
            frame,
            x,
            y,
            1,
            Line::styled("│", Style::default().fg(t.border).bg(t.panel)),
        );
    }
    pane_label(frame, inner.x + 3, inner.y + 2, t, "CHAT");
    tab_strip(
        frame,
        Rect::new(x + 1, top, SIDEBAR - 1, 2),
        t,
        t.subtle,
        true,
    );
    frame.render_widget(
        Paragraph::new("─".repeat(SIDEBAR as usize - 1))
            .style(Style::default().fg(t.border).bg(t.subtle)),
        Rect::new(x + 1, top + 1, SIDEBAR - 1, 1),
    );
    body(frame, Rect::new(x + 3, top + 3, SIDEBAR - 4, h - 3), t);
    chat(
        frame,
        Rect::new(inner.x, inner.y + 4, inner.width - SIDEBAR, h - 1),
        t,
    );
    footer(
        frame,
        Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
        t,
        inner.width - SIDEBAR,
    );
    text(
        frame,
        x,
        inner.bottom() - 1,
        1,
        Line::styled("│", Style::default().fg(t.border).bg(t.panel)),
    );
}

/// `3 · Split box` — the header is two boxes sharing one wall, the narrow one
/// sitting over the sidebar and naming the tab.
///
/// The join is explicit rather than implied: the shared wall is the divider,
/// and the sidebar hangs off the right box the way the transcript hangs off the
/// left. The cost is a `┤├` pair at the seam, which is busier than one line.
fn draw_split_box(frame: &mut Frame, area: Rect, t: &Theme) {
    let inner = window(frame, area, t);
    let x = inner.x + inner.width - SIDEBAR;
    fill(frame, Rect::new(inner.x, inner.y, inner.width, 3), t.subtle);
    // The two boxes overlap by one column: the left one's right wall and the
    // right one's left wall are the same cell, which is what "sharing a wall"
    // has to mean for the seam to be one line rather than two.
    let left = Rect::new(inner.x + 1, inner.y, x - inner.x, 3);
    let right = Rect::new(x, inner.y, inner.right() - 1 - x, 3);
    for (b, title) in [(left, WORKSPACE), (right, TABS[ACTIVE].1)] {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.border))
            .style(Style::default().bg(t.subtle));
        let bi = block.inner(b);
        frame.render_widget(block, b);
        let line = if title == WORKSPACE {
            Line::from(vec![
                Span::styled(
                    format!(" {WORKSPACE}"),
                    Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("   {TITLE}"), Style::default().fg(t.muted)),
            ])
        } else {
            Line::styled(
                format!(" {title}"),
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            )
        };
        text(frame, bi.x, bi.y, bi.width, line);
    }
    // The shared wall drawn as junctions rather than as two rounded corners
    // meeting. `╭` against `╮` reads as two boxes that happen to touch; `┬`
    // and `┴` read as one frame with a wall in it.
    for (y, glyph) in [(inner.y, "┬"), (inner.y + 2, "┴")] {
        text(
            frame,
            x,
            y,
            1,
            Line::styled(glyph, Style::default().fg(t.border).bg(t.subtle)),
        );
    }
    let top = inner.y + 3;
    let h = inner.bottom() - top - 1;
    fill(frame, Rect::new(inner.x, top, inner.width, h), t.panel);
    for y in top..top + h {
        text(
            frame,
            x,
            y,
            1,
            Line::styled("│", Style::default().fg(t.border).bg(t.panel)),
        );
    }
    // No underline: the box above already names the tab in the accent colour,
    // and a second marker for the same fact left a rule floating over the
    // first row of contents.
    tab_strip(
        frame,
        Rect::new(x + 1, top, SIDEBAR - 1, 2),
        t,
        t.panel,
        false,
    );
    body(frame, Rect::new(x + 3, top + 2, SIDEBAR - 4, h - 2), t);
    chat(
        frame,
        Rect::new(inner.x, top + 1, inner.width - SIDEBAR, h - 1),
        t,
    );
    footer(
        frame,
        Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
        t,
        inner.width - SIDEBAR,
    );
    text(
        frame,
        x,
        inner.bottom() - 1,
        1,
        Line::styled("│", Style::default().fg(t.border).bg(t.panel)),
    );
}

/// `4 · Tabs in header` — F1–F5 sit on the header's own row, and the sidebar
/// below carries nothing but the selected tab's contents.
///
/// This is the cheapest of the ten: the strip's two rows come back to the
/// contents, and the tabs are visible from anywhere rather than only when the
/// sidebar is open. It costs the header its right-hand half, so a long session
/// title is the first thing to be cut.
fn draw_tabs_in_header(frame: &mut Frame, area: Rect, t: &Theme) {
    let inner = window(frame, area, t);
    fill(frame, Rect::new(inner.x, inner.y, inner.width, 3), t.subtle);
    let boxed = Rect::new(inner.x + 1, inner.y, inner.width - 2, 3);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.border))
        .style(Style::default().bg(t.subtle));
    let bi = block.inner(boxed);
    frame.render_widget(block, boxed);
    text(
        frame,
        bi.x,
        bi.y,
        bi.width,
        Line::from(vec![
            Span::raw(" "),
            Span::styled(
                WORKSPACE,
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  {TITLE}"), Style::default().fg(t.muted)),
        ]),
    );
    // The tabs right-aligned inside the header box, over the sidebar's
    // columns. Every cell is the same width — naming only the selected one
    // made the row reflow as the selection moved, so the number under your
    // finger was never twice in the same place.
    let mut spans: Vec<Span<'static>> = Vec::new();
    // The number alone. An initial beside it looked like it named the tab and
    // did not: TOKENS and TOOLS are both `T`, so the one distinction the
    // letter was there to draw is the one it cannot draw.
    for (index, (key, _)) in TABS.iter().enumerate() {
        let active = index == ACTIVE;
        spans.push(Span::styled(
            format!(" {key} "),
            if active {
                Style::default()
                    .fg(t.accent)
                    .bg(t.active_tab)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(t.muted).bg(t.subtle)
            },
        ));
        spans.push(Span::styled(" ", Style::default().bg(t.subtle)));
    }
    // The selected tab named in full, ahead of the strip: the letters alone
    // say which of five, but not which one AGENT is.
    spans.insert(
        0,
        Span::styled(
            format!("{}  ", TABS[ACTIVE].1),
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        ),
    );
    let line = Line::from(spans);
    let w = line.width() as u16;
    text(frame, bi.x + bi.width - w, bi.y, w, line);
    let x = inner.x + inner.width - SIDEBAR;
    text(
        frame,
        x,
        inner.y + 2,
        1,
        Line::styled("┬", Style::default().fg(t.border).bg(t.subtle)),
    );
    let top = inner.y + 3;
    let h = inner.bottom() - top - 1;
    fill(frame, Rect::new(inner.x, top, inner.width, h), t.panel);
    for y in top..top + h {
        text(
            frame,
            x,
            y,
            1,
            Line::styled("│", Style::default().fg(t.border).bg(t.panel)),
        );
    }
    body(frame, Rect::new(x + 3, top + 1, SIDEBAR - 4, h - 1), t);
    chat(
        frame,
        Rect::new(inner.x, top + 1, inner.width - SIDEBAR, h - 1),
        t,
    );
    footer(
        frame,
        Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
        t,
        inner.width - SIDEBAR,
    );
    text(
        frame,
        x,
        inner.bottom() - 1,
        1,
        Line::styled("│", Style::default().fg(t.border).bg(t.panel)),
    );
}

/// `5 · Full box` — the sidebar is a rounded box of its own, inset to line up
/// with the header and the composer.
///
/// Nothing is welded; instead everything is the same kind of object, so the
/// strip stops looking like a leftover. It is the most expensive: two more
/// rows and two more columns of border, all of it out of the contents.
fn draw_full_box(frame: &mut Frame, area: Rect, t: &Theme) {
    let inner = window(frame, area, t);
    header_box(frame, inner, t, inner.width);
    let x = inner.x + inner.width - SIDEBAR;
    let top = inner.y + 4;
    // Down to the frame rather than stopping a row above it. Closed on four
    // sides, the box was floating clear of the status bar with a strip of
    // panel under it — which is the same "rule stopping in mid-air" that the
    // divider's carry-through into the footer was written to fix.
    let h = inner.bottom() - top;
    fill(
        frame,
        Rect::new(inner.x, inner.y + 3, inner.width, 1),
        t.panel,
    );
    fill(frame, Rect::new(x, top, SIDEBAR, h), t.panel);
    let boxed = Rect::new(x, top, SIDEBAR - 1, h);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.border))
        .style(Style::default().bg(t.panel));
    let bi = block.inner(boxed);
    frame.render_widget(block, boxed);
    // No CHAT label. With the sidebar a box of its own, the header's lower
    // edge runs the whole width and belongs to neither pane, so a name on it
    // pointed at a rule that is not the chat pane's. The tab box titles
    // itself; the transcript underneath needs no label to be recognised.
    // The tab name on the box's own top edge, the way a bordered widget titles
    // itself — so the strip does not spend a row saying what it is.
    text(
        frame,
        boxed.x + 2,
        boxed.y,
        SIDEBAR - 4,
        Line::styled(
            format!(" {} ", TABS[ACTIVE].1),
            Style::default()
                .fg(t.accent)
                .bg(t.panel)
                .add_modifier(Modifier::BOLD),
        ),
    );
    // The box's top edge already names the tab in the accent colour, so the
    // strip needs no underline — it would have landed on the first row of
    // contents, marking a row that is not a tab.
    tab_strip(frame, Rect::new(bi.x, bi.y, bi.width, 2), t, t.panel, false);
    body(
        frame,
        Rect::new(bi.x + 1, bi.y + 2, bi.width - 1, bi.height - 2),
        t,
    );
    chat(frame, Rect::new(inner.x, top, inner.width - SIDEBAR, h), t);
    footer(
        frame,
        Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
        t,
        inner.width - SIDEBAR,
    );
}

/// `6 · No gap` — the sidebar starts on the header's lower edge; the blank row
/// belongs to the chat pane alone.
///
/// The smallest possible change from what is there now, and it removes the
/// severed look on its own: the strip's first row touches the chrome above it.
/// The chat side keeps its breathing room, so the first message still does not
/// sit against a rule.
fn draw_no_gap(frame: &mut Frame, area: Rect, t: &Theme) {
    let inner = window(frame, area, t);
    header_box(frame, inner, t, inner.width);
    let x = inner.x + inner.width - SIDEBAR;
    fill(
        frame,
        Rect::new(inner.x, inner.y + 3, inner.width - SIDEBAR, 1),
        t.panel,
    );
    let top = inner.y + 3;
    let h = inner.bottom() - top - 1;
    fill(frame, Rect::new(x, top, SIDEBAR, h), t.panel);
    for y in top..top + h {
        text(
            frame,
            x,
            y,
            1,
            Line::styled("│", Style::default().fg(t.border).bg(t.panel)),
        );
    }
    pane_label(frame, inner.x + 3, inner.y + 2, t, "CHAT");
    pane_label(frame, x + 2, inner.y + 2, t, TABS[ACTIVE].1);
    // The strip's own row, then a rule under it. The active tab is marked by
    // its filled cell rather than by an accent underline, which on this layout
    // would have landed on the first row of contents.
    tab_strip(
        frame,
        Rect::new(x + 1, top, SIDEBAR - 1, 2),
        t,
        t.subtle,
        false,
    );
    frame.render_widget(
        Paragraph::new("─".repeat(SIDEBAR as usize - 1))
            .style(Style::default().fg(t.border).bg(t.subtle)),
        Rect::new(x + 1, top + 1, SIDEBAR - 1, 1),
    );
    body(frame, Rect::new(x + 3, top + 2, SIDEBAR - 4, h - 2), t);
    chat(
        frame,
        Rect::new(inner.x, inner.y + 4, inner.width - SIDEBAR, h - 1),
        t,
    );
    footer(
        frame,
        Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
        t,
        inner.width - SIDEBAR,
    );
    text(
        frame,
        x,
        inner.bottom() - 1,
        1,
        Line::styled("│", Style::default().fg(t.border).bg(t.panel)),
    );
}

/// `7 · Vertical tabs` — the five names run down the strip's left edge and the
/// contents take the whole height beside them.
///
/// A tab strip costs two rows whatever is in it; a rail costs columns, and the
/// sidebar has columns to spare more often than rows. It reads as a continuation
/// of the header because there is no horizontal rule under it to cut it off.
fn draw_vertical_tabs(frame: &mut Frame, area: Rect, t: &Theme) {
    let inner = window(frame, area, t);
    header_box(frame, inner, t, inner.width);
    let x = inner.x + inner.width - SIDEBAR;
    fill(
        frame,
        Rect::new(inner.x, inner.y + 3, inner.width - SIDEBAR, 1),
        t.panel,
    );
    text(
        frame,
        x,
        inner.y + 2,
        1,
        Line::styled("┬", Style::default().fg(t.border).bg(t.subtle)),
    );
    let top = inner.y + 3;
    let h = inner.bottom() - top - 1;
    fill(frame, Rect::new(x, top, SIDEBAR, h), t.panel);
    for y in top..top + h {
        text(
            frame,
            x,
            y,
            1,
            Line::styled("│", Style::default().fg(t.border).bg(t.panel)),
        );
    }
    // The rail is a full-height column in the chrome colour, so it reads as
    // the header's surface continuing downward. It has to be painted for its
    // whole height, not only behind the five labels, or the strip stops
    // halfway and the contents beside it look like they have leaked into it.
    let rail = 10;
    fill(frame, Rect::new(x + 1, top, rail, h), t.subtle);
    for y in top..top + h {
        text(
            frame,
            x + rail + 1,
            y,
            1,
            Line::styled("│", Style::default().fg(t.border).bg(t.subtle)),
        );
    }
    for (index, (key, name)) in TABS.iter().enumerate() {
        let active = index == ACTIVE;
        let y = top + index as u16;
        let style = if active {
            Style::default()
                .fg(t.accent)
                .bg(t.active_tab)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(t.muted).bg(t.subtle)
        };
        text(
            frame,
            x + 1,
            y,
            rail,
            Line::styled(format!("{key} {name:<6}"), style),
        );
        if active {
            text(
                frame,
                x + 1,
                y,
                1,
                Line::styled("▎", Style::default().fg(t.accent).bg(t.active_tab)),
            );
        }
    }
    pane_label(frame, inner.x + 3, inner.y + 2, t, "CHAT");
    body(
        frame,
        Rect::new(x + rail + 3, top, SIDEBAR - rail - 4, h),
        t,
    );
    chat(
        frame,
        Rect::new(inner.x, inner.y + 4, inner.width - SIDEBAR, h - 1),
        t,
    );
    footer(
        frame,
        Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
        t,
        inner.width - SIDEBAR,
    );
    text(
        frame,
        x,
        inner.bottom() - 1,
        1,
        Line::styled("│", Style::default().fg(t.border).bg(t.panel)),
    );
}

/// `8 · Header spans` — one header box across both panes, with the divider
/// starting at its floor and the pane names written onto that same edge.
///
/// Like `2 · Welded`, but the tab name sits on the header's lower edge rather
/// than inside the strip, so the sidebar's first row is already contents. The
/// header stops being a chat-pane header and becomes the window's.
fn draw_header_spans(frame: &mut Frame, area: Rect, t: &Theme) {
    let inner = window(frame, area, t);
    header_box(frame, inner, t, inner.width);
    let x = inner.x + inner.width - SIDEBAR;
    fill(
        frame,
        Rect::new(inner.x, inner.y + 3, inner.width - SIDEBAR, 1),
        t.panel,
    );
    text(
        frame,
        x,
        inner.y + 2,
        1,
        Line::styled("┬", Style::default().fg(t.border).bg(t.subtle)),
    );
    pane_label(frame, inner.x + 3, inner.y + 2, t, "CHAT");
    pane_label(frame, x + 2, inner.y + 2, t, TABS[ACTIVE].1);
    let top = inner.y + 3;
    let h = inner.bottom() - top - 1;
    fill(frame, Rect::new(x, top, SIDEBAR, h), t.panel);
    for y in top..top + h {
        text(
            frame,
            x,
            y,
            1,
            Line::styled("│", Style::default().fg(t.border).bg(t.panel)),
        );
    }
    // Numbers only, one row, under the contents rather than over them: the
    // name is already on the edge above.
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (index, (key, _)) in TABS.iter().enumerate() {
        spans.push(Span::styled(
            format!(" {key} "),
            if index == ACTIVE {
                Style::default()
                    .fg(t.accent)
                    .bg(t.active_tab)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(t.faint).bg(t.subtle)
            },
        ));
    }
    body(frame, Rect::new(x + 3, top, SIDEBAR - 4, h - 2), t);
    // The numbers at the foot of the strip, on the chrome colour, so the row
    // belongs to the footer band rather than trailing the contents.
    fill(
        frame,
        Rect::new(x + 1, inner.bottom() - 2, SIDEBAR - 1, 1),
        t.subtle,
    );
    text(
        frame,
        x + 3,
        inner.bottom() - 2,
        SIDEBAR - 4,
        Line::from(spans),
    );
    chat(
        frame,
        Rect::new(inner.x, inner.y + 4, inner.width - SIDEBAR, h - 1),
        t,
    );
    footer(
        frame,
        Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
        t,
        inner.width - SIDEBAR,
    );
    text(
        frame,
        x,
        inner.bottom() - 1,
        1,
        Line::styled("│", Style::default().fg(t.border).bg(t.panel)),
    );
}

/// `9 · Filled strip` — no border anywhere: the sidebar is a column in the
/// chrome colour, running from the header's floor to the footer.
///
/// Colour does the separating, so there is no line to be cut off and no join to
/// get wrong. It works because the header and footer are already `subtle`: the
/// strip becomes the third side of the same frame. Weakest at low contrast,
/// where the column edge disappears.
fn draw_filled(frame: &mut Frame, area: Rect, t: &Theme) {
    let inner = window(frame, area, t);
    header_box(frame, inner, t, inner.width);
    let x = inner.x + inner.width - SIDEBAR;
    fill(
        frame,
        Rect::new(inner.x, inner.y + 3, inner.width - SIDEBAR, 1),
        t.panel,
    );
    let top = inner.y + 3;
    let h = inner.bottom() - top;
    fill(frame, Rect::new(x, top, SIDEBAR, h), t.subtle);
    pane_label(frame, inner.x + 3, inner.y + 2, t, "CHAT");
    text(
        frame,
        x + 2,
        top,
        SIDEBAR - 3,
        Line::styled(
            TABS[ACTIVE].1,
            Style::default()
                .fg(t.accent)
                .bg(t.subtle)
                .add_modifier(Modifier::BOLD),
        ),
    );
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (index, (key, _)) in TABS.iter().enumerate() {
        spans.push(Span::styled(
            format!(" {key} "),
            if index == ACTIVE {
                Style::default()
                    .fg(t.accent)
                    .bg(t.active_tab)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(t.faint).bg(t.subtle)
            },
        ));
    }
    let w: u16 = spans.iter().map(|s| s.width() as u16).sum();
    text(frame, x + SIDEBAR - w - 2, top, w, Line::from(spans));
    // The contents sit on the strip's own surface, so each row is painted
    // rather than left on the panel behind it.
    for (index, line) in agent_rows(t).into_iter().enumerate() {
        let y = top + 2 + index as u16;
        if y >= inner.bottom() - 1 {
            break;
        }
        fill(frame, Rect::new(x + 2, y, SIDEBAR - 3, 1), t.subtle);
        frame.render_widget(
            Paragraph::new(line).style(Style::default().bg(t.subtle)),
            Rect::new(x + 2, y, SIDEBAR - 3, 1),
        );
    }
    chat(
        frame,
        Rect::new(inner.x, inner.y + 4, inner.width - SIDEBAR, h - 2),
        t,
    );
    footer(
        frame,
        Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
        t,
        inner.width - SIDEBAR,
    );
}

/// `10 · Rail` — a two-column rail of markers against the divider, with the
/// selected tab's name written once at the top of the contents.
///
/// The narrowest navigation that still says where you are. Five glyphs stacked
/// against the border cost no rows at all, and the rail is a continuation of
/// the divider rather than a thing sitting beside it. The risk is a marker that
/// nobody can decode without pressing it.
fn draw_rail(frame: &mut Frame, area: Rect, t: &Theme) {
    let inner = window(frame, area, t);
    header_box(frame, inner, t, inner.width);
    let x = inner.x + inner.width - SIDEBAR;
    fill(
        frame,
        Rect::new(inner.x, inner.y + 3, inner.width - SIDEBAR, 1),
        t.panel,
    );
    text(
        frame,
        x,
        inner.y + 2,
        1,
        Line::styled("┬", Style::default().fg(t.border).bg(t.subtle)),
    );
    let top = inner.y + 3;
    let h = inner.bottom() - top - 1;
    fill(frame, Rect::new(x, top, SIDEBAR, h), t.panel);
    for y in top..top + h {
        text(
            frame,
            x,
            y,
            1,
            Line::styled("│", Style::default().fg(t.border).bg(t.panel)),
        );
    }
    // Three columns of chrome against the divider, full height, so the rail is
    // the divider thickening rather than five glyphs floating in the contents.
    let rail = 3;
    fill(frame, Rect::new(x + 1, top, rail, h), t.subtle);
    let marks = ["◷", "⚒", "✦", "◆", "≡"];
    for (index, mark) in marks.iter().enumerate() {
        let y = top + 1 + index as u16 * 2;
        let active = index == ACTIVE;
        let style = if active {
            Style::default()
                .fg(t.accent)
                .bg(t.active_tab)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(t.faint).bg(t.subtle)
        };
        text(
            frame,
            x + 1,
            y,
            rail,
            Line::styled(format!(" {mark} "), style),
        );
    }
    pane_label(frame, inner.x + 3, inner.y + 2, t, "CHAT");
    text(
        frame,
        x + rail + 2,
        top,
        SIDEBAR - rail - 3,
        Line::from(vec![
            Span::styled(
                TABS[ACTIVE].1,
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled("   F4", Style::default().fg(t.faint)),
        ]),
    );
    body(
        frame,
        Rect::new(x + rail + 2, top + 2, SIDEBAR - rail - 3, h - 2),
        t,
    );
    chat(
        frame,
        Rect::new(inner.x, inner.y + 4, inner.width - SIDEBAR, h - 1),
        t,
    );
    footer(
        frame,
        Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
        t,
        inner.width - SIDEBAR,
    );
    text(
        frame,
        x,
        inner.bottom() - 1,
        1,
        Line::styled("│", Style::default().fg(t.border).bg(t.panel)),
    );
}

/// Rendered through a `TestBackend`, so a layout can be asserted on without a
/// terminal — the same door the other examples leave open.
pub fn render(width: u16, height: u16, only: Option<usize>) -> Vec<String> {
    use ratatui::backend::TestBackend;
    let theme = &THEMES[0];
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("backend");
    terminal
        .draw(|frame| paint(frame, theme, only))
        .expect("draw");
    terminal
        .backend()
        .buffer()
        .content()
        .chunks(width as usize)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect())
        .collect()
}

fn paint(frame: &mut Frame, theme: &Theme, only: Option<usize>) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.canvas).fg(theme.text)),
        area,
    );
    match only {
        Some(n) if n >= 1 && n <= STYLES.len() => draw_one(frame, area, theme, n - 1),
        _ => draw_index(frame, area, theme),
    }
}

fn main() -> std::io::Result<()> {
    let mut index: Option<usize> = std::env::args().nth(1).and_then(|a| a.parse().ok());
    let theme = &THEMES[0];
    let mut terminal = ratatui::init();
    let result = (|| -> std::io::Result<()> {
        loop {
            terminal.draw(|frame| paint(frame, theme, index))?;
            if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                use crossterm::event::KeyCode;
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    // Paging through them beats restarting the example ten
                    // times, which is what choosing one actually involves.
                    KeyCode::Char(c @ '1'..='9') => {
                        index = Some(c as usize - '0' as usize);
                    }
                    KeyCode::Char('0') => index = Some(10),
                    KeyCode::Right | KeyCode::Char('n') => {
                        index = Some(index.unwrap_or(0) % STYLES.len() + 1);
                    }
                    KeyCode::Left | KeyCode::Char('p') => {
                        index = Some(match index.unwrap_or(1) {
                            1 => STYLES.len(),
                            n => n - 1,
                        });
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

/// The index: every style named, with what it costs, since a full-size sample
/// is the only way to judge a join and ten of them will not fit at once.
fn draw_index(frame: &mut Frame, area: Rect, theme: &Theme) {
    let mut y = area.y;
    for line in [
        Line::styled(
            "  sidebar styles · 1–9, 0 for ten · ←/→ to page · q to quit",
            Style::default().fg(theme.faint),
        ),
        Line::default(),
    ] {
        frame.render_widget(Paragraph::new(line), Rect::new(area.x, y, area.width, 1));
        y += 1;
    }
    for style in STYLES.iter() {
        if y >= area.bottom() {
            return;
        }
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!("  {:<22}", style.name),
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<9}", style.cost),
                    Style::default().fg(theme.muted),
                ),
                Span::styled(style.note, Style::default().fg(theme.faint)),
            ])),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;
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
            Span::styled(
                format!("{}  ", style.cost),
                Style::default().fg(theme.muted),
            ),
            Span::styled(style.note, Style::default().fg(theme.faint)),
        ])),
        Rect::new(area.x, area.y, area.width, 1),
    );
    // The real thing is 100 columns or wider before a sidebar appears at all,
    // so a sample narrower than that would be judging a layout that never runs.
    let width = area.width.saturating_sub(4).clamp(80, 110);
    let height = area.height.saturating_sub(4).clamp(14, 22);
    if area.width < 84 || area.height < 18 {
        frame.render_widget(
            Paragraph::new(Line::styled(
                "  … make the window at least 84×18",
                Style::default().fg(theme.faint),
            )),
            Rect::new(area.x, area.y + 2, area.width, 1),
        );
        return;
    }
    (style.draw)(
        frame,
        Rect::new(area.x + 2, area.y + 2, width, height),
        theme,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every style has to draw inside its rect. A panic here is an arithmetic
    /// slip in one of the layouts, which is the failure these are prone to.
    #[test]
    fn every_style_draws() {
        for n in 1..=STYLES.len() {
            let rows = render(110, 26, Some(n));
            assert_eq!(rows.len(), 26, "style {n}");
        }
    }

    /// Kept rather than deleted: choosing between these means reading them,
    /// and a terminal is not always to hand. Every layout bug found while
    /// writing this file — a rule over the contents, a rail the text ran
    /// through, two boxes meeting at `╭╮` instead of `┬` — was visible here
    /// and invisible to `every_style_draws`.
    ///
    ///     cargo test -p enowx-tui --example sidebarstyles -- --ignored --nocapture
    #[test]
    #[ignore = "prints the samples for reading"]
    fn show() {
        for n in 1..=STYLES.len() {
            println!("\n=== {} ===", STYLES[n - 1].name);
            for row in render(110, 26, Some(n)) {
                println!("{}", row.trim_end());
            }
        }
    }

    /// The join is the whole point, so it is asserted rather than left to the
    /// eye: the sidebar's first row must carry chrome at the divider column
    /// rather than starting in mid-air below a gap.
    ///
    /// Three are exempt, and each for a stated reason rather than because the
    /// assertion was inconvenient. `1 · Current` is the before picture — it
    /// fails by definition, which is why the other nine exist. `5 · Full box`
    /// joins nothing on purpose: it answers the severed look by making the
    /// strip a peer of the header rather than a continuation of it.
    /// `9 · Filled strip` separates with colour and draws no line at all.
    #[test]
    fn the_strip_meets_the_header() {
        for (n, style) in STYLES.iter().enumerate() {
            let n = n + 1;
            if matches!(n, 1 | 5 | 9) {
                continue;
            }
            let rows = render(110, 26, Some(n));
            // The window frame starts at column 2, the header box at 3, and
            // its lower edge is the row the divider must leave from.
            let edge = rows
                .iter()
                .position(|r| r.contains('╰'))
                .unwrap_or_else(|| panic!("{}: no header box", style.name));
            let below = &rows[edge + 1];
            let cut = below.chars().filter(|c| *c == '│').count();
            assert!(
                cut >= 3,
                "{}: the row under the header has {cut} vertical rules; \
                 the strip is not joined to the chrome above it",
                style.name
            );
        }
    }
}
