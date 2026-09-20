use super::*;

pub(super) fn draw_main(frame: &mut Frame, app: &mut App, area: Rect) {
    app.sidebar_area = None;
    app.sidebar_tabs.clear();
    app.sidebar_pages_area = None;
    let border = Block::default()
        .borders(if area.height <= 8 {
            Borders::NONE
        } else {
            Borders::ALL
        })
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(app.theme.border))
        .style(Style::default().bg(app.theme.panel));
    let inner = border.inner(area);
    frame.render_widget(border, area);
    // Three rows: the box's two edges and the row between them.
    let header = if inner.height >= 12 { 3 } else { 0 };
    let footer = if inner.height >= 10 { 1 } else { 0 };
    // A blank row under the header box, matching the one above the composer:
    // a box resting directly on the first message reads as containing it.
    let gap = if header > 0 { 1 } else { 0 };
    let parts = Layout::vertical([
        Constraint::Length(header),
        Constraint::Length(gap),
        Constraint::Min(3),
        Constraint::Length(footer),
    ])
    .split(inner);
    if header > 0 {
        draw_title(frame, app, parts[0]);
    }
    // The chat pane owns the window: telemetry only appears when both panes fit
    // side by side, so shrinking the terminal never buries the conversation.
    let sidebar = if app.show_sidebar && parts[2].width >= 100 && parts[2].height >= 12 {
        (parts[2].width * 2 / 5).clamp(38, 60)
    } else {
        0
    };
    if sidebar > 0 {
        let columns =
            Layout::horizontal([Constraint::Min(52), Constraint::Length(sidebar)]).split(parts[2]);
        draw_composer_pane(frame, app, columns[0]);
        draw_sidebar(frame, app, columns[1]);
        // Name each pane on the rule above it rather than spending a row on a
        // heading. The label says which half a key acts on, which is the only
        // question the divider leaves open.
        pane_label(frame, app, columns[0], "CHAT", false);
        pane_label(
            frame,
            app,
            columns[1],
            crate::ui::sidebar::TABS[app.sidebar_tab].1,
            true,
        );
    } else {
        draw_composer_pane(frame, app, parts[2]);
    }
    if footer > 0 {
        // The footer belongs to the chat pane, so it stops at the divider.
        // Running it the full width put a lighter band under the sidebar that
        // belonged to neither pane.
        let footer_area = if sidebar > 0 {
            Rect::new(
                parts[3].x,
                parts[3].y,
                parts[3].width.saturating_sub(sidebar),
                parts[3].height,
            )
        } else {
            parts[3]
        };
        debug_assert!(
            sidebar == 0
                || footer_area.x + footer_area.width == parts[2].x + parts[2].width - sidebar,
            "the footer must stop exactly at the divider column"
        );
        // Fill the strip beside it with the sidebar's own background so the
        // row reads as a continuation of the pane above, not as a gap.
        if sidebar > 0 {
            frame.render_widget(
                Block::default().style(Style::default().bg(app.theme.panel)),
                Rect::new(
                    footer_area.x + footer_area.width,
                    parts[3].y,
                    sidebar,
                    parts[3].height,
                ),
            );
        }
        draw_footer(frame, app, footer_area);
        // Carry the pane divider through the footer to the frame. The
        // divider is the sidebar's left border, so it ended where the
        // sidebar did — one row short of the bottom — and a vertical rule
        // stopping in mid-air reads as a rendering fault rather than as the
        // edge of a pane.
        if sidebar > 0 {
            // The divider is the sidebar's left border, so its column is the
            // one the sidebar starts in. Deriving it from the footer's width
            // instead put it one cell out, and the rule stopped a row short
            // of the frame with nothing continuing it.
            let x = parts[2].x + parts[2].width - sidebar;
            frame.render_widget(
                // The rule takes the SIDEBAR's background, not the footer's.
                // It marks the sidebar's edge, and painting it `subtle` left
                // one lighter cell standing proud of the dark column below —
                // a single-cell leak, but the eye finds it immediately.
                Paragraph::new("│")
                    .style(Style::default().fg(app.theme.border).bg(app.theme.panel)),
                Rect::new(x, parts[3].y, 1, parts[3].height),
            );
        }
    }
}

/// The header: a name badge, then the running context as a breadcrumb.
///
/// The three coloured dots and "PID 91731" that used to sit here were
/// decoration — nobody reads a pid, and the dots imitated a window chrome the
/// terminal already draws. What belongs at the top is what the next keystroke
/// acts on: which workspace, which agent, which model.
/// The header: a box, matching the composer at the other end of the window.
///
/// The pair frames the conversation between them, which a rule at the top
/// and a box at the bottom did not — the two edges read as different kinds of
/// thing. The agent, model and state are not repeated here; the status bar
/// carries those, and saying them twice left neither line able to say
/// anything else.
fn draw_title(frame: &mut Frame, app: &App, area: Rect) {
    let t = app.theme;
    frame.render_widget(Block::default().style(Style::default().bg(t.subtle)), area);

    // Inset by one column on each side, exactly as the composer's box is, so
    // the two line up down the window rather than nearly lining up.
    let boxed = Rect::new(
        area.x + 1,
        area.y,
        area.width.saturating_sub(2),
        area.height.min(3),
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(t.border))
        .style(Style::default().bg(t.subtle));
    let inner = block.inner(boxed);
    frame.render_widget(block, boxed);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let project = app
        .workspace
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();

    let mut left = vec![
        Span::styled(" ", Style::default()),
        Span::styled(
            project,
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        ),
    ];
    if !app.title.is_empty() {
        left.push(Span::styled("   ", Style::default().fg(t.faint)));
        left.push(Span::styled(
            crate::text::trim(&app.title, 48),
            Style::default().fg(t.muted),
        ));
    }

    draw_split_line(frame, Line::from(left), Line::default(), inner);
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let t = app.theme;
    frame.render_widget(Block::default().style(Style::default().bg(t.subtle)), area);

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
            Style::default()
                .fg(t.accent)
                .add_modifier(ratatui::style::Modifier::BOLD),
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
        let moved_on = viewing.blocks.len().saturating_sub(viewing.blocks_at_open);
        let right = Line::from(vec![if moved_on > 0 {
            Span::styled(
                format!("main chat +{moved_on} ↑"),
                Style::default()
                    .fg(t.yellow)
                    .add_modifier(ratatui::style::Modifier::BOLD),
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
    // Segments rather than a sentence. Each block is one fact, read at a
    // glance and in a fixed place: state, then agent, then what the session
    // has cost. A run-on line of "· ·" separators makes the reader parse it.
    let (state_label, state_colour) = if app.busy {
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
        format!(" {} ", app.active_agent()),
        Style::default()
            .fg(t.accent)
            .bg(t.active_tab)
            .add_modifier(Modifier::BOLD),
    ));
    // On the same surface as the agent, separated by a dot and set quieter:
    // the pair reads as one block, with the agent — the part that changes
    // hands — leading it.
    let model = app.model_label();
    left_spans.push(Span::styled(
        format!(
            "· {} ",
            if model.is_empty() {
                "no model".to_owned()
            } else {
                model
            }
        ),
        Style::default().fg(t.muted).bg(t.active_tab),
    ));
    if app.busy {
        left_spans.push(Span::styled(
            format!(
                " {} ",
                crate::app::fmt_elapsed(app.turn_started.elapsed().as_secs())
            ),
            Style::default().fg(t.yellow),
        ));
    }
    // The status line's own message, which is where a command's result lands.
    if !app.status.is_empty() && app.status != "ready" {
        left_spans.push(Span::styled(
            format!(" {}", crate::text::trim(&app.status, 40)),
            Style::default().fg(t.muted),
        ));
    }

    // RIGHT: the keys, and the session's spend when there is room for it.
    // `draw_split_line` drops the whole right side rather than truncating it,
    // so the counts are added only when they will not push the keys off —
    // the sidebar already carries them in full.
    let mut right_spans: Vec<Span<'static>> = Vec::new();
    // The footer stops at the pane divider, so this is the chat pane's width
    // rather than the window's — a window wide enough for a sidebar leaves
    // the footer about sixty columns.
    let roomy = area.width >= 66;
    if roomy && app.context_window > 0 && app.context_tokens > 0 {
        let pct = (app.context_tokens as u64 * 100 / app.context_window.max(1) as u64).min(999);
        right_spans.push(Span::styled(
            format!("ctx {pct}%  "),
            Style::default().fg(if pct >= 85 { t.red } else { t.faint }),
        ));
    }
    // The keys that apply right now, where the tip used to rotate. A tip is
    // read once; a key is looked up, and looking it up is the reason to keep
    // a row of chrome at all.
    let hints: &[(&str, &str)] = if app.viewing.is_some() {
        &[("Esc", "back")]
    } else if app.busy {
        &[("Ctrl+C", "stop")]
    } else {
        &[("Ctrl+P", "commands"), ("/", "run one")]
    };
    for (key, what) in hints {
        right_spans.push(Span::styled(
            (*key).to_owned(),
            Style::default().fg(t.muted).add_modifier(Modifier::BOLD),
        ));
        right_spans.push(Span::styled(
            format!(" {what}  "),
            Style::default().fg(t.faint),
        ));
    }

    draw_split_line(frame, Line::from(left_spans), Line::from(right_spans), area);
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

/// A pane's name, sitting on the rule at its top edge.
///
/// Borrowed from how a bordered box carries its title: the label belongs to
/// the line, not to a row of its own. With two panes and a header already
/// taking three rows, a heading row each would cost a tenth of a short
/// terminal.
fn pane_label(frame: &mut Frame, app: &App, area: Rect, label: &str, right: bool) {
    // Two rows up: the blank row under the header box, then the box's own
    // lower edge, which is the line separating the header from the panes.
    // On a blank row the label floats and reads as a heading.
    if area.width < 24 || area.y < 2 {
        return;
    }
    let t = app.theme;
    let text = format!(" {label} ");
    let width = text.chars().count() as u16;
    // The header box is inset one column from the window frame, so its
    // corners sit at x+1 and at the far edge minus one. Clear both: a label
    // written over `╰` or `╯` makes the box look broken rather than titled.
    let x = if right {
        area.x + area.width.saturating_sub(width + 3)
    } else {
        area.x + 3
    };
    frame.render_widget(
        Paragraph::new(Line::styled(
            text,
            Style::default()
                .fg(t.faint)
                .bg(t.subtle)
                .add_modifier(Modifier::BOLD),
        )),
        Rect::new(x, area.y - 2, width.min(area.width), 1),
    );
}
