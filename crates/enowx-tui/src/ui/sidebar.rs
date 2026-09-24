use super::*;
use crate::text::thousands;
use enowx_core::ROLES;

pub(super) const TABS: [(&str, &str); 5] = [
    ("1", "TOKENS"),
    ("2", "TOOLS"),
    ("3", "SKILLS"),
    ("4", "AGENT"),
    ("5", "LOGS"),
];

pub(super) fn draw_sidebar(frame: &mut Frame, app: &mut App, area: Rect) {
    if area.width < 24 || area.height < 6 {
        return;
    }
    let t = app.theme;
    app.sidebar_area = Some(area);
    // A box of its own rather than a strip hanging off a divider, inset one
    // column on the right so it lines up with the header and the composer.
    // The three of them then read as the same kind of object, which is what
    // stops the sidebar looking like a severed remnant of the chrome above.
    let boxed = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.border))
        .style(Style::default().bg(t.panel));
    let inner = block.inner(boxed);
    frame.render_widget(block, boxed);
    // The tab's name on the box's own top edge, the way a bordered widget
    // titles itself. It used to sit on the header's lower edge, which now
    // spans the full width and belongs to neither pane.
    if boxed.width > 8 {
        frame.render_widget(
            Paragraph::new(Line::styled(
                format!(" {} ", TABS[app.sidebar_tab].1),
                Style::default()
                    .fg(t.accent)
                    .bg(t.panel)
                    .add_modifier(Modifier::BOLD),
            )),
            Rect::new(boxed.x + 2, boxed.y, boxed.width.saturating_sub(4), 1),
        );
    }
    if inner.width == 0 || inner.height < 3 {
        return;
    }

    // Two rows: the labels, and an underline marking the active one. The
    // third was a blank lead-in that bought nothing — the header above
    // already separates the strip from the window edge.
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(inner);
    draw_tabs(frame, app, rows[0]);

    // One column of padding rather than two: the box's own border already
    // holds the contents off the edge, so the old inset would have indented
    // everything twice.
    let body = rows[1].inner(Margin {
        horizontal: 1,
        vertical: 0,
    });
    app.delegation_rects.clear();
    let (lines, delegation_markers) = sidebar_lines(app, body.width as usize);
    let page_size = body.height.max(1) as usize;
    app.sidebar_pages = lines.len().div_ceil(page_size).max(1);
    app.sidebar_page = app.sidebar_page.min(app.sidebar_pages - 1);
    frame.render_widget(
        Paragraph::new(
            lines
                .into_iter()
                .skip(app.sidebar_page * page_size)
                .take(page_size)
                .collect::<Vec<_>>(),
        ),
        body,
    );

    // Markers are line indices into the whole tab; only the page on screen
    // can be clicked, so the rest are dropped rather than mapped to rows the
    // user cannot see. Two rows each: the name and the task under it.
    let first = app.sidebar_page * page_size;
    for (line, index) in delegation_markers {
        for offset in 0..2 {
            let line = line + offset;
            if line < first || line >= first + page_size {
                continue;
            }
            let y = body.y + (line - first) as u16;
            if y < body.bottom() {
                app.delegation_rects
                    .push((Rect::new(body.x, y, body.width, 1), index));
            }
        }
    }

    app.sidebar_pages_area = Some(rows[2]);
    // Only paging lives here; global shortcuts already sit in the window footer.
    let footer = if app.sidebar_pages > 1 {
        format!(
            "◀ Alt+←   page {}/{}   Alt+→ ▶",
            app.sidebar_page + 1,
            app.sidebar_pages
        )
    } else {
        String::new()
    };
    // The pager sits on the sidebar's own background. Painting it `subtle`
    // put a lighter band across the bottom of the pane that belonged to
    // neither the sidebar above it nor the window footer below, so the
    // sidebar looked like it had a footer of its own.
    frame.render_widget(
        Paragraph::new(footer)
            .alignment(Alignment::Center)
            .style(Style::default().fg(t.muted).bg(t.panel)),
        rows[2],
    );
}

fn draw_tabs(frame: &mut Frame, app: &mut App, area: Rect) {
    let t = app.theme;
    // On the box's own surface, and with no rule under it. The strip used to
    // sit on `subtle` with a bottom border; inside a box that reads as a band
    // of foreign colour, and the rule is the one separator that disappears in
    // a low-contrast theme. The selected tab's own fill marks the strip's
    // extent, and a blank row below it does the separating.
    frame.render_widget(
        Block::default().style(Style::default().bg(t.panel)),
        area,
    );
    // Full names need ~44 columns; below that the number alone still identifies
    // the tab and keeps every target the same clickable width.
    // Bare numbers are unreadable as a strip — nothing says what tab 3 is —
    // so the words stay for as long as they fit at all. The box's top edge
    // names only the selected one.
    let compact = area.width < 44;
    for (index, (key, name)) in TABS.iter().enumerate() {
        let start = area.x + area.width * index as u16 / 5;
        let end = area.x + area.width * (index as u16 + 1) / 5;
        let rect = Rect::new(start, area.y, end.saturating_sub(start), 2);
        app.sidebar_tabs.push((rect, index));
        let active = index == app.sidebar_tab;
        let style = Style::default()
            .fg(if active { t.accent } else { t.muted })
            .bg(if active { t.active_tab } else { t.panel });
        let label = if compact {
            (*key).to_owned()
        } else {
            format!("{key} {name}")
        };
        frame.render_widget(
            Paragraph::new(Line::styled(
                trim(&label, rect.width as usize),
                if active {
                    style.add_modifier(Modifier::BOLD)
                } else {
                    style
                },
            ))
            .alignment(Alignment::Center)
            .style(style),
            Rect::new(rect.x, rect.y, rect.width, 1),
        );
        if active && rect.width > 0 {
            frame.render_widget(
                Paragraph::new("─".repeat(rect.width as usize))
                    .style(Style::default().fg(t.accent).bg(t.active_tab)),
                Rect::new(rect.x, area.y + 1, rect.width, 1),
            );
        }
    }
}

/// One roster entry: the name, on one row.
///
/// It used to carry the agent's one-line description under it. At the
/// sidebar's 38–60 columns a roster of sixteen then ran to forty-odd rows of
/// prose in the pane that also has to show a delegation running right now —
/// and the list exists to be scanned for a name, not read.
fn agent_entry(
    lines: &mut Vec<Line<'static>>,
    agent: &enowx_core::AgentDef,
    active: bool,
    width: usize,
    theme: &Theme,
) {
    let marker = if active { "▸ " } else { "  " };
    let mut name = vec![
        Span::styled(
            marker.to_owned(),
            Style::default().fg(if active { theme.accent } else { theme.faint }),
        ),
        Span::styled(
            trim(&agent.name, width.saturating_sub(9).max(1)),
            if active {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            },
        ),
    ];
    // The active agent is also named in full above; the tag is what makes it
    // findable in a list of sixteen without reading back up the pane.
    if active {
        name.push(Span::styled(
            "  active".to_owned(),
            Style::default().fg(theme.green),
        ));
    }
    lines.push(Line::from(name));
}

/// A section heading: the title alone, in the accent colour.
///
/// It used to be a rule with the title set into it. That reads well in a theme
/// with a visible border and vanishes in one without: `chrome_void` puts
/// `border` at RGB(36,36,54) over a `panel` of RGB(13,13,20), about 1.3:1, so
/// the rule was a line you could measure and not see — and the heading went
/// with it. Colour and weight carry the separation now, which every theme has.
fn heading(lines: &mut Vec<Line<'static>>, title: &str, _width: usize, theme: &Theme) {
    if !lines.is_empty() {
        lines.push(Line::default());
    }
    lines.push(Line::styled(
        title.to_owned(),
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    ));
}

/// A headline figure: the number first and bold, what it measures after it.
///
/// The number leads because it is what the eye is looking for — a label-left,
/// value-right row makes you read across the pane to find it, which is fine
/// for a list and wrong for the one figure the tab exists to show.
fn headline(
    lines: &mut Vec<Line<'static>>,
    value: &str,
    label: &str,
    theme: &Theme,
) {
    lines.push(Line::from(vec![
        Span::styled(
            value.to_owned(),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("  {label}"), Style::default().fg(theme.muted)),
    ]));
}

/// A figure and its name, aligned in a column so several read as a block.
fn figure(lines: &mut Vec<Line<'static>>, value: &str, label: &str, theme: &Theme) {
    // Ten columns fits a formatted cost (`$0.0000`) and a thousands-separated
    // count; past that the label simply starts later on that row rather than
    // the column breaking.
    lines.push(Line::from(vec![
        Span::styled(
            format!("{value:<10}"),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled(label.to_owned(), Style::default().fg(theme.muted)),
    ]));
}

/// A hint in the faint colour: what key does something here.
fn hint(lines: &mut Vec<Line<'static>>, text: &str, theme: &Theme) {
    lines.push(Line::default());
    lines.push(Line::styled(
        text.to_owned(),
        Style::default().fg(theme.faint),
    ));
}

/// The context bar, running the full width.
///
/// The percentage used to sit at its right end. It leads the tab as a headline
/// now, so repeating it here would say the same thing twice and cost the bar
/// seven columns of track.
fn gauge(percent: f64, width: usize, theme: &Theme) -> Line<'static> {
    let track = width.max(4);
    let filled = ((track as f64) * percent / 100.0).round() as usize;
    Line::from(vec![
        Span::styled(
            "█".repeat(filled),
            // Red once there is little room left: the bar is the one thing
            // here that says "compact soon", and a full bar in the accent
            // colour looks the same as an empty one at a glance.
            Style::default().fg(if percent >= 85.0 {
                theme.red
            } else {
                theme.accent
            }),
        ),
        Span::styled(
            "░".repeat(track.saturating_sub(filled)),
            Style::default().fg(theme.faint),
        ),
    ])
}

fn sidebar_lines(app: &App, width: usize) -> (Vec<Line<'static>>, Vec<(usize, usize)>) {
    let mut delegation_markers: Vec<(usize, usize)> = Vec::new();
    let t = &app.theme;
    let mut lines: Vec<Line<'static>> = Vec::new();
    let calls: usize = app.tool_counts.values().sum();

    match app.sidebar_tab {
        0 => {
            // How full the context is decides whether to compact, which is the
            // only thing anyone does about this tab. It leads.
            let window = app.context_window;
            let percent = if window > 0 {
                (100.0 * app.context_tokens as f64 / window as f64).min(100.0)
            } else {
                0.0
            };
            headline(
                &mut lines,
                &format!("{percent:.1}%"),
                "context terpakai",
                t,
            );
            lines.push(gauge(percent, width, t));
            lines.push(Line::styled(
                format!(
                    "{} / {} token",
                    thousands(app.context_tokens as u64),
                    thousands(window as u64)
                ),
                Style::default().fg(t.faint),
            ));

            lines.push(Line::default());
            let cost = crate::pricing::cost_usd(&app.config, app.tokens_in, app.tokens_out, 0);
            figure(
                &mut lines,
                &crate::pricing::format_cost(&app.config, cost),
                "biaya sesi",
                t,
            );
            figure(&mut lines, &calls.to_string(), "panggilan tool", t);
            figure(&mut lines, app.activity.label(), "status", t);
            // Only once it has done something: a row reading "0" on every
            // install that has never configured TypeSafe is noise.
            if app.trimmed_count > 0 {
                figure(
                    &mut lines,
                    &app.trimmed_count.to_string(),
                    &format!(
                        "hasil tool dipangkas · {} token hemat",
                        crate::text::thousands(app.trimmed_saved as u64)
                    ),
                    t,
                );
            }

            // `Per 1M in`/`out` were here. They are the model's price list,
            // fixed for the whole session and already in `/provider` — a
            // number nobody acts on, printed where the ones they do act on
            // have to be found among them. `Input`/`Output` of the last call
            // and a message count went the same way: the context bar above
            // already says how much room is left, which is the question they
            // were being read to answer.
            hint(&mut lines, "1–5 pindah tab", t);
        }
        1 => {
            let failed = app
                .blocks
                .iter()
                .filter(|block| matches!(block.kind, TranscriptKind::Tool { error: true, .. }))
                .count();
            headline(&mut lines, &calls.to_string(), "panggilan tool", t);
            // A failure is the one thing here worth interrupting for, so it is
            // only shown when there is one — and in red when there is.
            if failed > 0 {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{failed:<10}"),
                        Style::default().fg(t.red).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("gagal", Style::default().fg(t.red)),
                ]));
            }

            // Only the tools that have actually been called, busiest first.
            // The full roster was sixteen rows of `0 calls` that said nothing
            // about this session; what it was really carrying is which tools
            // this agent cannot use, and that is the short list below.
            heading(&mut lines, "DIPAKAI", width, t);
            let mut used: Vec<(&str, usize)> = ROLES[0]
                .allowed_tools()
                .iter()
                .map(|name| (*name, app.tool_counts.get(*name).copied().unwrap_or(0)))
                .filter(|(_, count)| *count > 0)
                .collect();
            used.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
            if used.is_empty() {
                lines.push(Line::styled(
                    "belum ada".to_owned(),
                    Style::default().fg(t.faint),
                ));
            } else {
                for (name, count) in used {
                    figure(&mut lines, &count.to_string(), name, t);
                }
            }

            let blocked: Vec<&str> = ROLES[0]
                .allowed_tools()
                .iter()
                .filter(|name| !app.role.allowed_tools().contains(*name))
                .copied()
                .collect();
            if !blocked.is_empty() {
                heading(&mut lines, "DIBLOKIR", width, t);
                for name in blocked {
                    lines.push(Line::styled(
                        name.to_owned(),
                        Style::default().fg(t.muted),
                    ));
                }
            }

            if !app.discovery.mcp_servers.is_empty() {
                heading(&mut lines, "MCP", width, t);
                // The server and how many tools it brought, one row each. The
                // tool names themselves were up to twelve rows per server —
                // a catalogue, read once when wiring the server up and never
                // again, filling the pane for every session after that.
                for server in &app.discovery.mcp_servers {
                    if server.enabled {
                        let count = app.agent.mcp_tools(&server.name).len();
                        figure(&mut lines, &count.to_string(), &server.name, t);
                    } else {
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("{:<10}", "mati"),
                                Style::default().fg(t.faint),
                            ),
                            Span::styled(
                                server.name.clone(),
                                Style::default().fg(t.muted),
                            ),
                        ]));
                    }
                }
            }
            hint(&mut lines, "1–5 pindah tab", t);
        }
        2 => {
            let skills = &app.discovery.skills;
            headline(&mut lines, &skills.len().to_string(), "skill tersedia", t);
            // Shadowing is a problem to fix, not a count to note: two skills
            // of one name means the wrong one may load.
            if !app.discovery.shadowed_skills.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{:<10}", app.discovery.shadowed_skills.len()),
                        Style::default().fg(t.yellow).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("tertimpa nama sama", Style::default().fg(t.yellow)),
                ]));
            }

            if skills.is_empty() {
                hint(&mut lines, "tidak ada skill di workspace ini atau ~/", t);
            } else {
                // Project skills first: those are this workspace's own, and
                // the ones a user scans for. The scope tag is dropped from the
                // row and carried by the grouping instead.
                let mut project: Vec<&str> = Vec::new();
                let mut user: Vec<&str> = Vec::new();
                for skill in skills.iter() {
                    match skill.scope {
                        enowx_core::SkillScope::Project => project.push(&skill.name),
                        enowx_core::SkillScope::User => user.push(&skill.name),
                    }
                }
                for (title, group) in [("PROYEK INI", project), ("GLOBAL", user)] {
                    if group.is_empty() {
                        continue;
                    }
                    heading(&mut lines, title, width, t);
                    for name in group.iter().take(40) {
                        lines.push(Line::styled(
                            (*name).to_owned(),
                            Style::default().fg(t.text),
                        ));
                    }
                    if group.len() > 40 {
                        lines.push(Line::styled(
                            format!("+{} lagi", group.len() - 40),
                            Style::default().fg(t.faint),
                        ));
                    }
                }
            }
            hint(&mut lines, "dimuat lewat tool skill_read", t);
        }
        3 => {
            let active = app.active_agent();
            // Who is answering, and whether they are working. The model and
            // the step limit were here too; both are in the status bar and
            // `/model`, and neither changes while you watch this tab.
            headline(&mut lines, active, app.activity.label(), t);

            // Delegations first: a sixteen-agent roster is reference
            // material, while a sub-agent that is running right now is the
            // thing someone opens this tab to look at.
            heading(&mut lines, "DIDELEGASIKAN", width, t);
            if app.delegations.is_empty() {
                lines.push(Line::styled(
                    "belum ada".to_owned(),
                    Style::default().fg(t.faint),
                ));
            } else {
                for (index, delegation) in app.delegations.iter().enumerate() {
                    delegation_markers.push((lines.len(), index));
                    let colour = match delegation.state {
                        crate::app::DelegationState::Running => t.yellow,
                        crate::app::DelegationState::Finished => t.green,
                        crate::app::DelegationState::Failed => t.red,
                    };
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("{} ", delegation.state.marker()),
                            Style::default().fg(colour),
                        ),
                        Span::styled(
                            trim(&delegation.agent, width.saturating_sub(4).max(1)),
                            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
                        ),
                    ]));
                    // The task says which delegation this is when the same
                    // agent has been used more than once.
                    lines.push(Line::styled(
                        format!(
                            "  {}",
                            trim(&delegation.task, width.saturating_sub(2).max(1))
                        ),
                        Style::default().fg(t.faint),
                    ));
                }
                lines.push(Line::styled(
                    "klik untuk buka · Esc kembali".to_owned(),
                    Style::default().fg(t.faint),
                ));
            }

            // The roster, names only. The one-line description each carried
            // was reference material for choosing an agent by hand — sixteen
            // rows of prose under sixteen names, in a pane that has to show a
            // running delegation above them.
            let roster = &app.discovery.agents;
            heading(
                &mut lines,
                &format!("ROSTER · {}", roster.len()),
                width,
                t,
            );
            for agent in roster {
                agent_entry(&mut lines, agent, agent.name == active, width, t);
            }
            hint(&mut lines, "1–5 pindah tab", t);
        }
        _ => {
            // What happened this session. The transcript says what was said;
            // this says what the harness did — which is what answers "why
            // did it stop there?" once the status line has moved on.
            let filter = crate::logs::FILTERS[app.log_filter % crate::logs::FILTERS.len()];
            let entries = app.logs.matching(filter);
            headline(
                &mut lines,
                &entries.len().to_string(),
                &match filter {
                    None => "baris log".to_owned(),
                    Some(kind) => format!("baris · saring {}", kind.label()),
                },
                t,
            );

            if entries.is_empty() {
                lines.push(Line::styled(
                    "belum ada yang tercatat".to_owned(),
                    Style::default().fg(t.faint),
                ));
            }
            lines.push(Line::default());
            // Newest last, so the eye lands on the most recent without
            // scrolling — the same way the transcript reads.
            for entry in entries {
                let colour = match entry.kind {
                    crate::logs::LogKind::Agent => t.accent,
                    crate::logs::LogKind::Model => t.muted,
                    crate::logs::LogKind::Problem => t.red,
                    crate::logs::LogKind::Context => t.green,
                };
                let stamp = crate::logs::since(app.started, entry.at);
                lines.push(Line::from(vec![
                    Span::styled(format!("{stamp} "), Style::default().fg(t.faint)),
                    Span::styled(
                        format!("{} ", entry.kind.marker()),
                        Style::default().fg(colour),
                    ),
                    Span::styled(
                        trim(&entry.text, width.saturating_sub(stamp.len() + 3).max(1)),
                        Style::default().fg(t.text),
                    ),
                ]));
                if app.log_detail {
                    if let Some(detail) = entry.detail.as_deref() {
                        lines.push(Line::styled(
                            format!("      {}", trim(detail, width.saturating_sub(6).max(1))),
                            Style::default().fg(t.faint),
                        ));
                    }
                }
            }
            // At the end rather than the top: these two keys belong to this
            // tab and nowhere else, and putting them above the log pushed the
            // newest entry down by two rows on every render.
            hint(
                &mut lines,
                if app.log_detail {
                    "F6 saring · F7 ringkas"
                } else {
                    "F6 saring · F7 detail"
                },
                t,
            );
        }
    }
    (lines, delegation_markers)
}
