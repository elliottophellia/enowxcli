use super::*;

/// Side column width at each breakpoint. Stepped rather than proportional, so
/// the label and value columns inside its cards sit on the same characters
/// from one terminal width to the next instead of drifting with every resize.
const SIDE_REGULAR: u16 = 40;
const SIDE_WIDE: u16 = 52;
/// Narrower than this and the side column cannot sit beside a readable chat;
/// its figures move into the status bar instead.
const SIDE_MIN_WINDOW: u16 = 100;
const WIDE_WINDOW: u16 = 160;
/// Shorter than this and the side column is dropped: two stacked cards with a
/// row or two each carry nothing the status bar does not.
const SIDE_MIN_HEIGHT: u16 = 12;
/// Shorter than this and the SESSION card goes, so the detail card keeps
/// enough rows to be worth having; the card's figures join the status bar.
const SESSION_MIN_HEIGHT: u16 = 18;

/// Where every box sits, worked out once per frame from the window size.
///
/// One place decides the edges so they cannot disagree. The old layout had
/// the header, sidebar, composer and footer each choose their own insets, and
/// no two of them ended on the same column.
pub(super) struct Grid {
    /// Chat box, palette and composer, stacked.
    pub main: Rect,
    pub side: Option<Rect>,
    /// The last row, full width.
    pub status: Rect,
    /// Whether the side column leads with the SESSION card.
    pub session_card: bool,
}

impl Grid {
    pub(super) fn new(area: Rect, show_sidebar: bool) -> Self {
        let status = Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1);
        let body = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1));
        let fits = show_sidebar && area.width >= SIDE_MIN_WINDOW && area.height >= SIDE_MIN_HEIGHT;
        if !fits {
            return Self {
                main: body,
                side: None,
                status,
                session_card: false,
            };
        }
        let side_w = if area.width >= WIDE_WINDOW {
            SIDE_WIDE
        } else {
            SIDE_REGULAR
        };
        // One blank column between the columns: the two boxes' own edges
        // already separate them, and a wider gap is a gutter with nothing in it.
        Self {
            main: Rect::new(body.x, body.y, body.width - side_w - 1, body.height),
            side: Some(Rect::new(
                body.right() - side_w,
                body.y,
                side_w,
                body.height,
            )),
            status,
            session_card: area.height >= SESSION_MIN_HEIGHT,
        }
    }
}

pub(super) fn draw_main(frame: &mut Frame, app: &mut App, area: Rect) {
    app.composer_palette = None;
    app.composer_palette_rows.clear();
    app.question_rows.clear();
    app.sidebar_area = None;
    app.sidebar_tabs.clear();
    app.sidebar_pages_area = None;
    if app.is_home() {
        draw_home(frame, app, area);
        return;
    }
    // The next home screen plays its opening again.
    app.home_started = None;
    let grid = Grid::new(area, app.show_sidebar);
    draw_main_column(frame, app, grid.main);
    if let Some(side) = grid.side {
        draw_sidebar(frame, app, side, grid.session_card);
    }
    // The SESSION card's figures are said once: in the card when it is on
    // screen, in the status bar when it is not.
    let figures_elsewhere = grid.side.is_some() && grid.session_card;
    draw_footer(frame, app, grid.status, !figures_elsewhere);
}

/// Columns between a box's border and its content, on each side.
pub(super) const PAD_X: u16 = 2;
/// Rows between a box's border and its content, top and bottom.
pub(super) const PAD_Y: u16 = 1;

/// The content area inside a box: `inner` (the box minus its border) with
/// `PAD_X` columns each side and, when `vertical`, `PAD_Y` rows top and
/// bottom. Text resting against the border reads as cramped and makes the
/// border look like part of the text.
///
/// A box too small to afford it gives up the padding rather than its content:
/// one column each side below 24 columns, no rows below five.
pub(super) fn padded(inner: Rect, vertical: bool) -> Rect {
    let px = if inner.width >= 24 {
        PAD_X
    } else if inner.width >= 6 {
        1
    } else {
        0
    };
    let py = if vertical && inner.height >= 2 * PAD_Y + 3 {
        PAD_Y
    } else {
        0
    };
    Rect::new(
        inner.x + px,
        inner.y + py,
        inner.width.saturating_sub(2 * px),
        inner.height.saturating_sub(2 * py),
    )
}

/// A rounded box on the panel surface. Every box in the window is drawn by
/// this, so they share one border, one fill and one corner.
pub(super) fn panel_box(
    frame: &mut Frame,
    area: Rect,
    border: ratatui::style::Color,
    fill: ratatui::style::Color,
) -> Rect {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(fill));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

/// A title set into a box's top edge after one run of the rule, `╭─ title ─`,
/// the way the detail card's tabs sit in its edge.
pub(super) fn box_title(
    frame: &mut Frame,
    area: Rect,
    spans: Vec<Span<'static>>,
    fill: ratatui::style::Color,
) {
    if area.width <= 7 || area.height == 0 {
        return;
    }
    let room = area.width.saturating_sub(5) as usize;
    let mut line = Line::from(
        std::iter::once(Span::raw(" "))
            .chain(spans)
            .chain(std::iter::once(Span::raw(" ")))
            .collect::<Vec<_>>(),
    );
    if line.width() > room {
        line = Line::styled(trim(&line.to_string(), room), line.style);
    }
    let width = line.width() as u16;
    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(fill)),
        Rect::new(area.x + 2, area.y, width, 1),
    );
}

/// Keys set into a box's bottom edge, `╰─ ↑↓ move · Esc close ─╯`, where the
/// sidebar keeps its pager. A row of hints inside the box cost a row of the
/// content it described.
pub(super) fn box_hint(
    frame: &mut Frame,
    area: Rect,
    hint: &str,
    colour: ratatui::style::Color,
    fill: ratatui::style::Color,
) {
    if hint.is_empty() || area.width <= 7 || area.height < 2 {
        return;
    }
    let text = trim(&format!(" {hint} "), area.width.saturating_sub(5) as usize);
    let width = text.chars().count() as u16;
    frame.render_widget(
        Paragraph::new(Line::styled(text, Style::default().fg(colour).bg(fill))),
        Rect::new(area.x + 2, area.bottom() - 1, width, 1),
    );
}

/// A centred overlay: the same rounded box as the layout's own, with an accent
/// edge because it has the keyboard, its title in the top edge and its keys in
/// the bottom one. `content_rows` is what the caller has to show; the border
/// and the padding are added here, so no caller counts them. Returns the box
/// and its padded content area.
pub(super) fn overlay(
    frame: &mut Frame,
    app: &App,
    width: u16,
    content_rows: u16,
    title: &str,
    hint: &str,
) -> (Rect, Rect) {
    let t = app.theme;
    let area = frame.area();
    let width = width.min(area.width.saturating_sub(4));
    let height = (content_rows + 2 + 2 * PAD_Y).min(area.height.saturating_sub(2));
    let rect = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, rect);
    panel_box(frame, rect, t.accent, t.panel);
    let title = title.trim();
    if !title.is_empty() {
        box_title(
            frame,
            rect,
            vec![Span::styled(
                title.to_owned(),
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            )],
            t.panel,
        );
    }
    box_hint(frame, rect, hint, t.muted, t.panel);
    let inner = Rect::new(
        rect.x + 1,
        rect.y + 1,
        rect.width.saturating_sub(2),
        rect.height.saturating_sub(2),
    );
    (rect, padded(inner, true))
}

pub(super) fn draw_footer(frame: &mut Frame, app: &App, area: Rect, show_figures: bool) {
    let t = app.theme;
    // Inset by a border and the padding, so the status bar's first and last
    // characters sit on the same columns as the markers inside the boxes above.
    let inset = 1 + PAD_X;
    if area.width < 2 * inset + 4 {
        return;
    }
    let area = Rect::new(area.x + inset, area.y, area.width - 2 * inset, area.height);

    // LEFT: spinner (busy only) + agent + model. Idle just shows agent + model.
    let mut left_spans: Vec<Span<'static>> = Vec::new();
    // Viewing a branch replaces the footer rather than decorating it: the
    // session marker belongs to the main conversation, which is not what is
    // on screen, and leaving it there put two status dots side by side.
    if let Some(viewing) = app.viewing.as_ref() {
        // The transcript on screen is not the conversation, so the footer has
        // to say so — otherwise a scrollback of someone else's tool calls
        // looks like the main session having gone strange.
        let state = app
            .delegations
            .get(viewing.index)
            .map(|d| d.state)
            .unwrap_or(crate::app::DelegationState::Finished);
        let running = state == crate::app::DelegationState::Running;
        let colour = match state {
            crate::app::DelegationState::Running => t.yellow,
            crate::app::DelegationState::Finished => t.green,
            crate::app::DelegationState::Failed => t.red,
        };
        // A spinner rather than a still marker: "is it working" is the
        // question someone watching a sub-agent has, and only movement
        // answers it.
        left_spans.push(Span::styled(
            if running {
                format!("{} ", app.spinner())
            } else {
                format!("{} ", state.marker())
            },
            Style::default().fg(colour),
        ));
        left_spans.push(Span::styled(
            format!("{} ", viewing.agent),
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        ));
        left_spans.push(Span::styled(
            match state {
                crate::app::DelegationState::Running => "working",
                crate::app::DelegationState::Finished => "finished",
                crate::app::DelegationState::Failed => "failed",
            },
            Style::default().fg(colour),
        ));
        left_spans.push(Span::styled(" · Esc back", Style::default().fg(t.muted)));
        // The conversation carries on behind this. Say when it has, or
        // staying here to read the work looks like being stuck in it.
        // A delegation's report lands on its existing row, so it is counted
        // on its own: the block count alone would miss it.
        let moved_on = viewing.blocks.len().saturating_sub(viewing.blocks_at_open)
            + crate::app::reports_in(&viewing.blocks).saturating_sub(viewing.reports_at_open);
        let right = Line::from(vec![if moved_on > 0 {
            Span::styled(
                format!("main chat +{moved_on} ↑"),
                Style::default().fg(t.yellow).add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(
                if running {
                    "sub-agent · live".to_string()
                } else {
                    "sub-agent transcript".to_string()
                },
                Style::default().fg(t.muted),
            )
        }]);
        draw_split_line(frame, Line::from(left_spans), right, area);
        return;
    }
    let figures = show_figures && area.width >= 66 && app.context_window > 0;
    left_spans.extend(status_spans(app));
    draw_split_line(
        frame,
        Line::from(left_spans),
        Line::from(key_spans(app, figures)),
        area,
    );
}

/// The status bar with only its keys, on the columns it always has them.
/// The home screen shows the rest of it under its composer.
pub(super) fn draw_keys(frame: &mut Frame, app: &App, area: Rect) {
    let inset = 1 + PAD_X;
    if area.width < 2 * inset + 4 {
        return;
    }
    let area = Rect::new(area.x + inset, area.y, area.width - 2 * inset, area.height);
    draw_split_line(
        frame,
        Line::default(),
        Line::from(key_spans(app, false)),
        area,
    );
}

/// The state, the agent and its model, and a command's last message: the
/// status bar's left side, and the line under the home screen's composer.
pub(super) fn status_spans(app: &App) -> Vec<Span<'static>> {
    let t = app.theme;
    let mut left_spans: Vec<Span<'static>> = Vec::new();
    // Segments rather than a sentence. Each block is one fact, read at a
    // glance and in a fixed place: state, then agent, then what the session
    // has cost. A run-on line of "· ·" separators makes the reader parse it.
    let (state_label, state_colour) = if app.question.is_some() {
        ("QUESTION", t.accent2)
    } else if app.busy {
        ("WORKING", t.yellow)
    } else if app.status == "failed" {
        ("FAILED", t.red)
    } else {
        ("READY", t.green)
    };
    left_spans.push(Span::styled(
        format!(" {state_label} "),
        Style::default()
            .fg(t.panel)
            .bg(state_colour)
            .add_modifier(Modifier::BOLD),
    ));
    left_spans.push(Span::styled(
        format!(
            " {} ",
            enowx_core::agent_def::display_name(app.active_agent())
        ),
        Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
    ));
    // Quieter than the agent and joined to it by a dot: the pair reads as one
    // block, with the agent — the part that changes hands — leading it.
    let model = app.model_label();
    left_spans.push(Span::styled(
        format!(
            "· {}",
            if model.is_empty() {
                "no model".to_owned()
            } else {
                model
            }
        ),
        Style::default().fg(t.muted),
    ));
    if app.busy {
        left_spans.push(Span::styled(
            format!(
                "  {}",
                crate::app::fmt_elapsed(app.turn_started.elapsed().as_secs())
            ),
            Style::default().fg(t.yellow),
        ));
        // What the turn is doing right now — waiting on the model, thinking,
        // writing, or which tool is running. A clock alone cannot tell a slow
        // model from a long tool call.
        left_spans.push(Span::styled(
            format!(" · {}", app.activity.label()),
            Style::default().fg(t.muted),
        ));
    }
    // Idle, with specialists still at work: say so, or a finished-looking
    // status bar reads as though nothing is happening.
    let working = app
        .delegations
        .iter()
        .filter(|d| d.state == crate::app::DelegationState::Running)
        .count();
    if !app.busy && working > 0 {
        left_spans.push(Span::styled(
            format!(
                "  {working} {} working in the background",
                if working == 1 { "agent" } else { "agents" }
            ),
            Style::default().fg(t.yellow),
        ));
    }
    // The status line's own message, which is where a command's result lands.
    if !app.status.is_empty() && app.status != "ready" {
        left_spans.push(Span::styled(
            format!("  {}", crate::text::trim(&app.status, 40)),
            Style::default().fg(t.muted),
        ));
    }
    left_spans
}

/// The status bar's right side: the session's figures when `figures`, then
/// the keys that apply right now.
fn key_spans(app: &App, figures: bool) -> Vec<Span<'static>> {
    let t = app.theme;
    // The session's figures when no card is showing them, then the keys.
    // `draw_split_line` drops the whole right side rather than truncating
    // it, so the figures only come along when there is room.
    let mut right_spans: Vec<Span<'static>> = Vec::new();
    if figures {
        let pct = (app.context_tokens as u64 * 100 / app.context_window.max(1) as u64).min(999);
        right_spans.push(Span::styled(
            format!("ctx {pct}%"),
            Style::default().fg(if pct >= 85 { t.red } else { t.muted }),
        ));
        let cost = crate::pricing::cost_usd(&app.config, app.tokens_in, app.tokens_out, 0);
        right_spans.push(Span::styled(
            format!("  {}   ", crate::pricing::format_cost(&app.config, cost)),
            Style::default().fg(t.muted),
        ));
    }
    // The keys that apply right now, where the tip used to rotate. A tip is
    // read once; a key is looked up, and looking it up is the reason to keep
    // a row of chrome at all.
    let hints: &[(&str, &str)] = if app.question.is_some() {
        &[("Enter", "answer"), ("Esc", "stop")]
    } else if app.busy {
        &[("Ctrl+C", "stop")]
    } else {
        &[("Ctrl+P", "commands"), ("/", "run one")]
    };
    for (index, (key, what)) in hints.iter().enumerate() {
        if index > 0 {
            right_spans.push(Span::raw("   "));
        }
        right_spans.push(Span::styled(
            (*key).to_owned(),
            Style::default().fg(t.muted).add_modifier(Modifier::BOLD),
        ));
        right_spans.push(Span::styled(
            format!(" {what}"),
            Style::default().fg(t.faint),
        ));
    }
    right_spans
}

pub(super) fn draw_split_line(frame: &mut Frame, left: Line, right: Line, area: Rect) {
    let rw = if right.width() + left.width().min(20) + 2 <= area.width as usize {
        right.width() as u16
    } else {
        0
    };
    let lw = area.width.saturating_sub(if rw > 0 { rw + 2 } else { 0 });
    let left = if left.width() > lw as usize {
        Line::styled(trim(&left.to_string(), lw as usize), left.style)
    } else {
        left
    };
    frame.render_widget(
        Paragraph::new(left),
        Rect::new(area.x, area.y, lw, area.height),
    );
    if rw > 0 {
        frame.render_widget(
            Paragraph::new(right).alignment(Alignment::Right),
            Rect::new(area.right() - rw, area.y, rw, area.height),
        );
    }
}
