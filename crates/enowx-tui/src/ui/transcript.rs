use super::*;

pub(super) fn draw_welcome(frame: &mut Frame, app: &App, area: Rect) {
    let (title, hint) = if app.config.is_ready() {
        (
            "What are we working on?",
            "/sessions to resume · /help for commands",
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
    frame.render_widget(Paragraph::new(lines), area);
}

pub(super) fn draw_transcript(frame: &mut Frame, app: &mut App, area: Rect) {
    let theme = app.theme;
    let width = area.width.saturating_sub(1).max(1) as usize;
    app.tool_header_markers.clear();
    app.tool_header_rects.clear();
    app.file_link_markers.clear();
    app.file_link_rects.clear();
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

/// Card style: a one-row header with a solid background, then a body block
/// with a subtle background that extends to the right edge so the whole card
/// reads as one continuous surface. All colors come from the active theme so
/// switching themes restyles every card at once.
fn card(
    lines: &mut Vec<Line<'static>>,
    title: &str,
    text: &str,
    width: usize,
    accent: ratatui::style::Color,
    theme: &Theme,
) {
    let inner_w = width.saturating_sub(2).max(1);
    let heading = trim(title, inner_w);
    let heading_len = heading.width();
    let header_pad = inner_w.saturating_sub(heading_len);
    // Header: bold title on the accent color, padded to full width.
    lines.push(Line::from(vec![Span::styled(
        format!("  {heading}{}", " ".repeat(header_pad)),
        Style::default()
            .fg(theme.canvas)
            .bg(accent)
            .add_modifier(Modifier::BOLD),
    )]));
    // Body: markdown-rendered lines painted onto the subtle background with
    // a two-column left padding, right-padded to the same width.
    let body_w = width.saturating_sub(4).max(1);
    let mut inner: Vec<Line<'static>> = Vec::new();
    render_markdown(text, body_w, &mut inner, theme);
    if inner.is_empty() {
        inner.push(Line::default());
    }
    for row in inner {
        let row_w = row.width();
        let pad = body_w.saturating_sub(row_w);
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.push(Span::styled("  ", Style::default().bg(theme.subtle)));
        for span in row.spans {
            // Repaint each span's background so gaps between spans still
            // show the card tint.
            let style = span.style.bg(theme.subtle);
            spans.push(Span::styled(span.content.into_owned(), style));
        }
        spans.push(Span::styled(
            format!("{}  ", " ".repeat(pad)),
            Style::default().bg(theme.subtle),
        ));
        lines.push(Line::from(spans));
    }
}

pub(crate) fn render_markdown(
    text: &str,
    width: usize,
    lines: &mut Vec<Line<'static>>,
    theme: &Theme,
) {
    let mut in_code = false;
    // Set when the opening fence names a language we can highlight; cleared at
    // the closing fence. `code_state` carries a block comment across lines.
    let mut code_syntax: Option<crate::syntax::Syntax> = None;
    let mut code_state = crate::syntax::State::default();
    let mut ordered_counter: Option<usize> = None;
    let raw_lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < raw_lines.len() {
        let raw = raw_lines[i];
        // Fenced code block.
        if let Some(rest) = raw.trim_start().strip_prefix("```") {
            in_code = !in_code;
            let bar = if in_code {
                let lang = rest.trim();
                code_syntax = crate::syntax::lookup(lang);
                code_state = crate::syntax::State::default();
                if lang.is_empty() {
                    "┌ code".to_string()
                } else {
                    format!("┌ {lang}")
                }
            } else {
                code_syntax = None;
                "└".into()
            };
            let pad = width.saturating_sub(unicode_width_of(&bar));
            lines.push(Line::from(vec![
                Span::styled(bar, Style::default().fg(theme.muted).bg(theme.subtle)),
                Span::styled(" ".repeat(pad), Style::default().bg(theme.subtle)),
            ]));
            i += 1;
            continue;
        }
        if in_code {
            emit_code_line(lines, raw, width, code_syntax.as_ref(), &mut code_state, theme);
            i += 1;
            continue;
        }

        // Table: header row + separator + data rows. Detected when the next
        // line looks like `| --- | --- |`. We consume all consecutive `|` rows.
        if is_table_row(raw) && i + 1 < raw_lines.len() && is_table_separator(raw_lines[i + 1]) {
            let mut rows: Vec<Vec<String>> = Vec::new();
            rows.push(split_table_row(raw));
            i += 2; // skip header + separator
            while i < raw_lines.len() && is_table_row(raw_lines[i]) {
                rows.push(split_table_row(raw_lines[i]));
                i += 1;
            }
            render_table(&rows, width, lines, theme);
            continue;
        }

        if raw.trim().is_empty() {
            ordered_counter = None;
            lines.push(Line::default());
            i += 1;
            continue;
        }

        if let Some(body) = raw.strip_prefix("##### ") {
            emit_wrapped(
                lines,
                &markdown_spans(body, theme),
                "",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
                width,
                theme,
            );
            i += 1;
            continue;
        }
        if let Some(body) = raw.strip_prefix("#### ") {
            emit_wrapped(
                lines,
                &markdown_spans(body, theme),
                "",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
                width,
                theme,
            );
            i += 1;
            continue;
        }
        if let Some(body) = raw.strip_prefix("### ") {
            emit_wrapped(
                lines,
                &markdown_spans(body, theme),
                "",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
                width,
                theme,
            );
            i += 1;
            continue;
        }
        if let Some(body) = raw.strip_prefix("## ") {
            emit_wrapped(
                lines,
                &markdown_spans(body, theme),
                "",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
                width,
                theme,
            );
            i += 1;
            continue;
        }
        if let Some(body) = raw.strip_prefix("# ") {
            emit_wrapped(
                lines,
                &markdown_spans(body, theme),
                "",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
                width,
                theme,
            );
            i += 1;
            continue;
        }

        if let Some(body) = raw.strip_prefix("> ") {
            emit_wrapped(
                lines,
                &markdown_spans(body, theme),
                "│ ",
                Style::default().fg(theme.faint),
                width,
                theme,
            );
            i += 1;
            continue;
        }

        if matches!(raw.trim(), "---" | "***" | "___") {
            lines.push(Line::styled(
                "─".repeat(width.max(1)),
                Style::default().fg(theme.muted),
            ));
            i += 1;
            continue;
        }

        let trimmed = raw.trim_start();
        let indent = raw.len() - trimmed.len();
        if let Some(item) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            let prefix = format!("{}• ", " ".repeat(indent));
            emit_wrapped(
                lines,
                &markdown_spans(item, theme),
                &prefix,
                Style::default().fg(theme.text),
                width,
                theme,
            );
            i += 1;
            continue;
        }

        if let Some((num, item)) = split_ordered(trimmed) {
            let display = ordered_counter.map_or(num, |n| n + 1);
            ordered_counter = Some(display);
            let prefix = format!("{}{}. ", " ".repeat(indent), display);
            emit_wrapped(
                lines,
                &markdown_spans(item, theme),
                &prefix,
                Style::default().fg(theme.text),
                width,
                theme,
            );
            i += 1;
            continue;
        } else {
            ordered_counter = None;
        }

        // Paragraph.
        emit_wrapped(
            lines,
            &markdown_spans(raw, theme),
            "",
            Style::default().fg(theme.text),
            width,
            theme,
        );
        i += 1;
    }
}

fn is_table_row(line: &str) -> bool {
    let t = line.trim();
    t.starts_with('|') && t.ends_with('|') && t.matches('|').count() >= 2
}

fn is_table_separator(line: &str) -> bool {
    let t = line.trim();
    if !is_table_row(t) {
        return false;
    }
    // Every non-empty cell is dashes with optional colon (alignment marker).
    t.trim_matches('|').split('|').all(|cell| {
        let c = cell.trim();
        !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':')
    })
}

fn split_table_row(line: &str) -> Vec<String> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(|c| c.trim().to_string())
        .collect()
}

fn render_table(rows: &[Vec<String>], width: usize, lines: &mut Vec<Line<'static>>, theme: &Theme) {
    if rows.is_empty() {
        return;
    }
    let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if cols == 0 {
        return;
    }
    // Column widths: measure content, then rebalance so total fits `width - (cols+1)` (borders).
    let mut widths = vec![0usize; cols];
    for row in rows {
        for (c, cell) in row.iter().enumerate() {
            widths[c] = widths[c].max(unicode_width_of(cell));
        }
    }
    let budget = width.saturating_sub(cols + 1).max(cols); // 1 pipe per col + trailing
    let total: usize = widths.iter().sum();
    if total > budget {
        let scale = budget as f32 / total as f32;
        for w in widths.iter_mut() {
            *w = ((*w as f32) * scale).floor().max(3.0) as usize;
        }
    }
    let border_style = Style::default().fg(theme.muted).bg(theme.subtle);
    let cell_style = Style::default().fg(theme.text).bg(theme.subtle);
    let header_style = Style::default()
        .fg(theme.accent)
        .bg(theme.subtle)
        .add_modifier(Modifier::BOLD);

    // Top border
    let top: String = std::iter::once('┌')
        .chain(widths.iter().enumerate().flat_map(|(i, w)| {
            let seg: Vec<char> = std::iter::repeat_n('─', *w + 2).collect();
            let sep = if i + 1 == cols { '┐' } else { '┬' };
            seg.into_iter().chain(std::iter::once(sep))
        }))
        .collect();
    lines.push(Line::styled(top, border_style));

    for (r, row) in rows.iter().enumerate() {
        let mut spans: Vec<Span<'static>> = vec![Span::styled("│", border_style)];
        for (c, w) in widths.iter().enumerate() {
            let cell = row.get(c).map(String::as_str).unwrap_or("");
            // Parse the cell's inline markdown so **bold** and `code` inside
            // a table cell render like they do in paragraphs. Fall back to
            // plain text when the cell is empty. Layout is char-count based
            // for width; styling is span-based.
            let parsed = markdown_spans(cell, theme);
            let plain_text: String = parsed.iter().map(|(s, _)| s.as_str()).collect();
            let plain_w = unicode_width_of(&plain_text);
            // Clip to the column width, replacing overflow with `…`. Since we
            // clip on chars we may cut mid-style; acceptable for tables.
            let (rendered, pad) = if plain_w > *w {
                let mut cut: String = plain_text.chars().take(w.saturating_sub(1)).collect();
                cut.push('…');
                (
                    vec![(cut, if r == 0 { header_style } else { cell_style })],
                    0,
                )
            } else {
                // Adopt the header style when we're on the first row so bold
                // titles read like a header, otherwise keep the span colors.
                let base_style = if r == 0 { header_style } else { cell_style };
                let styled: Vec<(String, Style)> = parsed
                    .into_iter()
                    .map(|(s, style)| {
                        // For rows 2+, keep inline markdown coloring on the
                        // subtle background; for row 0, force the header
                        // color so accent stays consistent.
                        let merged = if r == 0 {
                            base_style
                        } else {
                            style.bg(theme.subtle)
                        };
                        (s, merged)
                    })
                    .collect();
                (styled, w.saturating_sub(plain_w))
            };
            spans.push(Span::styled(" ", cell_style));
            for (text, style) in rendered {
                spans.push(Span::styled(text, style));
            }
            spans.push(Span::styled(format!("{} ", " ".repeat(pad)), cell_style));
            spans.push(Span::styled("│", border_style));
        }
        lines.push(Line::from(spans));

        // Separator under header
        if r == 0 {
            let sep: String = std::iter::once('├')
                .chain(widths.iter().enumerate().flat_map(|(i, w)| {
                    let seg: Vec<char> = std::iter::repeat_n('─', *w + 2).collect();
                    let s = if i + 1 == cols { '┤' } else { '┼' };
                    seg.into_iter().chain(std::iter::once(s))
                }))
                .collect();
            lines.push(Line::styled(sep, border_style));
        }
    }

    let bot: String = std::iter::once('└')
        .chain(widths.iter().enumerate().flat_map(|(i, w)| {
            let seg: Vec<char> = std::iter::repeat_n('─', *w + 2).collect();
            let sep = if i + 1 == cols { '┘' } else { '┴' };
            seg.into_iter().chain(std::iter::once(sep))
        }))
        .collect();
    lines.push(Line::styled(bot, border_style));
}

/// Parse inline markdown into styled spans: **bold**, *italic*, `code`,
/// [text](url). Everything else is plain text under the caller's base style.
fn markdown_spans(text: &str, theme: &Theme) -> Vec<(String, Style)> {
    let mut out: Vec<(String, Style)> = Vec::new();
    let plain = Style::default().fg(theme.text);
    let mut buf = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let flush = |buf: &mut String, out: &mut Vec<(String, Style)>| {
        if !buf.is_empty() {
            out.push((std::mem::take(buf), plain));
        }
    };
    while i < chars.len() {
        let c = chars[i];
        // Inline code: `…`
        if c == '`' {
            if let Some(end) = chars[i + 1..].iter().position(|&c| c == '`') {
                flush(&mut buf, &mut out);
                let body: String = chars[i + 1..i + 1 + end].iter().collect();
                out.push((body, Style::default().fg(theme.accent2).bg(theme.subtle)));
                i += end + 2;
                continue;
            }
        }
        // Bold: **…**
        if c == '*' && chars.get(i + 1) == Some(&'*') {
            if let Some(end) = find_pair(&chars, i + 2, "**") {
                flush(&mut buf, &mut out);
                let body: String = chars[i + 2..end].iter().collect();
                out.push((
                    body,
                    Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
                ));
                i = end + 2;
                continue;
            }
        }
        // Italic: *…* (single asterisk, avoid ** that we already handled)
        if c == '*' {
            if let Some(end) = find_pair(&chars, i + 1, "*") {
                flush(&mut buf, &mut out);
                let body: String = chars[i + 1..end].iter().collect();
                out.push((
                    body,
                    Style::default()
                        .fg(theme.text)
                        .add_modifier(Modifier::ITALIC),
                ));
                i = end + 1;
                continue;
            }
        }
        // Link: [text](url) → underline the text, keep url out of sight
        if c == '[' {
            if let Some(close) = chars[i + 1..].iter().position(|&c| c == ']') {
                let after = i + 1 + close + 1;
                if chars.get(after) == Some(&'(') {
                    if let Some(url_end) = chars[after + 1..].iter().position(|&c| c == ')') {
                        flush(&mut buf, &mut out);
                        let label: String = chars[i + 1..i + 1 + close].iter().collect();
                        out.push((
                            label,
                            Style::default()
                                .fg(theme.accent)
                                .add_modifier(Modifier::UNDERLINED),
                        ));
                        i = after + 1 + url_end + 1;
                        continue;
                    }
                }
            }
        }
        buf.push(c);
        i += 1;
    }
    flush(&mut buf, &mut out);
    out
}

fn find_pair(chars: &[char], start: usize, delim: &str) -> Option<usize> {
    let bytes: Vec<char> = delim.chars().collect();
    let mut i = start;
    while i + bytes.len() <= chars.len() {
        if chars[i..i + bytes.len()] == bytes[..] {
            // Don't match empty spans.
            if i > start {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn split_ordered(text: &str) -> Option<(usize, &str)> {
    let digits: String = text.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let rest = &text[digits.len()..];
    let body = rest.strip_prefix(". ")?;
    digits.parse().ok().map(|n| (n, body))
}

/// Emit a styled-span line, wrapping at ASCII whitespace so inline styles
/// (code, bold, links) survive the wrap without losing characters like `<`,
/// `>`, `=`, `"` that word-based wrappers treat as word boundaries.
fn emit_wrapped(
    lines: &mut Vec<Line<'static>>,
    spans: &[(String, Style)],
    prefix: &str,
    base: Style,
    width: usize,
    _theme: &Theme,
) {
    let usable = width.saturating_sub(unicode_width_of(prefix)).max(1);

    if spans.iter().all(|(s, _)| s.is_empty()) {
        lines.push(Line::styled(prefix.to_string(), base));
        return;
    }

    // Flatten to (char, style) pairs so wrap decisions do not have to know
    // about span boundaries. Rebuild spans on emit by grouping consecutive
    // chars that share a style.
    let mut chars: Vec<(char, Style)> = Vec::new();
    for (text, style) in spans {
        for ch in text.chars() {
            chars.push((ch, *style));
        }
    }

    let mut row_start = 0usize;
    let mut first_row = true;
    let mut i = 0;
    while i < chars.len() {
        // Advance until the row is full, measuring DISPLAY WIDTH rather than
        // char count: CJK and emoji occupy two columns each, so counting
        // characters let a line of them overrun the panel by up to 2x.
        let mut end = row_start;
        let mut used = 0usize;
        while end < chars.len() {
            let w = unicode_width_of_char(chars[end].0).max(1);
            if used + w > usable {
                break;
            }
            used += w;
            end += 1;
        }
        if end == row_start {
            // One character wider than the whole row: emit it alone rather
            // than looping forever on a zero-width advance.
            end = row_start + 1;
        }
        // If we would cut in the middle of a word, back up to the last space
        // in the current window so wrap happens at whitespace. Scripts that do
        // not use spaces (Chinese, Japanese) have no break to find, so the
        // width-based cut above stands — without this guard such a paragraph
        // collapsed into one unwrappable row.
        if end < chars.len() && chars[end].0 != ' ' {
            let mut back = end;
            while back > row_start && chars[back - 1].0 != ' ' {
                back -= 1;
            }
            if back > row_start {
                end = back;
            }
        }
        // Emit chars[row_start..end] as grouped spans, trim trailing space.
        let mut segment_end = end;
        while segment_end > row_start && chars[segment_end - 1].0 == ' ' {
            segment_end -= 1;
        }
        push_row(
            lines,
            &chars,
            row_start,
            segment_end,
            prefix,
            base,
            first_row,
        );
        first_row = false;
        // Skip the whitespace we broke on so the next row does not start
        // with a leading space.
        i = end;
        while i < chars.len() && chars[i].0 == ' ' {
            i += 1;
        }
        row_start = i;
    }
}

fn push_row(
    lines: &mut Vec<Line<'static>>,
    chars: &[(char, Style)],
    start: usize,
    end: usize,
    prefix: &str,
    base: Style,
    first_row: bool,
) {
    let indent = if first_row {
        prefix.to_string()
    } else {
        // Pad by display width so a continuation row lines up under the first
        // even when the prefix contains wide glyphs.
        " ".repeat(unicode_width_of(prefix))
    };
    let mut spans: Vec<Span<'static>> = Vec::new();
    if !indent.is_empty() {
        spans.push(Span::styled(indent, base));
    }
    if start >= end {
        lines.push(Line::from(spans));
        return;
    }
    let mut current = String::new();
    let mut current_style = chars[start].1;
    for (ch, style) in &chars[start..end] {
        if *style != current_style && !current.is_empty() {
            spans.push(Span::styled(std::mem::take(&mut current), current_style));
        }
        current_style = *style;
        current.push(*ch);
    }
    if !current.is_empty() {
        spans.push(Span::styled(current, current_style));
    }
    lines.push(Line::from(spans));
}

/// User messages sit in a bordered, subtle-tinted card so the eye can find
/// them while scrolling. Every body line is hard-clipped to the card's inner
/// width so a nested list or fenced code inside a paste can never draw past
/// the right border. Tall messages get truncated with a "▸ N more lines"
/// hint the caller can wire to expansion later.
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
    // Force every rendered line to fit inside `body_w`. `render_markdown`
    // already wraps text, but styled spans, tables, and headings can still
    // exceed the target width (styled bg extends past the char count). Re-wrap
    // by flattening to text and re-styling to a single foreground colour.
    let mut clipped: Vec<(String, Style)> = Vec::new();
    for row in inner {
        let joined: String = row
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>();
        let style = row
            .spans
            .first()
            .map(|s| s.style)
            .unwrap_or_else(|| Style::default().fg(theme.text));
        if joined.is_empty() {
            clipped.push((String::new(), style));
            continue;
        }
        for wrapped in textwrap::wrap(&joined, body_w.max(1)) {
            clipped.push((wrapped.into_owned(), style));
        }
    }

    let overflow = clipped.len().saturating_sub(USER_CARD_MAX_LINES);
    let visible: Vec<(String, Style)> = if overflow > 0 {
        clipped.into_iter().take(USER_CARD_MAX_LINES).collect()
    } else {
        clipped
    };

    let border_style = Style::default().fg(theme.accent).bg(theme.subtle);
    let top = format!("╭{}╮", "─".repeat(width.saturating_sub(2)));
    let bottom = format!("╰{}╯", "─".repeat(width.saturating_sub(2)));
    lines.push(Line::styled(top, border_style));
    for (text, style) in visible {
        let text_w = unicode_width_of(&text);
        let pad = body_w.saturating_sub(text_w);
        let style = style.bg(theme.subtle);
        lines.push(Line::from(vec![
            Span::styled("│ ", border_style),
            Span::styled(text, style),
            Span::styled(format!("{} │", " ".repeat(pad)), border_style),
        ]));
    }
    if overflow > 0 {
        let msg = format!(
            "▸ {overflow} more line{s} …",
            s = if overflow == 1 { "" } else { "s" }
        );
        let msg_w = unicode_width_of(&msg);
        let pad = body_w.saturating_sub(msg_w);
        lines.push(Line::from(vec![
            Span::styled("│ ", border_style),
            Span::styled(msg, Style::default().fg(theme.muted).bg(theme.subtle)),
            Span::styled(format!("{} │", " ".repeat(pad)), border_style),
        ]));
    }
    lines.push(Line::styled(bottom, border_style));
}

fn unicode_width_of(text: &str) -> usize {
    use unicode_width::UnicodeWidthStr;
    text.width()
}

fn unicode_width_of_char(c: char) -> usize {
    use unicode_width::UnicodeWidthChar;
    c.width().unwrap_or(0)
}

/// Draw one source line inside a fenced block: gutter, highlighted body,
/// background padding out to `width`.
///
/// Long lines SOFT-WRAP rather than being cut with an ellipsis. Truncating
/// loses the tail of exactly the lines that need reading most (a long
/// signature, a deep path), and the terminal cannot scroll a block
/// horizontally, so the characters were simply unrecoverable. Continuation
/// rows use a dimmer gutter so a wrap is never mistaken for a real newline.
fn emit_code_line(
    lines: &mut Vec<Line<'static>>,
    raw: &str,
    width: usize,
    syntax: Option<&crate::syntax::Syntax>,
    state: &mut crate::syntax::State,
    theme: &Theme,
) {
    const GUTTER: &str = "│ ";
    // A wrap continuation must not look like a new source line. `↳` reads as
    // "this is the same line, continued" at a glance, where a dimmer vertical
    // bar was too easily mistaken for the real gutter.
    const CONT: &str = "│↳";
    let gutter_w = unicode_width_of(GUTTER);
    let usable = width.saturating_sub(gutter_w).max(1);

    // Tokenize once per source line; an unknown language yields a single plain
    // run so the block still renders, just without colour.
    let runs: Vec<(String, crate::syntax::Tok)> = match syntax {
        Some(s) => crate::syntax::highlight(raw, s, state),
        None => vec![(raw.to_string(), crate::syntax::Tok::Plain)],
    };

    // Flatten to (char, token) so a wrap can fall mid-run without having to
    // split the run list by hand.
    let mut cells: Vec<(char, crate::syntax::Tok)> = Vec::new();
    for (text, tok) in &runs {
        for ch in text.chars() {
            cells.push((ch, *tok));
        }
    }
    if cells.is_empty() {
        lines.push(Line::from(vec![
            Span::styled(GUTTER, Style::default().fg(theme.muted).bg(theme.subtle)),
            Span::styled(" ".repeat(usable), Style::default().bg(theme.subtle)),
        ]));
        return;
    }

    let mut idx = 0;
    let mut first = true;
    while idx < cells.len() {
        // Take as many cells as fit, measuring by display width so CJK and
        // emoji (width 2) do not overflow the block.
        let mut used = 0usize;
        let mut end = idx;
        while end < cells.len() {
            let w = unicode_width_of_char(cells[end].0).max(1);
            if used + w > usable {
                break;
            }
            used += w;
            end += 1;
        }
        // A single cell wider than the whole block would loop forever.
        if end == idx {
            end = idx + 1;
            used = usable;
        }

        let mut spans: Vec<Span<'static>> = Vec::with_capacity(4);
        spans.push(Span::styled(
            if first { GUTTER } else { CONT },
            Style::default()
                .fg(if first { theme.muted } else { theme.faint })
                .bg(theme.subtle),
        ));
        // Regroup consecutive same-token cells into spans.
        let mut run_start = idx;
        while run_start < end {
            let tok = cells[run_start].1;
            let mut run_end = run_start;
            while run_end < end && cells[run_end].1 == tok {
                run_end += 1;
            }
            let body: String = cells[run_start..run_end].iter().map(|(c, _)| *c).collect();
            spans.push(Span::styled(
                body,
                tok.style(theme).bg(theme.subtle),
            ));
            run_start = run_end;
        }
        spans.push(Span::styled(
            " ".repeat(usable.saturating_sub(used)),
            Style::default().bg(theme.subtle),
        ));
        lines.push(Line::from(spans));
        idx = end;
        first = false;
    }
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
    tool_expanded: &std::collections::HashMap<String, bool>,
    lines: &mut Vec<Line<'static>>,
    tool_headers: &mut Vec<(String, usize)>,
    file_links: &mut Vec<(usize, String)>,
) {

        match &block.kind {
            TranscriptKind::User => {
                user_card(lines, &block.text, width, &theme);
            }
            TranscriptKind::Assistant => {
                render_markdown(&block.text, width, lines, &theme);
            }
            TranscriptKind::Reasoning if show_reasoning => {
                card(
                    lines,
                    "THOUGHT TRACE",
                    &block.text,
                    width,
                    theme.accent2,
                    &theme,
                );
            }
            TranscriptKind::Reasoning => return,
            TranscriptKind::Tool {
                id,
                name,
                args,
                result,
                running,
                error,
            } => {
                use crate::ui::tool::{classify, render_diff, ToolBody, ToolRender};
                let icon = if *running {
                    ("›", theme.yellow)
                } else if *error {
                    ("✗", theme.red)
                } else {
                    ("✓", theme.green)
                };
                let render = classify(name, args, result);
                // `bash` output can be huge (a stray `ls` on node_modules
                // spills hundreds of lines). Default-collapse it regardless
                // of the master toggle; user clicks the header to expand.
                let default_expand = if name == "bash" {
                    false
                } else {
                    show_tool_output
                };
                let expanded = tool_expanded.get(id).copied().unwrap_or(default_expand);
                match render {
                    ToolRender::Summary(text) => {
                        // Register a file marker for tools whose summary
                        // starts with `<verb> <path>` so a click opens it.
                        register_summary_file_link(
                            name,
                            args,
                            &text,
                            lines.len(),
                            file_links,
                        );
                        lines.push(Line::from(vec![
                            Span::styled(format!("{} ", icon.0), Style::default().fg(icon.1)),
                            Span::styled(text, Style::default().fg(theme.muted)),
                        ]));
                    }
                    ToolRender::Detail {
                        header,
                        subtitle,
                        body,
                    } => {
                        let chevron = if expanded { "▾" } else { "▸" };
                        let header_y_marker = lines.len();
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("{chevron} {} ", icon.0),
                                Style::default().fg(icon.1),
                            ),
                            Span::styled(header, Style::default().fg(theme.text)),
                        ]));
                        tool_headers.push((id.clone(), header_y_marker));
                        // Header path (write, bash) is also a link target.
                        register_detail_file_link(
                            name,
                            args,
                            header_y_marker,
                            file_links,
                        );
                        if expanded || *error {
                            if let Some(sub) = subtitle {
                                for wrapped in textwrap::wrap(&sub, width.saturating_sub(4).max(1))
                                {
                                    lines.push(Line::from(vec![
                                        Span::styled("    ", Style::default().fg(theme.muted)),
                                        Span::styled(
                                            wrapped.into_owned(),
                                            Style::default().fg(theme.muted),
                                        ),
                                    ]));
                                }
                            }
                            match body {
                                ToolBody::Plain(text) => {
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
                                    let flush = |lines: &mut Vec<Line<'static>>,
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
                                    path,
                                    old,
                                    new,
                                    start_line,
                                } => {
                                    render_diff(
                                        &path,
                                        &old,
                                        &new,
                                        start_line,
                                        width,
                                        lines,
                                        &theme,
                                        file_links,
                                    );
                                }
                                ToolBody::Preview { content, total } => {
                                    crate::ui::tool::render_preview(
                                        &content, total, width, lines, &theme,
                                    );
                                }
                                ToolBody::Tree { items } => {
                                    crate::ui::tool::render_tree(
                                        &items,
                                        width,
                                        lines,
                                        &theme,
                                        file_links,
                                    );
                                }
                                ToolBody::Todo { items } => {
                                    crate::ui::tool::render_todo(&items, width, lines, &theme);
                                }
                            }
                        }
                    }
                }
            }
            TranscriptKind::Notice => {
                let mut inner: Vec<Line<'static>> = Vec::new();
                render_markdown(&block.text, width, &mut inner, &theme);
                for line in inner {
                    lines.push(line);
                }
            }
            TranscriptKind::Error => {
                lines.push(Line::styled(
                    "error",
                    Style::default().fg(theme.red).add_modifier(Modifier::BOLD),
                ));
                for line in textwrap::wrap(&block.text, width.saturating_sub(3)) {
                    lines.push(Line::styled(
                        format!("│ {line}"),
                        Style::default().fg(theme.red),
                    ));
                }
            }
            TranscriptKind::System => {
                for line in block.text.lines() {
                    lines.push(Line::styled(
                        line.to_string(),
                        Style::default().fg(theme.muted),
                    ));
                }
            }
        }
        lines.push(Line::default());
    }

/// Bring `app.render_cache` in line with `app.blocks`, re-rendering only the
/// entries whose key changed.
fn refresh_render_cache(app: &mut App, width: usize) {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let theme = app.theme;
    let show_reasoning = app.show_reasoning;
    let show_tool_output = app.show_tool_output;

    app.render_cache.resize(app.blocks.len(), None);

    for idx in 0..app.blocks.len() {
        let block = &app.blocks[idx];
        let mut hasher = DefaultHasher::new();
        block.text.hash(&mut hasher);
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
            } => {
                id.hash(&mut hasher);
                name.hash(&mut hasher);
                args.hash(&mut hasher);
                result.hash(&mut hasher);
                running.hash(&mut hasher);
                error.hash(&mut hasher);
                let default_expand = if name == "bash" {
                    false
                } else {
                    show_tool_output
                };
                (
                    true,
                    app.tool_expanded.get(id).copied().unwrap_or(default_expand),
                )
            }
            other => {
                std::mem::discriminant(other).hash(&mut hasher);
                (false, false)
            }
        };
        let skipped = matches!(block.kind, TranscriptKind::Reasoning) && !show_reasoning;
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
        if !skipped {
            render_block(
                block,
                width,
                &theme,
                show_reasoning,
                show_tool_output,
                &app.tool_expanded,
                &mut lines,
                &mut tool_headers,
                &mut file_links,
            );
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
