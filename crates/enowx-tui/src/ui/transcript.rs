use super::markdown::render_markdown;
use super::*;

pub(super) fn draw_welcome(frame: &mut Frame, app: &App, area: Rect) {
    let (title, hint) = if app.config.is_ready() {
        (
            "What are we working on?",
            "/resume to reopen a session · /help for commands",
        )
    } else if app.config.provider_active() && app.config.model.default.is_empty() {
        ("Choose a model", "Open /model to select a model.")
    } else {
        (
            "Connect a provider",
            "Open /provider to configure your connection.",
        )
    };
    let mut lines = Vec::new();
    if area.height > 5 {
        lines.push(Line::default());
    }
    for line in textwrap::wrap(title, area.width as usize) {
        lines.push(Line::styled(
            line.into_owned(),
            Style::default()
                .fg(app.theme.text)
                .add_modifier(Modifier::BOLD),
        ));
    }
    if area.height > 3 {
        lines.push(Line::default());
        for line in textwrap::wrap(hint, area.width as usize) {
            lines.push(Line::styled(
                line.into_owned(),
                Style::default().fg(app.theme.muted),
            ));
        }
    }
    frame.render_widget(
        Paragraph::new(lines),
        Rect::new(
            area.x + GUTTER as u16,
            area.y,
            area.width.saturating_sub(GUTTER as u16),
            area.height,
        ),
    );
}

/// Columns reserved at the transcript's left for markers: `▌` for the user,
/// `✓`/`✗` for a tool, `↳` for a handover. Every block's text starts after it,
/// so the conversation has one left edge however the blocks alternate.
pub(super) const GUTTER: usize = 2;

/// Render into a scratch buffer `GUTTER` narrower, then shift every line onto
/// the text column. The gutter keeps the panel's own colour, so a block with a
/// tinted background (code, a card) starts on the text column rather than
/// bleeding under the markers.
fn indented(
    lines: &mut Vec<Line<'static>>,
    theme: &Theme,
    width: usize,
    render: impl FnOnce(usize, &mut Vec<Line<'static>>),
) {
    let mut inner: Vec<Line<'static>> = Vec::new();
    render(width.saturating_sub(GUTTER).max(1), &mut inner);
    for mut line in inner {
        line.spans.insert(
            0,
            Span::styled(" ".repeat(GUTTER), Style::default().bg(theme.panel)),
        );
        lines.push(line);
    }
}

/// Shift every line from `from` onwards onto the text column.
fn gutter_from(lines: &mut [Line<'static>], from: usize, theme: &Theme) {
    for line in lines.iter_mut().skip(from) {
        line.spans.insert(
            0,
            Span::styled(" ".repeat(GUTTER), Style::default().bg(theme.panel)),
        );
    }
}

pub(super) fn draw_transcript(frame: &mut Frame, app: &mut App, area: Rect) {
    let theme = app.theme;
    let width = area.width.max(1) as usize;
    app.tool_header_markers.clear();
    app.tool_header_rects.clear();
    app.file_link_markers.clear();
    app.file_link_rects.clear();
    app.user_block_markers.clear();
    app.user_block_rects.clear();
    app.transcript_area = Some(area);
    // Re-render only the blocks whose key changed, then stitch the cached
    // pieces together. Streaming mutates just the last block, so a token
    // arriving mid-turn costs one block of parsing rather than the whole
    // transcript's — which is what made a long session slow down as it grew.
    refresh_render_cache(app, width);

    let mut lines: Vec<Line> = Vec::new();
    // Total height first, WITHOUT materialising any lines. Scroll position
    // and the scrollbar need the transcript's full length, but the frame only
    // ever shows `area.height` rows of it — cloning every line of a long
    // session to then draw thirty of them is what kept the cost proportional
    // to the conversation even once parsing was cached.
    let mut total: usize = 0;
    let mut compact_tools = false;
    for idx in 0..app.blocks.len() {
        let Some(cached) = app.render_cache.get(idx).and_then(|c| c.as_ref()) else {
            continue;
        };
        if cached.skipped {
            continue;
        }
        if compact_tools && !cached.is_tool {
            total += 1;
        }
        total += cached.lines.len();
        compact_tools = cached.is_tool;
    }

    // The last block's closing blank rows are spacing before a block that
    // has not arrived yet. At the bottom of the transcript they only doubled
    // the box's own bottom padding, so they are left below the scroll range.
    let trailing = (0..app.blocks.len())
        .rev()
        .filter_map(|idx| app.render_cache.get(idx).and_then(|c| c.as_ref()))
        .find(|cached| !cached.skipped)
        .map(|cached| {
            cached
                .lines
                .iter()
                .rev()
                .take_while(|line| is_blank_line(Some(line)))
                .count()
        })
        .unwrap_or(0);
    let total = total.saturating_sub(trailing);
    let content_height = total.min(u16::MAX as usize) as u16;
    let max_scroll = content_height.saturating_sub(area.height);
    app.max_scroll = max_scroll;
    if app.auto_scroll {
        app.scroll = max_scroll;
    } else {
        app.scroll = app.scroll.min(max_scroll);
    }

    // Now walk again and keep only the window the viewport will show. Markers
    // are rebased onto absolute transcript rows (what the click handlers
    // expect) even for blocks scrolled out of view, so a marker's row stays
    // comparable against `app.scroll`.
    let view_start = app.scroll as usize;
    let view_end = view_start.saturating_add(area.height as usize);
    let mut cursor: usize = 0;
    let mut compact_tools = false;
    for idx in 0..app.blocks.len() {
        let Some(cached) = app.render_cache.get(idx).and_then(|c| c.as_ref()) else {
            continue;
        };
        if cached.skipped {
            continue;
        }
        if compact_tools && !cached.is_tool {
            if cursor >= view_start && cursor < view_end {
                lines.push(Line::default());
            }
            cursor += 1;
        }
        let base = cursor;
        let headers: Vec<(String, usize)> = cached
            .tool_headers
            .iter()
            .map(|(id, off)| (id.clone(), base + off))
            .collect();
        let links: Vec<(usize, String)> = cached
            .file_links
            .iter()
            .map(|(off, path)| (base + off, path.clone()))
            .collect();
        let block_end = base + cached.lines.len();
        if block_end > view_start && base < view_end {
            let from = view_start.saturating_sub(base);
            let to = (view_end - base).min(cached.lines.len());
            lines.extend(cached.lines[from..to].iter().cloned());
        }
        cursor = block_end;
        compact_tools = cached.is_tool;
        app.tool_header_markers.extend(headers);
        app.file_link_markers.extend(links);
        // A user message is a point the conversation can be rewound to, so
        // every row it occupies is clickable — not just its first line.
        if matches!(app.blocks[idx].kind, TranscriptKind::User) {
            for row in base..block_end {
                app.user_block_markers.push((row, idx));
            }
        }
    }
    // `lines` now holds only the visible window, so everything downstream
    // indexes from the top of the viewport rather than the transcript.
    let view_offset = view_start;

    // Truncate any line that overshoots `width` instead of wrapping. Wrap
    // would push a continuation onto the next row without the gutter/marker
    // that the diff and card layouts rely on, leaving what look like blank
    // gap rows. A trailing `…` says content was elided.
    let mut wrapped: Vec<Line> = Vec::new();
    for line in lines {
        if line.width() <= width {
            wrapped.push(line);
            continue;
        }
        // Rebuild the line span-by-span, cutting when we hit `width - 1` so
        // the trailing `…` fits. Style is preserved per-span.
        let mut budget = width.saturating_sub(1);
        let mut out_spans: Vec<Span<'static>> = Vec::new();
        for span in line.spans.into_iter() {
            // `line.width()` above is a display width, so the budget has to be
            // spent in the same unit or a row of wide glyphs gets cut short.
            let span_w = unicode_width_of(&span.content);
            if span_w <= budget {
                budget -= span_w;
                out_spans.push(span);
                continue;
            }
            // Partial span: take as many chars as fit the remaining columns.
            let mut taken = String::new();
            let mut used = 0usize;
            for c in span.content.chars() {
                let w = unicode_width_of_char(c).max(1);
                if used + w > budget {
                    break;
                }
                used += w;
                taken.push(c);
            }
            out_spans.push(Span::styled(taken, span.style));
            break;
        }
        out_spans.push(Span::styled(
            "…".to_string(),
            ratatui::style::Style::default().fg(theme.muted),
        ));
        wrapped.push(Line::from(out_spans).style(line.style));
    }
    // Snapshot visible rows with the terminal row they occupy so a mouse
    // drag selection can extract exactly what the user saw. `wrapped` already
    // starts at the first visible row, so row 0 is the top of the viewport.
    app.wrapped_snapshot.clear();
    for (idx, line) in wrapped.iter().enumerate().take(area.height as usize) {
        let screen_y = area.y + idx as u16;
        app.wrapped_snapshot.push((screen_y, line.to_string()));
    }
    // Convert tool header markers to on-screen rects. Markers carry absolute
    // transcript rows, so subtracting the scroll gives the screen row; the
    // truncation pass above is one-to-one, so no index remapping is needed.
    for (id, marker) in std::mem::take(&mut app.tool_header_markers) {
        let screen_y = marker as i32 - view_offset as i32;
        if screen_y < 0 || screen_y >= area.height as i32 {
            continue;
        }
        let y = area.y + screen_y as u16;
        app.tool_header_rects
            .push((Rect::new(area.x, y, width as u16, 1), id));
    }
    for (marker, idx) in std::mem::take(&mut app.user_block_markers) {
        let screen_y = marker as i32 - view_offset as i32;
        if screen_y < 0 || screen_y >= area.height as i32 {
            continue;
        }
        let y = area.y + screen_y as u16;
        app.user_block_rects
            .push((Rect::new(area.x, y, width as u16, 1), idx));
    }
    // Convert file link markers to on-screen rects so a click can open the
    // file with the OS default app.
    for (marker, path) in std::mem::take(&mut app.file_link_markers) {
        let screen_y = marker as i32 - view_offset as i32;
        if screen_y < 0 || screen_y >= area.height as i32 {
            continue;
        }
        let y = area.y + screen_y as u16;
        app.file_link_rects
            .push((Rect::new(area.x, y, width as u16, 1), path));
    }
    // Apply active selection highlight before rendering.
    if let Some(sel) = app.selection {
        let (start, end) = normalize_selection(sel);
        for (idx, line) in wrapped.iter_mut().enumerate() {
            let row = area.y + idx as u16;
            if row < start.0 || row > end.0 {
                continue;
            }
            let bg = app.theme.active_tab;
            highlight_line(line, row, start, end, bg);
        }
    }
    // `wrapped` already begins at the first visible row, so the paragraph
    // draws from its own top — scrolling was applied when the window was cut.
    frame.render_widget(
        Paragraph::new(wrapped),
        Rect::new(area.x, area.y, width as u16, area.height),
    );
}

/// Normalize a selection so `start` is top-left and `end` is bottom-right.
fn normalize_selection(sel: crate::app::TextSelection) -> ((u16, u16), (u16, u16)) {
    let (a, b) = (sel.anchor, sel.head);
    if (a.0, a.1) <= (b.0, b.1) {
        (a, b)
    } else {
        (b, a)
    }
}

/// Paint the portion of `line` that falls inside `[start..=end]` with `bg`.
/// `row` is the terminal row the line renders on.
fn highlight_line(
    line: &mut Line<'static>,
    row: u16,
    start: (u16, u16),
    end: (u16, u16),
    bg: ratatui::style::Color,
) {
    let mut col: u16 = 0;
    for span in line.spans.iter_mut() {
        let span_start = col;
        let span_end = col + span.content.chars().count() as u16;
        col = span_end;
        let sel_start_col = if row == start.0 { start.1 } else { 0 };
        let sel_end_col = if row == end.0 { end.1 } else { u16::MAX };
        if span_end < sel_start_col || span_start > sel_end_col {
            continue;
        }
        // Partial highlight would need to split the span; for simplicity we
        // highlight the whole span when any part of it falls in range. Good
        // enough for whole-word / whole-line selection which is the common
        // case.
        span.style = span.style.bg(bg);
    }
}

/// User messages sit on a tinted band with an accent bar down the left, so
/// the eye can find them while scrolling. The markdown renderer already keeps
/// every row inside `body_w`, so rows keep their own styling (code, links)
/// and only take the band and the weight. Tall messages are cut with a
/// "▸ N more lines" row.
const USER_CARD_MAX_LINES: usize = 12;

fn user_card(lines: &mut Vec<Line<'static>>, text: &str, width: usize, theme: &Theme) {
    if width < 6 {
        return;
    }
    let body_w = width.saturating_sub(4);
    let mut inner: Vec<Line<'static>> = Vec::new();
    render_markdown(text, body_w, &mut inner, theme);
    if inner.is_empty() {
        inner.push(Line::default());
    }
    let overflow = inner.len().saturating_sub(USER_CARD_MAX_LINES);
    // A bar down the left rather than a box around it. A full border spends
    // two rows on rule and two columns on sides to say one thing, "the user
    // said this", and in a conversation that is every other block.
    let bar = Style::default().fg(theme.accent).bg(theme.subtle);
    let band = Style::default().bg(theme.subtle);
    for row in inner.into_iter().take(USER_CARD_MAX_LINES) {
        let used: usize = row.spans.iter().map(|s| unicode_width_of(&s.content)).sum();
        let mut spans = vec![Span::styled("▌ ", bar)];
        spans.extend(row.spans.into_iter().map(|span| {
            let style = span.style.bg(theme.subtle).add_modifier(Modifier::BOLD);
            Span::styled(span.content, style)
        }));
        // The band runs to the pane edge so the block reads as one surface
        // rather than as a ragged strip.
        spans.push(Span::styled(
            " ".repeat(body_w.saturating_sub(used) + 2),
            band,
        ));
        lines.push(Line::from(spans));
    }
    if overflow > 0 {
        let msg = format!(
            "▸ {overflow} more line{s} …",
            s = if overflow == 1 { "" } else { "s" }
        );
        let pad = body_w.saturating_sub(unicode_width_of(&msg));
        lines.push(Line::from(vec![
            Span::styled("▌ ", bar),
            Span::styled(msg, Style::default().fg(theme.muted).bg(theme.subtle)),
            Span::styled(" ".repeat(pad + 2), band),
        ]));
    }
}
fn unicode_width_of(text: &str) -> usize {
    use unicode_width::UnicodeWidthStr;
    text.width()
}

fn unicode_width_of_char(c: char) -> usize {
    use unicode_width::UnicodeWidthChar;
    c.width().unwrap_or(0)
}

fn args_path(args: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(args).ok()?;
    value
        .get("path")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

pub(super) fn register_summary_file_link(
    name: &str,
    args: &str,
    _text: &str,
    line_index: usize,
    markers: &mut Vec<(usize, String)>,
) {
    if !matches!(name, "read" | "write" | "skill_read" | "fetch") {
        return;
    }
    if let Some(path) = args_path(args) {
        markers.push((line_index, path));
    }
}

pub(super) fn register_detail_file_link(
    name: &str,
    args: &str,
    line_index: usize,
    markers: &mut Vec<(usize, String)>,
) {
    if !matches!(name, "write" | "edit") {
        return;
    }
    if let Some(path) = args_path(args) {
        markers.push((line_index, path));
    }
}

/// Render ONE transcript block into `lines`, recording click markers at
/// offsets relative to that block's first line.
///
/// Split out of `draw_transcript` so a block can be rendered once and reused
/// across frames. Takes plain buffers rather than `&mut App` because the cache
/// calls it while holding a borrow of the block list.
#[allow(clippy::too_many_arguments)]
fn render_block(
    block: &crate::session::TranscriptBlock,
    width: usize,
    theme: &Theme,
    show_reasoning: bool,
    show_tool_output: bool,
    error_repeats: usize,
    retry_attempt: u32,
    retry_max: u32,
    tool_expanded: &std::collections::HashMap<String, bool>,
    lines: &mut Vec<Line<'static>>,
    tool_headers: &mut Vec<(String, usize)>,
    file_links: &mut Vec<(usize, String)>,
) {
    match &block.kind {
        TranscriptKind::User => {
            user_card(lines, &block.text, width, theme);
        }
        TranscriptKind::Assistant => {
            indented(lines, theme, width, |w, out| {
                render_markdown(&block.text, w, out, theme)
            });
        }
        TranscriptKind::Reasoning if show_reasoning => {
            // Quiet on purpose: the model's working, not its answer. A marker
            // in the gutter and the text dimmed and slanted says "this is the
            // thinking" without the accent-filled band it used to sit under,
            // which was the loudest thing in the transcript.
            lines.push(Line::from(vec![
                Span::styled("✻ ", Style::default().fg(theme.accent2)),
                Span::styled(
                    "thinking",
                    Style::default()
                        .fg(theme.muted)
                        .add_modifier(Modifier::BOLD | Modifier::ITALIC),
                ),
            ]));
            indented(lines, theme, width, |w, out| {
                render_markdown(&block.text, w, out, theme);
                for line in out.iter_mut() {
                    for span in line.spans.iter_mut() {
                        span.style = span.style.fg(theme.muted).add_modifier(Modifier::ITALIC);
                    }
                }
            });
        }
        TranscriptKind::Reasoning => return,
        TranscriptKind::Brief { agent, id } => {
            // Closed by default. The brief is written for the sub-agent —
            // forty lines of instructions, exclusions and acceptance criteria
            // — and printing it in full buries the conversation it belongs
            // to. The row says who was sent what; the text is one click away.
            let expanded = tool_expanded.get(id).copied().unwrap_or(false);
            let body: Vec<&str> = block.text.lines().collect();
            let parts = crate::ui::tool::RowParts {
                verb: "delegate".into(),
                arg: agent.clone(),
                metric: format!("{} line brief", body.len()),
                status: None,
            };
            tool_headers.push((id.clone(), lines.len()));
            lines.push(tool_row(
                &parts,
                Some(if expanded { "▾" } else { "▸" }),
                ("◆", theme.accent),
                None,
                width,
                theme,
            ));
            if expanded {
                for line in body {
                    lines.push(Line::styled(
                        format!(
                            "  {}",
                            crate::text::trim(line, width.saturating_sub(2).max(1))
                        ),
                        Style::default().fg(theme.muted),
                    ));
                }
            }
        }
        TranscriptKind::Tool {
            id,
            name,
            args,
            result,
            running,
            error,
            started,
        } => {
            use crate::ui::tool::{classify, elapsed_note, render_diff, ToolBody, ToolRender};
            let icon = if *running {
                ("›", theme.yellow)
            } else if *error {
                ("✗", theme.red)
            } else {
                ("✓", theme.green)
            };
            // A tool that has been running a while should say so, rather
            // than looking identical at one second and at two minutes.
            let waiting = if *running {
                started.as_ref().and_then(elapsed_note)
            } else {
                None
            };
            let render = classify(name, args, result);
            let default_expand = crate::ui::tool::opens_by_default(name, show_tool_output);
            let expanded = tool_expanded.get(id).copied().unwrap_or(default_expand);
            match render {
                ToolRender::Summary(parts) => {
                    // Register a file marker for tools whose argument is a
                    // path, so a click opens it.
                    register_summary_file_link(
                        name,
                        args,
                        &format!("{} {}", parts.verb, parts.arg),
                        lines.len(),
                        file_links,
                    );
                    lines.push(tool_row(
                        &parts,
                        None,
                        icon,
                        waiting.as_deref(),
                        width,
                        theme,
                    ));
                }
                ToolRender::Detail {
                    header,
                    subtitle,
                    body,
                } => {
                    let chevron = if expanded { "▾" } else { "▸" };
                    let header_y_marker = lines.len();
                    lines.push(tool_row(
                        &header,
                        Some(chevron),
                        icon,
                        waiting.as_deref(),
                        width,
                        theme,
                    ));
                    tool_headers.push((id.clone(), header_y_marker));
                    // Header path (write, bash) is also a link target.
                    register_detail_file_link(name, args, header_y_marker, file_links);
                    if expanded || *error {
                        // Rendered two columns narrower and shifted onto the
                        // text column afterwards, so the body sits under the
                        // tool's name rather than under its status icon.
                        // Lines still go straight into `lines`, which keeps
                        // the file-link rows the renderers record correct.
                        let body_start = lines.len();
                        let width = width.saturating_sub(GUTTER).max(1);
                        if let Some(sub) = subtitle {
                            for wrapped in textwrap::wrap(&sub, width) {
                                lines.push(Line::from(vec![
                                    Span::styled("", Style::default().fg(theme.muted)),
                                    Span::styled(
                                        wrapped.into_owned(),
                                        Style::default().fg(theme.muted),
                                    ),
                                ]));
                            }
                        }
                        match body {
                            // `Formatted` carries text the classifier
                            // rewrote (pretty-printed JSON), so it draws
                            // exactly like `Plain` — the only difference
                            // is that it owns its buffer.
                            ToolBody::Plain(_) | ToolBody::Formatted(_) => {
                                let text: &str = match &body {
                                    ToolBody::Plain(t) => t,
                                    ToolBody::Formatted(t) => t.as_str(),
                                    _ => unreachable!("guarded by the arm pattern"),
                                };
                                // Parse ANSI SGR so `ls --color`,
                                // `grep --color`, and other TUI-aware
                                // programs render with their real
                                // colors instead of leaking `[m]`
                                // fragments. Non-bash Plain bodies
                                // (MCP proxy, generic tool) go through
                                // the same path — safe: they either
                                // have no escapes or the parser drops
                                // them.
                                let bg = theme.subtle;
                                let border_style = Style::default().fg(theme.accent).bg(bg);
                                let pieces = crate::ansi::parse(text);
                                let mut current: Vec<Span<'static>> = Vec::new();
                                let mut current_w: usize = 0;
                                // Layout: `│ content …` → 2 col gutter
                                // (`│ `) + body + right pad to full width.
                                let max_body_w = width.saturating_sub(3).max(1);
                                let flush =
                                    |lines: &mut Vec<Line<'static>>,
                                     current: &mut Vec<Span<'static>>,
                                     current_w: &mut usize| {
                                        let pad = max_body_w.saturating_sub(*current_w);
                                        let mut row: Vec<Span<'static>> = Vec::new();
                                        row.push(Span::styled("│ ", border_style));
                                        row.extend(std::mem::take(current));
                                        row.push(Span::styled(
                                            format!("{} ", " ".repeat(pad)),
                                            Style::default().bg(bg),
                                        ));
                                        lines.push(Line::from(row));
                                        *current_w = 0;
                                    };
                                for piece in pieces {
                                    let style = piece.style.bg(bg);
                                    for segment in piece.text.split_inclusive('\n') {
                                        let is_nl = segment.ends_with('\n');
                                        let visible: String = if is_nl {
                                            segment[..segment.len() - 1].to_string()
                                        } else {
                                            segment.to_string()
                                        };
                                        let vw = visible.chars().count();
                                        let room = max_body_w.saturating_sub(current_w);
                                        let (shown, overflow) = if vw > room {
                                            let mut s: String = visible
                                                .chars()
                                                .take(room.saturating_sub(1).max(1))
                                                .collect();
                                            s.push('…');
                                            (s, true)
                                        } else {
                                            (visible, false)
                                        };
                                        let shown_w = shown.chars().count();
                                        current.push(Span::styled(shown, style));
                                        current_w += shown_w;
                                        if is_nl || overflow {
                                            flush(lines, &mut current, &mut current_w);
                                        }
                                    }
                                }
                                if !current.is_empty() {
                                    flush(lines, &mut current, &mut current_w);
                                }
                            }
                            ToolBody::Diff {
                                old,
                                new,
                                start_line,
                            } => {
                                render_diff(&old, &new, start_line, width, lines, theme);
                            }
                            ToolBody::Preview { content, total } => {
                                crate::ui::tool::render_preview(
                                    &content, total, width, lines, theme,
                                );
                            }
                            ToolBody::Tree { items } => {
                                crate::ui::tool::render_tree(
                                    &items, width, lines, theme, file_links,
                                );
                            }
                            ToolBody::Todo { items } => {
                                crate::ui::tool::render_todo(&items, width, lines, theme);
                            }
                        }
                        gutter_from(lines, body_start, theme);
                    }
                }
            }
        }
        TranscriptKind::Notice => {
            indented(lines, theme, width, |w, out| {
                render_markdown(&block.text, w, out, theme)
            });
        }
        TranscriptKind::Retry => {
            // Red like an error, because it is one — the turn simply has
            // not given up yet. `retry N/M` replaces the per-attempt
            // messages that used to stack, one per backoff step.
            let label = if retry_max > 0 {
                format!("retry {retry_attempt}/{retry_max}")
            } else {
                "retry".to_string()
            };
            lines.push(Line::from(vec![
                Span::styled("↻ ", Style::default().fg(theme.red)),
                Span::styled(
                    label,
                    Style::default().fg(theme.red).add_modifier(Modifier::BOLD),
                ),
            ]));
            indented(lines, theme, width, |w, out| {
                for line in textwrap::wrap(&block.text, w) {
                    out.push(Line::styled(
                        line.into_owned(),
                        Style::default().fg(theme.red),
                    ));
                }
            });
        }
        TranscriptKind::Error => {
            // `×N` says the same failure is still arriving. Without it the
            // collapse would look like the error happened once.
            let label = if error_repeats > 1 {
                format!("error ×{error_repeats}")
            } else {
                "error".to_string()
            };
            lines.push(Line::from(vec![
                Span::styled("✗ ", Style::default().fg(theme.red)),
                Span::styled(
                    label,
                    Style::default().fg(theme.red).add_modifier(Modifier::BOLD),
                ),
            ]));
            indented(lines, theme, width, |w, out| {
                for line in textwrap::wrap(&block.text, w) {
                    out.push(Line::styled(
                        line.into_owned(),
                        Style::default().fg(theme.red),
                    ));
                }
            });
        }
        TranscriptKind::System => {
            indented(lines, theme, width, |_, out| {
                for line in block.text.lines() {
                    out.push(Line::styled(
                        line.to_string(),
                        Style::default().fg(theme.muted),
                    ));
                }
            });
        }
    }
    // Tool rows stack directly on top of each other: a blank line between
    // each one turned a run of ten calls into twenty rows of mostly empty
    // space. The separator before and after the whole run is added during
    // assembly, so the group still reads as distinct from the prose.
    if !matches!(block.kind, TranscriptKind::Tool { .. }) {
        lines.push(Line::default());
    }
}

/// The text of one handover marker: who took over, and why.
///
/// The reason is the whole point — a reply that changes voice with no stated
/// cause reads as the model misbehaving rather than as a different agent
/// answering. A switch recorded without one still shows the names.
fn switch_marker_text(switch: &enowx_core::session::AgentSwitch) -> String {
    let reason = switch.reason.trim();
    if reason.is_empty() {
        format!("{} → {}", switch.from, switch.to)
    } else {
        format!("{} → {} · {}", switch.from, switch.to, reason)
    }
}

/// A handover: `↳` in the gutter, then who took over and why, dimmed.
///
/// It used to be a rule drawn across the transcript with the label set into
/// it. The rule was the part nobody could see in a dark theme, and the label
/// then started on a different column from everything else.
///
/// No blank row above it: the block before always ends on one already.
fn switch_marker(lines: &mut Vec<Line<'static>>, text: &str, width: usize, theme: &Theme) {
    lines.push(Line::from(vec![
        Span::styled("↳ ", Style::default().fg(theme.accent2)),
        Span::styled(
            trim(text, width.saturating_sub(GUTTER).max(1)),
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::ITALIC),
        ),
    ]));
    lines.push(Line::default());
}

/// Bring `app.render_cache` in line with `app.blocks`, re-rendering only the
/// entries whose key changed.
fn refresh_render_cache(app: &mut App, width: usize) {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    // Handovers are drawn as part of the block they precede rather than
    // spliced in at assembly, so their rows are counted by the same pass that
    // sizes the transcript — a marker outside the cache would shift the
    // scroll offset without the scrollbar knowing.
    let mut markers_for: Vec<Vec<String>> = vec![Vec::new(); app.blocks.len()];
    let mut trailing: Vec<String> = Vec::new();
    for (at, switch) in &app.switch_markers {
        let text = switch_marker_text(switch);
        match markers_for.get_mut(*at) {
            Some(slot) => slot.push(text),
            // Recorded past the end: the handover happened but the new agent
            // has not answered yet, so it hangs off the bottom.
            None => trailing.push(text),
        }
    }

    let theme = app.theme;
    let show_reasoning = app.show_reasoning;
    let show_tool_output = app.show_tool_output;
    let error_repeats = app.error_repeats;
    let retry_attempt = app.retry_attempt;
    let retry_max = app.retry_max;

    app.render_cache.resize(app.blocks.len(), None);

    let last = app.blocks.len().saturating_sub(1);
    // Indexes three collections in step — blocks, their markers, and the cache
    // — so an iterator over any one of them would still need the index.
    #[allow(clippy::needless_range_loop)]
    for idx in 0..app.blocks.len() {
        let block = &app.blocks[idx];
        let before = std::mem::take(&mut markers_for[idx]);
        let after: &[String] = if idx == last { &trailing } else { &[] };
        let mut hasher = DefaultHasher::new();
        block.text.hash(&mut hasher);
        // The marker rows live inside this block's cached lines, so a change
        // to them has to invalidate it or the rule would freeze on screen.
        before.hash(&mut hasher);
        after.hash(&mut hasher);
        // Hash every field that alters what is drawn. A tool's `result` grows
        // while it streams, so it has to be part of the key.
        let (is_tool, expanded) = match &block.kind {
            TranscriptKind::Tool {
                id,
                name,
                args,
                result,
                running,
                error,
                started,
            } => {
                // A running tool shows a seconds counter, so its key has to
                // advance once a second or the cached header would freeze.
                // Only while running: a finished tool is stable again, and
                // hashing a live clock would re-render the whole transcript.
                if *running {
                    if let Some(at) = started {
                        at.elapsed().as_secs().hash(&mut hasher);
                    }
                }
                id.hash(&mut hasher);
                name.hash(&mut hasher);
                args.hash(&mut hasher);
                // Hash the result's SHAPE, not its bytes. A tool result can
                // be megabytes, and hashing all of it on every frame made the
                // cache cost scale with the output it was meant to avoid
                // re-rendering. Length plus the head and tail distinguishes a
                // growing stream from a finished one, which is all the key
                // needs to decide staleness.
                result.len().hash(&mut hasher);
                let bytes = result.as_bytes();
                bytes[..bytes.len().min(256)].hash(&mut hasher);
                if bytes.len() > 256 {
                    bytes[bytes.len() - 256..].hash(&mut hasher);
                }
                running.hash(&mut hasher);
                error.hash(&mut hasher);
                // Must match the renderer exactly, or a row would be cached
                // in one state and drawn in the other.
                let default_expand = crate::ui::tool::opens_by_default(name, show_tool_output);
                (
                    true,
                    app.tool_expanded.get(id).copied().unwrap_or(default_expand),
                )
            }
            TranscriptKind::Brief { agent, id } => {
                // Clicking the row toggles it, so the key has to carry that
                // state — otherwise the cached closed row is served again and
                // the click looks ignored.
                std::mem::discriminant(&block.kind).hash(&mut hasher);
                agent.hash(&mut hasher);
                id.hash(&mut hasher);
                // Not `is_tool`: that flag closes the gap between
                // consecutive tool rows, and a brief is punctuation between
                // the conversation and a sub-agent's work — it needs its
                // space.
                (false, app.tool_expanded.get(id).copied().unwrap_or(false))
            }
            other => {
                std::mem::discriminant(other).hash(&mut hasher);
                // The repeat counter is drawn into the error block, so a
                // change to it has to invalidate that block's cache entry.
                if matches!(other, TranscriptKind::Error) {
                    error_repeats.hash(&mut hasher);
                }
                // The attempt counter is drawn into the retry block, so it
                // has to invalidate that block's cache entry as it advances.
                if matches!(other, TranscriptKind::Retry) {
                    retry_attempt.hash(&mut hasher);
                    retry_max.hash(&mut hasher);
                }
                (false, false)
            }
        };
        let hidden = matches!(block.kind, TranscriptKind::Reasoning) && !show_reasoning;
        // A hidden block that carries a handover still has to draw, or
        // collapsing reasoning would silently swallow the marker with it.
        let skipped = hidden && before.is_empty() && after.is_empty();
        let key = crate::ui::BlockKey {
            content: hasher.finish(),
            width,
            theme: theme.name,
            expanded,
            show_reasoning,
        };
        if app
            .render_cache
            .get(idx)
            .and_then(|c| c.as_ref())
            .is_some_and(|c| c.key == key)
        {
            continue;
        }
        let mut lines: Vec<Line<'static>> = Vec::new();
        let mut tool_headers: Vec<(String, usize)> = Vec::new();
        let mut file_links: Vec<(usize, String)> = Vec::new();
        for text in &before {
            switch_marker(&mut lines, text, width, &theme);
        }
        // Markers occupy rows above the block, so the click offsets the
        // renderer records are relative to the wrong line until they are
        // shifted past them.
        let offset = lines.len();
        if !hidden {
            render_block(
                block,
                width,
                &theme,
                show_reasoning,
                show_tool_output,
                error_repeats,
                retry_attempt,
                retry_max,
                &app.tool_expanded,
                &mut lines,
                &mut tool_headers,
                &mut file_links,
            );
        }
        for (_, at) in tool_headers.iter_mut() {
            *at += offset;
        }
        for (at, _) in file_links.iter_mut() {
            *at += offset;
        }
        for text in after {
            switch_marker(&mut lines, text, width, &theme);
        }
        app.render_cache[idx] = Some(crate::ui::BlockRender {
            key,
            lines,
            tool_headers,
            file_links,
            is_tool,
            skipped,
        });
    }
}

/// Width of the verb column.
///
/// Ten covers every built-in tool (`skill_read` is the longest) so the
/// argument column starts at the same place on every row — that alignment is
/// the point of the layout. MCP tool names can be far longer and are clipped
/// rather than allowed to push the column out.
const VERB_COLUMN: usize = 10;

/// One tool-call row: `✓ verb       argument            metric ▸`.
///
/// The status icon sits in the transcript's marker gutter and the verb starts
/// on the text column, like every other block's text. The chevron has a
/// column of its own at the far right, blank when there is nothing to open, so
/// rows with and without a body share both edges: the verbs line up on the
/// left and the metrics are flush right in one column.
fn tool_row(
    parts: &crate::ui::tool::RowParts,
    chevron: Option<&str>,
    icon: (&str, ratatui::style::Color),
    waiting: Option<&str>,
    width: usize,
    theme: &Theme,
) -> Line<'static> {
    let faint = Style::default().fg(theme.faint);
    let dim = Style::default().fg(theme.muted);

    // Right-hand side: the metric, plus any status or elapsed note.
    let mut right = parts.metric.clone();
    if let Some(status) = &parts.status {
        right = if right.is_empty() {
            status.clone()
        } else {
            format!("{right} · {status}")
        };
    }
    if let Some(note) = waiting {
        right = if right.is_empty() {
            note.to_string()
        } else {
            format!("{right} · {note}")
        };
    }
    // A failing row's count is part of the failure, so it takes the icon's
    // colour rather than the neutral one.
    let right_style = if parts.status.is_some() {
        Style::default().fg(icon.1)
    } else {
        dim
    };

    let verb = trim(&parts.verb, VERB_COLUMN);
    let verb_pad = VERB_COLUMN.saturating_sub(unicode_width_of(&verb));

    let mut spans = vec![
        Span::styled(format!("{} ", icon.0), Style::default().fg(icon.1)),
        Span::styled(verb, Style::default().fg(theme.accent)),
        Span::raw(" ".repeat(verb_pad + 2)),
    ];
    // The space and the chevron column at the far right.
    const TAIL: usize = 2;

    // Whatever is left after the fixed columns and the right-hand text.
    let used: usize = spans.iter().map(|s| unicode_width_of(&s.content)).sum();
    let budget = width
        .saturating_sub(used + unicode_width_of(&right) + 2 + TAIL)
        .max(8);
    let arg = trim(&parts.arg, budget);
    let arg_w = unicode_width_of(&arg);
    spans.push(Span::styled(arg, Style::default().fg(theme.text)));

    let gap = width
        .saturating_sub(used + arg_w + unicode_width_of(&right) + TAIL)
        .max(1);
    spans.push(Span::raw(" ".repeat(gap)));
    spans.push(Span::styled(right, right_style));
    spans.push(Span::styled(format!(" {}", chevron.unwrap_or(" ")), faint));
    Line::from(spans)
}

/// Whether a rendered line is empty, used to avoid stacking blank lines.
fn is_blank_line(line: Option<&Line<'static>>) -> bool {
    match line {
        None => true,
        Some(line) => line.spans.iter().all(|s| s.content.trim().is_empty()),
    }
}
