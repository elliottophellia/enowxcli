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
    // Three rows: the breadcrumb, the keys that apply now, and the rule.
    let header = if inner.height >= 12 {
        3
    } else if inner.height >= 10 {
        2
    } else {
        0
    };
    let footer = if inner.height >= 10 { 1 } else { 0 };
    let parts = Layout::vertical([
        Constraint::Length(header),
        Constraint::Min(3),
        Constraint::Length(footer),
    ])
    .split(inner);
    if header > 0 {
        draw_title(frame, app, parts[0]);
    }
    // The chat pane owns the window: telemetry only appears when both panes fit
    // side by side, so shrinking the terminal never buries the conversation.
    let sidebar = if app.show_sidebar && parts[1].width >= 100 && parts[1].height >= 12 {
        (parts[1].width * 2 / 5).clamp(38, 60)
    } else {
        0
    };
    if sidebar > 0 {
        let columns =
            Layout::horizontal([Constraint::Min(52), Constraint::Length(sidebar)]).split(parts[1]);
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
        draw_composer_pane(frame, app, parts[1]);
    }
    if footer > 0 {
        // The footer belongs to the chat pane, so it stops at the divider.
        // Running it the full width put a lighter band under the sidebar that
        // belonged to neither pane.
        let footer_area = if sidebar > 0 {
            Rect::new(
                parts[2].x,
                parts[2].y,
                parts[2].width.saturating_sub(sidebar),
                parts[2].height,
            )
        } else {
            parts[2]
        };
        // Fill the strip beside it with the sidebar's own background so the
        // row reads as a continuation of the pane above, not as a gap.
        if sidebar > 0 {
            frame.render_widget(
                Block::default().style(Style::default().bg(app.theme.panel)),
                Rect::new(
                    footer_area.x + footer_area.width,
                    parts[2].y,
                    sidebar,
                    parts[2].height,
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
            let x = parts[1].x + parts[1].width - sidebar;
            frame.render_widget(
                // The rule takes the SIDEBAR's background, not the footer's.
                // It marks the sidebar's edge, and painting it `subtle` left
                // one lighter cell standing proud of the dark column below —
                // a single-cell leak, but the eye finds it immediately.
                Paragraph::new("│")
                    .style(Style::default().fg(app.theme.border).bg(app.theme.panel)),
                Rect::new(x, parts[2].y, 1, parts[2].height),
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
fn draw_title(frame: &mut Frame, app: &App, area: Rect) {
    let t = app.theme;
    frame.render_widget(Block::default().style(Style::default().bg(t.subtle)), area);

    let project = app
        .workspace
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();

    // The badge: reversed rather than coloured, so it reads as a label
    // attached to the window rather than as another piece of status.
    let mut left = vec![
        Span::styled(
            " ❯_ ENX ",
            Style::default()
                .fg(t.panel)
                .bg(t.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
    ];

    let mut crumb = |icon: &str, label: String, colour: ratatui::style::Color| {
        left.push(Span::styled(
            format!("{icon} "),
            Style::default().fg(colour),
        ));
        left.push(Span::styled(label, Style::default().fg(t.text)));
        left.push(Span::styled("  ", Style::default().fg(t.faint)));
    };
    crumb("◆", project, t.accent);
    crumb("●", app.active_agent().to_owned(), t.green);
    let model = app.model_label();
    if !model.is_empty() && model != "no model" {
        crumb("⛁", model, t.accent2);
    }
    // The session's own name, once it has one — a resumed session is
    // otherwise indistinguishable from a fresh one.
    if !app.title.is_empty() {
        left.push(Span::styled("› ", Style::default().fg(t.faint)));
        left.push(Span::styled(
            crate::text::trim(&app.title, 28),
            Style::default().fg(t.muted),
        ));
    }

    let right = if app.busy {
        Line::from(vec![
            Span::styled(format!("{} ", app.spinner()), Style::default().fg(t.yellow)),
            Span::styled(
                crate::app::fmt_elapsed(app.turn_started.elapsed().as_secs()),
                Style::default().fg(t.yellow),
            ),
        ])
    } else {
        Line::from(Span::styled("? /help", Style::default().fg(t.faint)))
    };

    draw_split_line(
        frame,
        Line::from(left),
        right,
        Rect::new(area.x + 1, area.y, area.width.saturating_sub(2), 1),
    );

    // Second row: the keys that apply right now. A fixed list would be
    // reference material; this changes with what is on screen.
    if area.height >= 2 {
        let hints: &[(&str, &str)] = if app.viewing.is_some() {
            &[("Esc", "back"), ("↑↓", "scroll"), ("F4", "agents")]
        } else if app.busy {
            &[
                ("Ctrl+C", "stop"),
                ("Ctrl+P", "commands"),
                ("F1–F5", "tabs"),
            ]
        } else {
            &[
                ("Ctrl+P", "commands"),
                ("Ctrl+↑", "edit last"),
                ("F1–F5", "tabs"),
                ("/", "run a command"),
            ]
        };
        let mut spans: Vec<Span<'static>> = Vec::new();
        for (key, what) in hints {
            spans.push(Span::styled(
                (*key).to_owned(),
                Style::default().fg(t.muted).add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                format!(" {what}   "),
                Style::default().fg(t.faint),
            ));
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(t.subtle)),
            Rect::new(area.x + 1, area.y + 1, area.width.saturating_sub(2), 1),
        );
    }
    // The rule sits under the whole header, not through the hints.
    if area.height >= 3 {
        frame.render_widget(
            Paragraph::new("─".repeat(area.width as usize))
                .style(Style::default().fg(t.border).bg(t.subtle)),
            Rect::new(area.x, area.y + area.height - 1, area.width, 1),
        );
    }
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

    // RIGHT: what the session has used, then the rotating tip.
    let mut right_spans: Vec<Span<'static>> = Vec::new();
    if app.context_window > 0 && app.context_tokens > 0 {
        let pct = (app.context_tokens as u64 * 100 / app.context_window.max(1) as u64).min(999);
        right_spans.push(Span::styled(
            format!("ctx {pct}% "),
            Style::default().fg(if pct >= 85 { t.red } else { t.faint }),
        ));
    }
    if app.tokens_in > 0 || app.tokens_out > 0 {
        right_spans.push(Span::styled(
            format!(
                "↓{} ↑{} ",
                crate::text::thousands(app.tokens_in as u64),
                crate::text::thousands(app.tokens_out as u64)
            ),
            Style::default().fg(t.faint),
        ));
    }
    right_spans.push(Span::styled(
        app.footer_tip().to_string(),
        Style::default().fg(t.muted),
    ));

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
    if area.width < 24 || area.y == 0 {
        return;
    }
    let t = app.theme;
    let text = format!(" {label} ");
    let width = text.chars().count() as u16;
    let x = if right {
        area.x + area.width.saturating_sub(width + 2)
    } else {
        area.x + 2
    };
    frame.render_widget(
        Paragraph::new(Line::styled(
            text,
            Style::default()
                .fg(t.faint)
                .bg(t.subtle)
                .add_modifier(Modifier::BOLD),
        )),
        Rect::new(x, area.y - 1, width.min(area.width), 1),
    );
}
