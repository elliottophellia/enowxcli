use super::*;
use crate::text::thousands;
use enowx_core::ROLES;

/// The detail card's tabs, in the order the keys 1–4 select them.
///
/// Token figures are no longer a tab: they are what the side column is glanced
/// at for, so they sit in the SESSION card above, always visible, and a tab
/// that only repeated them would be a key press to learn nothing new.
pub(crate) const TABS: [&str; 4] = ["Agents", "Tools", "Skills", "Log"];
pub(crate) const AGENTS_TAB: usize = 0;
pub(crate) const LOG_TAB: usize = 3;

/// Width of the label column in the SESSION card, so the figures start on one
/// column and read down as a block.
const LABEL_COLUMN: usize = 9;

pub(super) fn draw_sidebar(frame: &mut Frame, app: &mut App, area: Rect, session_card: bool) {
    if area.width < 24 || area.height < 6 {
        return;
    }
    app.sidebar_area = Some(area);
    let detail = if session_card {
        // Border, one column of padding, then the text: the card's content
        // starts on its column 2 like everything else.
        let rows = session_rows(app, area.width.saturating_sub(4) as usize);
        let height = (rows.len() as u16 + 2).min(area.height);
        draw_session_card(frame, app, Rect::new(area.x, area.y, area.width, height), rows);
        Rect::new(
            area.x,
            area.y + height,
            area.width,
            area.height.saturating_sub(height),
        )
    } else {
        area
    };
    if detail.height >= 3 {
        draw_detail_card(frame, app, detail);
    }
}

fn draw_session_card(frame: &mut Frame, app: &App, area: Rect, rows: Vec<Line<'static>>) {
    let t = app.theme;
    let inner = panel_box(frame, area, t.border, t.panel);
    box_title(
        frame,
        area,
        vec![Span::styled(
            "SESSION",
            Style::default().fg(t.muted).add_modifier(Modifier::BOLD),
        )],
        t.panel,
    );
    frame.render_widget(
        Paragraph::new(rows),
        Rect::new(
            inner.x + 1,
            inner.y,
            inner.width.saturating_sub(2),
            inner.height,
        ),
    );
}

/// The figures someone glances at the side column for: how full the context
/// is, what the session has used and cost, and how many tools it has called.
fn session_rows(app: &App, width: usize) -> Vec<Line<'static>> {
    let t = &app.theme;
    let label = |text: &str| {
        Span::styled(
            format!("{text:<LABEL_COLUMN$}"),
            Style::default().fg(t.muted),
        )
    };
    let value = |text: String| Span::styled(text, Style::default().fg(t.text));
    let mut rows = Vec::new();

    // How full the context is decides whether to compact, which is the one
    // thing anyone does about these numbers, so it leads — and its bar turns
    // red once there is little room left, when a full bar in the accent
    // colour would look the same as an empty one at a glance.
    let window = app.context_window;
    let percent = if window > 0 {
        (100.0 * app.context_tokens as f64 / window as f64).min(100.0)
    } else {
        0.0
    };
    let figure = format!("{percent:.0}%");
    let track = width.saturating_sub(LABEL_COLUMN + 5).max(4);
    let filled = ((track as f64) * percent / 100.0).round() as usize;
    let bar_colour = if percent >= 85.0 { t.red } else { t.accent };
    rows.push(Line::from(vec![
        label("context"),
        Span::styled(
            format!("{figure:<5}"),
            Style::default().fg(bar_colour).add_modifier(Modifier::BOLD),
        ),
        Span::styled("▰".repeat(filled), Style::default().fg(bar_colour)),
        Span::styled(
            "▱".repeat(track.saturating_sub(filled)),
            Style::default().fg(t.faint),
        ),
    ]));
    rows.push(Line::from(vec![
        label("tokens"),
        value(format!(
            "{} / {}",
            thousands(app.context_tokens as u64),
            thousands(window as u64)
        )),
    ]));
    let cost = crate::pricing::cost_usd(&app.config, app.tokens_in, app.tokens_out, 0);
    rows.push(Line::from(vec![
        label("cost"),
        value(crate::pricing::format_cost(&app.config, cost)),
    ]));

    let calls: usize = app.tool_counts.values().sum();
    let failed = app
        .blocks
        .iter()
        .filter(|block| matches!(block.kind, TranscriptKind::Tool { error: true, .. }))
        .count();
    let mut tools = vec![
        label("tools"),
        value(format!("{calls} {}", if calls == 1 { "call" } else { "calls" })),
    ];
    // A failure is the one thing here worth interrupting for, so it only
    // appears when there is one, and in red.
    if failed > 0 {
        tools.push(Span::styled(
            format!("  {failed} failed"),
            Style::default().fg(t.red),
        ));
    }
    rows.push(Line::from(tools));
    // Only once it has done something: a row reading "0" on every install
    // that has never configured TypeSafe is noise.
    if app.trimmed_count > 0 {
        rows.push(Line::from(vec![
            label("trimmed"),
            // Short enough for the narrowest card: "4 results · 2,100 saved".
            value(format!(
                "{} {} · {} saved",
                app.trimmed_count,
                if app.trimmed_count == 1 { "result" } else { "results" },
                thousands(app.trimmed_saved as u64)
            )),
        ]));
    }
    rows
}

fn draw_detail_card(frame: &mut Frame, app: &mut App, area: Rect) {
    let t = app.theme;
    let inner = panel_box(frame, area, t.border, t.panel);
    draw_tabs(frame, app, area);
    if inner.width < 3 || inner.height == 0 {
        return;
    }

    let body = Rect::new(
        inner.x + 1,
        inner.y,
        inner.width.saturating_sub(2),
        inner.height,
    );
    app.delegation_rects.clear();
    let (lines, delegation_markers) = detail_lines(app, body.width as usize);
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

    // The pager sits in the card's bottom edge, right-aligned, so paging costs
    // no row of the card's content. A click on its left half goes back, on
    // its right half forward.
    if app.sidebar_pages > 1 {
        let pager = Line::from(vec![Span::styled(
            format!(" ◀ {}/{} ▶ ", app.sidebar_page + 1, app.sidebar_pages),
            Style::default().fg(t.muted),
        )]);
        let width = pager.width() as u16;
        if width + 4 <= area.width {
            let rect = Rect::new(
                area.right().saturating_sub(width + 2),
                area.bottom().saturating_sub(1),
                width,
                1,
            );
            frame.render_widget(
                Paragraph::new(pager).style(Style::default().bg(t.panel)),
                rect,
            );
            app.sidebar_pages_area = Some(rect);
        }
    }
}

/// The tabs, set into the card's top edge where a box's title goes.
///
/// The selected one is in the accent colour and bold, the rest are muted, so
/// the strip doubles as the card's name without a second label repeating it.
/// Each name is its own click target.
fn draw_tabs(frame: &mut Frame, app: &mut App, area: Rect) {
    let t = app.theme;
    let mut x = area.x + 2;
    let limit = area.right().saturating_sub(2);
    for (index, name) in TABS.iter().enumerate() {
        let active = index == app.sidebar_tab;
        let text = format!(" {name} ");
        let width = text.chars().count() as u16;
        if x + width > limit {
            break;
        }
        frame.render_widget(
            Paragraph::new(Line::styled(
                text,
                if active {
                    Style::default()
                        .fg(t.accent)
                        .bg(t.panel)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(t.muted).bg(t.panel)
                },
            )),
            Rect::new(x, area.y, width, 1),
        );
        app.sidebar_tabs.push((Rect::new(x, area.y, width, 1), index));
        x += width;
        if index + 1 < TABS.len() && x + 1 < limit {
            frame.render_widget(
                Paragraph::new(Line::styled("·", Style::default().fg(t.faint).bg(t.panel))),
                Rect::new(x, area.y, 1, 1),
            );
            x += 1;
        }
    }
}

/// A section heading: the title alone, in the accent colour.
///
/// Not a rule with the title set into it: separation here is carried by
/// colour and weight, which every theme has, rather than by a line.
fn heading(lines: &mut Vec<Line<'static>>, title: &str, theme: &Theme) {
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
fn headline(lines: &mut Vec<Line<'static>>, value: &str, label: &str, theme: &Theme) {
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
    lines.push(Line::from(vec![
        Span::styled(
            format!("{value:<6}"),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled(label.to_owned(), Style::default().fg(theme.muted)),
    ]));
}

fn note(lines: &mut Vec<Line<'static>>, text: &str, theme: &Theme) {
    lines.push(Line::styled(
        text.to_owned(),
        Style::default().fg(theme.faint),
    ));
}

/// Names laid out in rows two spaces apart, wrapping at the card's width.
///
/// The roster exists to be scanned for a name. One name per row spent sixteen
/// rows on it in a card that also has to show a running delegation.
///
/// The active agent carries `›` as well as the accent colour. Colour alone
/// disappears under NO_COLOR, and then nothing in sixteen names says which
/// one is answering.
fn flowed_names(
    lines: &mut Vec<Line<'static>>,
    names: &[(String, bool)],
    width: usize,
    theme: &Theme,
) {
    let mut row: Vec<Span<'static>> = Vec::new();
    let mut used = 0;
    for (name, active) in names {
        let name = if *active {
            format!("›{name}")
        } else {
            name.clone()
        };
        let w = name.chars().count();
        let gap = if row.is_empty() { 0 } else { 2 };
        if used + gap + w > width && !row.is_empty() {
            lines.push(Line::from(std::mem::take(&mut row)));
            used = 0;
        }
        if !row.is_empty() {
            row.push(Span::raw("  "));
            used += 2;
        }
        row.push(Span::styled(
            name,
            if *active {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            },
        ));
        used += w;
    }
    if !row.is_empty() {
        lines.push(Line::from(row));
    }
}

fn detail_lines(app: &App, width: usize) -> (Vec<Line<'static>>, Vec<(usize, usize)>) {
    let mut delegation_markers: Vec<(usize, usize)> = Vec::new();
    let t = &app.theme;
    let mut lines: Vec<Line<'static>> = Vec::new();

    match app.sidebar_tab {
        AGENTS_TAB => {
            // Delegations first: a sixteen-agent roster is reference material,
            // while a sub-agent running right now is what someone opens this
            // tab to look at.
            heading(&mut lines, "DELEGATED", t);
            if app.delegations.is_empty() {
                note(&mut lines, "Nothing delegated yet.", t);
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
                    // agent has been used more than once. Its first line
                    // only: a brief runs to dozens, and printing the whole of
                    // it glued its lines into one run of text.
                    let task = delegation
                        .task
                        .lines()
                        .map(str::trim)
                        .find(|line| !line.is_empty())
                        .unwrap_or("");
                    lines.push(Line::styled(
                        format!("  {}", trim(task, width.saturating_sub(2).max(1))),
                        Style::default().fg(t.faint),
                    ));
                }
                note(&mut lines, "click one to open · Esc returns", t);
            }

            // The roster, names only and flowed, with the active agent in the
            // accent colour so it can be found without reading the list.
            let roster = &app.discovery.agents;
            let active = app.active_agent();
            heading(&mut lines, &format!("ROSTER · {}", roster.len()), t);
            let names: Vec<(String, bool)> = roster
                .iter()
                .map(|agent| (agent.name.clone(), agent.name == active))
                .collect();
            flowed_names(&mut lines, &names, width, t);
        }
        1 => {
            let calls: usize = app.tool_counts.values().sum();
            headline(
                &mut lines,
                &calls.to_string(),
                if calls == 1 { "tool call" } else { "tool calls" },
                t,
            );

            // Only the tools that have actually been called, busiest first.
            // The full roster was sixteen rows of `0 calls` that said nothing
            // about this session; what it was really carrying is which tools
            // this agent cannot use, and that is the short list below.
            heading(&mut lines, "USED", t);
            let mut used: Vec<(&str, usize)> = ROLES[0]
                .allowed_tools()
                .iter()
                .map(|name| (*name, app.tool_counts.get(*name).copied().unwrap_or(0)))
                .filter(|(_, count)| *count > 0)
                .collect();
            used.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
            if used.is_empty() {
                note(&mut lines, "No tools called yet.", t);
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
                heading(&mut lines, "BLOCKED", t);
                for name in blocked {
                    lines.push(Line::styled(
                        name.to_owned(),
                        Style::default().fg(t.muted),
                    ));
                }
            }

            if !app.discovery.mcp_servers.is_empty() {
                heading(&mut lines, "MCP", t);
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
                            Span::styled(format!("{:<6}", "off"), Style::default().fg(t.faint)),
                            Span::styled(server.name.clone(), Style::default().fg(t.muted)),
                        ]));
                    }
                }
            }
        }
        2 => {
            let skills = &app.discovery.skills;
            headline(
                &mut lines,
                &skills.len().to_string(),
                if skills.len() == 1 { "skill" } else { "skills" },
                t,
            );
            // Shadowing is a problem to fix, not a count to note: two skills
            // of one name means the wrong one may load.
            if !app.discovery.shadowed_skills.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{:<6}", app.discovery.shadowed_skills.len()),
                        Style::default().fg(t.yellow).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "shadowed by a same-name skill",
                        Style::default().fg(t.yellow),
                    ),
                ]));
            }

            if skills.is_empty() {
                lines.push(Line::default());
                note(&mut lines, "No skills in this workspace or ~/.", t);
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
                for (title, group) in [("THIS PROJECT", project), ("GLOBAL", user)] {
                    if group.is_empty() {
                        continue;
                    }
                    heading(&mut lines, title, t);
                    for name in group.iter().take(40) {
                        lines.push(Line::styled(
                            (*name).to_owned(),
                            Style::default().fg(t.text),
                        ));
                    }
                    if group.len() > 40 {
                        note(&mut lines, &format!("+{} more", group.len() - 40), t);
                    }
                }
            }
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
                    None => if entries.len() == 1 { "entry" } else { "entries" }.to_owned(),
                    Some(kind) => format!("entries · filtered to {}", kind.label()),
                },
                t,
            );
            lines.push(Line::default());
            if entries.is_empty() {
                note(&mut lines, "Nothing logged yet.", t);
            }
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
            lines.push(Line::default());
            note(
                &mut lines,
                if app.log_detail {
                    "F6 filter · F7 summary"
                } else {
                    "F6 filter · F7 detail"
                },
                t,
            );
        }
    }
    (lines, delegation_markers)
}
