//! Tool block rendering: one-line summary for cheap tools, header + collapsible
//! body for the interesting ones (`bash`, `edit`, `write`, MCP calls). Kept
//! separate from `transcript.rs` so the summary rules stay easy to scan.

use super::*;
use serde_json::Value;

/// What a tool block looks like on screen right now.
pub(super) enum ToolRender<'a> {
    /// One line, no body. `read`, `glob`, `grep`, `todo`, `skill_read`, etc.
    Summary(RowParts),
    /// Header line the user can toggle to reveal `body`. `subtitle` is a
    /// muted second line shown when expanded (e.g. the full `bash` command
    /// when the header only carries the program name).
    Detail {
        header: RowParts,
        subtitle: Option<String>,
        body: ToolBody<'a>,
    },
}

/// A tool row split into its columns so the renderer can align them.
///
/// Each branch of `classify` used to format one finished string, which glued
/// the verb, the argument and the count together: rows began at different
/// columns depending on their shape, and the count landed wherever the text
/// happened to end. Keeping the parts separate lets the layout own the
/// alignment.
pub(super) struct RowParts {
    /// The tool as the user knows it: `bash`, `read`, `github:list_issues`.
    pub verb: String,
    /// What it acted on — a path, a pattern, a command.
    pub arg: String,
    /// The outcome worth seeing at a glance: `140 lines`, `4 matches`.
    pub metric: String,
    /// Extra status shown after the metric, such as a non-zero exit code.
    pub status: Option<String>,
}

impl RowParts {
    pub(super) fn new(
        verb: impl Into<String>,
        arg: impl Into<String>,
        metric: impl Into<String>,
    ) -> Self {
        Self {
            verb: verb.into(),
            arg: arg.into(),
            metric: metric.into(),
            status: None,
        }
    }

    fn with_status(mut self, status: Option<String>) -> Self {
        self.status = status;
        self
    }
}

/// Plural-aware count, so one result never reads "1 matches".
fn counted(n: usize, singular: &str, plural: &str) -> String {
    if n == 1 {
        format!("{n} {singular}")
    } else {
        format!("{n} {plural}")
    }
}

pub(super) enum ToolBody<'a> {
    Plain(&'a str),
    /// Output the classifier reformatted (currently: pretty-printed JSON), so
    /// it is owned rather than borrowed from the raw result.
    Formatted(String),
    /// Per-line unified diff derived from an `edit` tool's args. `start_line`
    /// is where the region begins in the target file so the gutter can show
    /// real file line numbers instead of resetting to 1. The path is not
    /// carried: the tool row above the body names the file.
    Diff {
        old: String,
        new: String,
        start_line: usize,
    },
    /// File preview capped to a max number of lines; the header carries the
    /// total count so the reader knows there is more.
    Preview {
        content: String,
        total: usize,
    },
    /// Tree list under the header: each entry rendered with a `├─` / `└─`
    /// connector so multi-file `read`, `glob`, and `grep` share one gaya.
    Tree {
        items: Vec<String>,
    },
    /// Todo checklist rendered under the header. Done rows get
    /// strikethrough, in-progress bolded, blocked yellow.
    Todo {
        items: Vec<TodoItem>,
    },
}

pub(super) struct TodoItem {
    pub state: TodoState,
    pub label: String,
}

pub(super) enum TodoState {
    Pending,
    InProgress,
    Done,
    Blocked,
    Dropped,
}

fn parse_todo_state(s: &str) -> TodoState {
    match s.to_ascii_lowercase().as_str() {
        "in_progress" | "active" | "wip" => TodoState::InProgress,
        "done" | "completed" => TodoState::Done,
        "blocked" => TodoState::Blocked,
        "dropped" | "abandoned" | "cancelled" => TodoState::Dropped,
        _ => TodoState::Pending,
    }
}

/// Render a bullet-tree list of strings under the tool header. Uses `├─`
/// for interior rows and `└─` for the last so the eye can quickly scan the
/// items as one group.
/// How many rows a tree body shows before folding the rest into a hint.
///
/// Matches the caps `write` and the diff already use. Without one, opening a
/// repo-wide grep would materialise every hit — hundreds of lines the reader
/// then has to scroll past, for a body that is only ever a spot check.
pub(super) const TREE_PREVIEW_MAX: usize = 15;

pub(super) fn render_tree(
    items: &[String],
    width: usize,
    lines: &mut Vec<Line<'static>>,
    theme: &Theme,
    file_markers: &mut Vec<(usize, String)>,
) {
    let text_w = width.saturating_sub(5).max(1);
    let shown = items.len().min(TREE_PREVIEW_MAX);
    for (i, item) in items.iter().take(shown).enumerate() {
        let connector = if i + 1 == shown && items.len() <= TREE_PREVIEW_MAX {
            "└─"
        } else {
            "├─"
        };
        let clipped = trim(item, text_w);
        // A grep hit shows as `path:line:col:content`; strip trailing parts
        // so the marker is just the file path. Glob rows are already paths.
        let path = clipped
            .split(':')
            .next()
            .unwrap_or(clipped.as_str())
            .to_string();
        let row_idx = lines.len();
        file_markers.push((row_idx, path));
        lines.push(Line::from(vec![
            Span::styled(format!("{connector} "), Style::default().fg(theme.muted)),
            Span::styled(
                clipped,
                Style::default()
                    .fg(theme.text)
                    .add_modifier(Modifier::UNDERLINED),
            ),
        ]));
    }
    if items.len() > shown {
        let extra = items.len() - shown;
        lines.push(Line::styled(
            format!("└─ {extra} more"),
            Style::default().fg(theme.muted),
        ));
    }
}

/// Render a Todo checklist under the header. Icons:
/// - `☑` done (green strikethrough), `☐` pending (accent),
/// - `▶` in-progress (bold accent), `⊡` blocked (yellow),
/// - `⊠` dropped (muted strikethrough).
pub(super) fn render_todo(
    items: &[TodoItem],
    width: usize,
    lines: &mut Vec<Line<'static>>,
    theme: &Theme,
) {
    let text_w = width.saturating_sub(6).max(1);
    for (i, it) in items.iter().enumerate() {
        let connector = if i + 1 == items.len() {
            "└─"
        } else {
            "├─"
        };
        let (icon, style) = match it.state {
            TodoState::Done => (
                "☑",
                Style::default()
                    .fg(theme.green)
                    .add_modifier(Modifier::CROSSED_OUT),
            ),
            TodoState::Pending => ("☐", Style::default().fg(theme.accent)),
            TodoState::InProgress => (
                "▶",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            TodoState::Blocked => ("⊡", Style::default().fg(theme.yellow)),
            TodoState::Dropped => (
                "⊠",
                Style::default()
                    .fg(theme.muted)
                    .add_modifier(Modifier::CROSSED_OUT),
            ),
        };
        let clipped = trim(&it.label, text_w);
        lines.push(Line::from(vec![
            Span::styled(format!("{connector} "), Style::default().fg(theme.muted)),
            Span::styled(format!("{icon} "), style),
            Span::styled(clipped, style),
        ]));
    }
}

/// Decide which representation a tool call gets. Args are the raw JSON from
/// the provider; result is the stringified tool output.
pub(super) fn classify<'a>(name: &str, args: &'a str, result: &'a str) -> ToolRender<'a> {
    let parsed: Value = serde_json::from_str(args).unwrap_or(Value::Null);
    match name {
        "bash" => {
            // Even `ls` in a large repo can spill hundreds of lines. Default
            // collapsed; click the header to expand.
            let cmd = str_arg(&parsed, "command").unwrap_or("").trim();
            // The shell tool prefixes its output with `exit N`. Lift that into
            // the header and off the body: a non-zero exit is the single most
            // useful thing about a failed command, and it was previously
            // buried as the first line of collapsed output.
            let (exit, body) = split_exit_line(result);
            let n = body.lines().filter(|l| !l.trim().is_empty()).count();
            // The header shows as much of the command as fits rather than just
            // its first word — `$ cargo` said nothing about which cargo
            // invocation this was, and collapsed rows hid the subtitle.
            let head = summarize_command(cmd);
            let status = match exit {
                Some(code) if code != "0" => Some(format!("exit {code}")),
                _ => None,
            };
            let lines_note = counted(n, "line", "lines");
            ToolRender::Detail {
                header: RowParts::new("bash", head.clone(), lines_note).with_status(status),
                // Only worth a second line when the header had to abbreviate.
                subtitle: (head.len() < cmd.len()).then(|| trim(cmd, 240)),
                body: ToolBody::Plain(body),
            }
        }
        "edit" => {
            let path = str_arg(&parsed, "path").unwrap_or("").to_string();
            let old = str_arg(&parsed, "old_text").unwrap_or("").to_string();
            let new = str_arg(&parsed, "new_text").unwrap_or("").to_string();
            // The `edit` tool ends its output with `at line N`; use that so
            // the diff gutter shows real file line numbers instead of `1..`.
            let start_line = parse_start_line(result).unwrap_or(1);
            // The size of the change is the edit's metric, in the right-hand
            // column where every other tool puts its count. It used to be on
            // a row of its own under the header, beside the path the header
            // had already named.
            let (adds, dels) = diff_counts(&old, &new);
            ToolRender::Detail {
                header: RowParts::new("edit", path.clone(), format!("+{adds} -{dels}")),
                subtitle: None,
                body: ToolBody::Diff {
                    old,
                    new,
                    start_line,
                },
            }
        }
        "write" => {
            let path = str_arg(&parsed, "path").unwrap_or("").to_string();
            let content = str_arg(&parsed, "content").unwrap_or("");
            let n = content.lines().count();
            ToolRender::Detail {
                header: RowParts::new("write", path.clone(), counted(n, "line", "lines")),
                subtitle: None,
                body: ToolBody::Preview {
                    content: content.to_string(),
                    total: n,
                },
            }
        }
        "read" => {
            // A single-file read is fine as a summary; a multi-file batch
            // (rare but possible via one call listing several paths) uses a
            // tree body instead.
            let path = str_arg(&parsed, "path").unwrap_or("").to_string();
            let paths: Vec<String> = parsed
                .get("paths")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            if paths.len() > 1 {
                ToolRender::Detail {
                    header: RowParts::new(
                        "read",
                        counted(paths.len(), "file", "files"),
                        counted(paths.len(), "file", "files"),
                    ),
                    subtitle: None,
                    body: ToolBody::Tree { items: paths },
                }
            } else {
                let n = result.lines().count();
                ToolRender::Summary(RowParts::new(
                    "read",
                    path.clone(),
                    counted(n, "line", "lines"),
                ))
            }
        }
        "glob" => {
            let pat = str_arg(&parsed, "pattern")
                .or_else(|| str_arg(&parsed, "path"))
                .unwrap_or("")
                .to_string();
            let matches: Vec<String> = result
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.trim().to_string())
                .collect();
            // One shape regardless of count. Switching between a summary row
            // and an expandable tree at exactly two results made the same
            // tool look like two different things, and mixed `Glob` with
            // `glob` in the process.
            search_render("glob", &pat, matches, "match", "matches")
        }
        "grep" => {
            let pat = str_arg(&parsed, "pattern").unwrap_or("").to_string();
            let hits: Vec<String> = result
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.trim().to_string())
                .collect();
            search_render("grep", &pat, hits, "hit", "hits")
        }
        "todo" => {
            // Parse the items array into TodoItems. enowx-cli todo tool ships each
            // entry as `{ state: "pending"|"in_progress"|"done"|"blocked"|
            // "dropped", label: "..." }`; some builds only send `label`.
            let items: Vec<TodoItem> = parsed
                .get("items")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .map(|v| {
                            let label = v
                                .get("label")
                                .or_else(|| v.get("task"))
                                .and_then(Value::as_str)
                                .or_else(|| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let state = v
                                .get("state")
                                .and_then(Value::as_str)
                                .map(parse_todo_state)
                                .unwrap_or(TodoState::Pending);
                            TodoItem { state, label }
                        })
                        .collect()
                })
                .unwrap_or_default();
            if items.is_empty() {
                ToolRender::Summary(RowParts::new("todo", "", "0 items"))
            } else {
                ToolRender::Detail {
                    header: RowParts::new(
                        "todo",
                        String::new(),
                        counted(items.len(), "task", "tasks"),
                    ),
                    subtitle: None,
                    body: ToolBody::Todo { items },
                }
            }
        }
        "skill_read" => {
            let name = str_arg(&parsed, "name").unwrap_or("").to_string();
            ToolRender::Summary(RowParts::new("skill_read", name.clone(), String::new()))
        }
        "fetch" => {
            let url = str_arg(&parsed, "url").unwrap_or("").to_string();
            ToolRender::Summary(RowParts::new(
                "fetch",
                trim(&url, 60),
                format!("{} chars", result.len()),
            ))
        }
        other if other.starts_with("mcp__") => {
            let label = other.trim_start_matches("mcp__").replacen("__", ":", 1);
            // MCP servers overwhelmingly answer with JSON, and a whole
            // response on one line is unreadable. Pretty-print it when it
            // parses; anything else passes through untouched.
            match pretty_json(result) {
                Some(pretty) => {
                    let n = pretty.lines().count();
                    ToolRender::Detail {
                        header: RowParts::new(label.clone(), "json", counted(n, "line", "lines")),
                        subtitle: None,
                        body: ToolBody::Formatted(pretty),
                    }
                }
                None => {
                    let n = result.lines().count();
                    ToolRender::Detail {
                        header: RowParts::new(
                            label.clone(),
                            String::new(),
                            counted(n, "line", "lines"),
                        ),
                        subtitle: None,
                        body: ToolBody::Plain(result),
                    }
                }
            }
        }
        _ => {
            let n = result.lines().count();
            ToolRender::Detail {
                header: RowParts::new(name, String::new(), counted(n, "line", "lines")),
                subtitle: None,
                body: ToolBody::Plain(result),
            }
        }
    }
}

fn str_arg<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

/// Pull the trailing `at line N` from an `edit` tool result. The tool emits
/// `Updated <path> at line N`; anything else falls through to `None` so the
/// caller keeps its default gutter start.
fn parse_start_line(result: &str) -> Option<usize> {
    let idx = result.rfind("at line ")?;
    let tail = &result[idx + "at line ".len()..];
    let digits: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// Max lines a `write` preview shows before truncating. Anything past this
/// gets `+N more lines` at the bottom; click the header to see full output.
pub(super) const WRITE_PREVIEW_MAX: usize = 12;

/// Compact write preview matching the diff style: single-line header, then
/// rows shaped ` NNN│content`. Overlong rows truncate with `…`; content past
/// `WRITE_PREVIEW_MAX` is folded into a `↳ N more lines` hint.
pub(super) fn render_preview(
    content: &str,
    total: usize,
    width: usize,
    lines: &mut Vec<Line<'static>>,
    theme: &Theme,
) {
    let num_w = total.to_string().len().max(2);
    // Layout: `  NNN│content` — 2 space + num_w + 1 `│`
    let text_w = width.saturating_sub(num_w + 3).max(1);

    let bg = theme.subtle;
    let border = Style::default().fg(theme.accent).bg(bg);
    let dim_bg = Style::default().fg(theme.muted).bg(bg);
    let text_bg = Style::default().fg(theme.text).bg(bg);
    let show: Vec<&str> = content.lines().take(WRITE_PREVIEW_MAX).collect();
    for (idx, raw) in show.iter().enumerate() {
        let line_no = idx + 1;
        let (indent_viz, rest) = visualize_indent(raw);
        let indent_w = indent_viz.chars().count();
        let body_budget = text_w.saturating_sub(indent_w);
        let (body_shown, _) = truncate(&rest, body_budget);
        let used = 2 + num_w + 1 + indent_w + body_shown.chars().count();
        let pad = width.saturating_sub(used);
        lines.push(Line::from(vec![
            Span::styled("│ ", border),
            Span::styled(format!("{line_no:>num_w$}"), dim_bg),
            Span::styled("│", dim_bg),
            Span::styled(indent_viz, dim_bg),
            Span::styled(body_shown, text_bg),
            Span::styled(" ".repeat(pad), Style::default().bg(bg)),
        ]));
    }
    if total > WRITE_PREVIEW_MAX {
        let extra = total - WRITE_PREVIEW_MAX;
        let msg = format!("↳ {extra} more lines");
        let pad = width.saturating_sub(2 + msg.chars().count());
        lines.push(Line::from(vec![
            Span::styled("│ ", border),
            Span::styled(msg, dim_bg),
            Span::styled(" ".repeat(pad), Style::default().bg(bg)),
        ]));
    }
}

/// Rich per-line diff: header with `✎ Edit: 📄 <path> [+N/-M]`, colored
/// line-number gutter, marker column, and inline whitespace visualization.
/// Everything sits on `theme.subtle` and reaches the right edge so the diff
/// reads as one continuous block.
/// Compact diff renderer: header, then rows shaped
/// `+NNN|content`. Prefix / number / separator are dim so the marker and
/// content stand out. Overlong rows are truncated with `…`; if the diff has
/// more than `DIFF_PREVIEW_MAX` rows, the tail is folded into a hint.
pub(super) const DIFF_PREVIEW_MAX: usize = 15;

#[allow(clippy::too_many_arguments)]
/// Lines added and removed by an edit, for the tool row's metric.
///
/// Same LCS as the rendered diff so the two agree. An edit big enough to make
/// that quadratic is counted as a whole-block replacement instead.
fn diff_counts(old: &str, new: &str) -> (usize, usize) {
    let a: Vec<&str> = old.split('\n').collect();
    let b: Vec<&str> = new.split('\n').collect();
    if a.len().saturating_mul(b.len()) > 4_000_000 {
        return (b.len(), a.len());
    }
    let ops = lcs_diff(&a, &b);
    let adds = ops.iter().filter(|op| matches!(op, DiffOp::Add(_))).count();
    let dels = ops.iter().filter(|op| matches!(op, DiffOp::Del(_))).count();
    (adds, dels)
}

/// The body of an `edit`: changed lines with their file line numbers.
///
/// No header row of its own. The tool row above already names the file,
/// carries the +/- counts and opens the file on a click.
pub(super) fn render_diff(
    old: &str,
    new: &str,
    start_line: usize,
    width: usize,
    lines: &mut Vec<Line<'static>>,
    theme: &Theme,
) {
    let old_lines: Vec<&str> = old.split('\n').collect();
    let new_lines: Vec<&str> = new.split('\n').collect();
    let ops = lcs_diff(&old_lines, &new_lines);

    let text = Style::default().fg(theme.text);
    let dim = Style::default().fg(theme.muted);
    let add_c = Style::default().fg(theme.green);
    let del_c = Style::default().fg(theme.red);

    let bg = theme.subtle;
    let border = Style::default().fg(theme.accent).bg(bg);

    let total_target = start_line + old_lines.len().max(new_lines.len());
    let num_w = total_target.to_string().len().max(2);
    // Layout: `  +NNN│content` — 2 space + 1 marker + num_w + 1 `│`
    let text_w = width.saturating_sub(num_w + 4).max(1);

    let mut old_no = start_line;
    let mut new_no = start_line;
    let mut rendered = 0usize;
    let visible_ops: Vec<&DiffOp> = ops.iter().collect();
    // Pair each replaced line with its counterpart so the row can highlight
    // just the words that moved. `pair[i]` is the line op `i` was replacing
    // (or replaced by); None for a pure insertion or deletion, where every
    // word is new and per-word emphasis would carry no signal.
    let pair = pair_replacements(&visible_ops);
    let cut = visible_ops.len().saturating_sub(DIFF_PREVIEW_MAX);
    let show = &visible_ops[..(DIFF_PREVIEW_MAX).min(visible_ops.len())];
    for (op_idx, op) in show.iter().enumerate() {
        let (marker, marker_st, content_st, n_str, content) = match op {
            DiffOp::Del(t) => {
                let n = old_no.to_string();
                old_no += 1;
                ("-", del_c.add_modifier(Modifier::BOLD), del_c, n, t.clone())
            }
            DiffOp::Add(t) => {
                let n = new_no.to_string();
                new_no += 1;
                ("+", add_c.add_modifier(Modifier::BOLD), add_c, n, t.clone())
            }
            DiffOp::Keep(t) => {
                let n = new_no.to_string();
                old_no += 1;
                new_no += 1;
                (" ", dim, text, n, t.clone())
            }
        };
        let (indent_viz, rest) = visualize_indent(&content);
        let indent_w = indent_viz.chars().count();
        let body_budget = text_w.saturating_sub(indent_w).max(1);
        // Tint the whole row, not just the glyph: a changed line should be
        // identifiable from the row's background alone, without tracking back
        // to the `+`/`-` in the gutter. Kept rows stay on the neutral panel
        // background so the eye lands on what actually changed.
        let row_bg = match op {
            DiffOp::Add(_) => blend(bg, theme.green),
            DiffOp::Del(_) => blend(bg, theme.red),
            DiffOp::Keep(_) => bg,
        };
        let num_style = dim.bg(row_bg);
        let indent_style = dim.bg(row_bg);
        let body_style = content_st.bg(row_bg);
        // Words that actually moved get a stronger background still, so a
        // one-token rename stands out inside an otherwise unchanged line.
        // Ranges are byte offsets into `rest`, i.e. after the indent.
        let emph_bg = match op {
            DiffOp::Add(_) => blend2(bg, theme.green),
            DiffOp::Del(_) => blend2(bg, theme.red),
            DiffOp::Keep(_) => row_bg,
        };
        let emph_style = content_st.bg(emph_bg).add_modifier(Modifier::BOLD);
        let changed: Vec<(usize, usize)> = match (op, pair.get(op_idx).and_then(|p| p.as_ref())) {
            (DiffOp::Keep(_), _) | (_, None) => Vec::new(),
            (_, Some(counterpart)) => {
                // Compare the post-indent text on both sides so a pure
                // re-indent does not read as a word change.
                let (_, other_rest) = visualize_indent(counterpart);
                changed_ranges(&rest, &other_rest)
            }
        };

        // A long edited line WRAPS instead of being cut with an ellipsis.
        // Truncating hid the tail of exactly the lines worth reading — a long
        // signature, a deep path — and a terminal cannot scroll a diff row
        // sideways, so those characters were unrecoverable. Continuation rows
        // repeat the gutter with a blank number so the row still reads as one
        // logical line.
        let chunks = wrap_chars(&rest, body_budget);
        let mut chunk_start = 0usize; // byte offset of this chunk within `rest`
        for (ci, chunk) in chunks.iter().enumerate() {
            let first = ci == 0;
            let indent_here = if first {
                indent_viz.clone()
            } else {
                " ".repeat(indent_w)
            };
            let num_cell = if first {
                format!("{n_str:>num_w$}")
            } else {
                " ".repeat(num_w)
            };
            let marker_cell = if first { marker } else { " " };
            let used = 2 + 1 + num_w + 1 + indent_w + chunk.chars().count();
            let pad = width.saturating_sub(used);
            let mut spans = vec![
                Span::styled("│ ", border),
                Span::styled(marker_cell.to_string(), marker_st.bg(row_bg)),
                Span::styled(num_cell, num_style),
                Span::styled("│", num_style),
                Span::styled(indent_here, indent_style),
            ];
            spans.extend(split_emphasis(
                chunk,
                chunk_start,
                &changed,
                body_style,
                emph_style,
            ));
            spans.push(Span::styled(" ".repeat(pad), Style::default().bg(row_bg)));
            lines.push(Line::from(spans));
            chunk_start += chunk.len();
        }
        rendered += 1;
    }
    let _ = rendered;
    if cut > 0 {
        let msg = format!("↳ {cut} more lines");
        let pad = width.saturating_sub(2 + msg.chars().count());
        lines.push(Line::from(vec![
            Span::styled("│ ", border),
            Span::styled(msg, dim.bg(bg)),
            Span::styled(" ".repeat(pad), Style::default().bg(bg)),
        ]));
    }
}

/// Render indent as visible glyphs (space → `·`, tab → ` → `) and return the
/// visualized indent plus the remaining text after the leading whitespace.
fn visualize_indent(s: &str) -> (String, String) {
    let mut indent = String::new();
    let mut rest = String::new();
    let mut in_leading = true;
    for c in s.chars() {
        if in_leading && c == ' ' {
            indent.push('·');
        } else if in_leading && c == '\t' {
            indent.push_str(" → ");
        } else {
            in_leading = false;
            rest.push(c);
        }
    }
    (indent, rest)
}

/// Truncate a string with `…` when it exceeds `budget`. Returns
/// `(shown, was_truncated)`.
fn truncate(text: &str, budget: usize) -> (String, bool) {
    if text.chars().count() <= budget {
        return (text.to_string(), false);
    }
    if budget == 0 {
        return (String::new(), true);
    }
    let mut s: String = text.chars().take(budget.saturating_sub(1).max(1)).collect();
    s.push('…');
    (s, true)
}

enum DiffOp {
    Keep(String),
    Del(String),
    Add(String),
}

fn lcs_diff(a: &[&str], b: &[&str]) -> Vec<DiffOp> {
    let n = a.len();
    let m = b.len();
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in 0..n {
        for j in 0..m {
            dp[i + 1][j + 1] = if a[i] == b[j] {
                dp[i][j] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut ops = Vec::new();
    let mut i = n;
    let mut j = m;
    // The walk runs backwards and is reversed at the end, so on a tie it
    // takes the addition: that puts each deletion ahead of the addition that
    // replaced it once reversed — the order a diff is read in, and the order
    // `pair_replacements` matches on. Preferring the deletion here read every
    // replaced line backwards and left none of them paired.
    while i > 0 && j > 0 {
        if a[i - 1] == b[j - 1] {
            ops.push(DiffOp::Keep(a[i - 1].to_string()));
            i -= 1;
            j -= 1;
        } else if dp[i][j - 1] >= dp[i - 1][j] {
            ops.push(DiffOp::Add(b[j - 1].to_string()));
            j -= 1;
        } else {
            ops.push(DiffOp::Del(a[i - 1].to_string()));
            i -= 1;
        }
    }
    while i > 0 {
        ops.push(DiffOp::Del(a[i - 1].to_string()));
        i -= 1;
    }
    while j > 0 {
        ops.push(DiffOp::Add(b[j - 1].to_string()));
        j -= 1;
    }
    ops.reverse();
    ops
}

/// Mix `accent` into `base` at low strength for a diff row's background.
///
/// A solid green/red fill is unreadable behind source text in a dark theme,
/// and the terminal has no alpha channel — so the blend is computed here and
/// emitted as one opaque colour. Non-RGB themes (256-colour terminals) fall
/// through to the unmodified background rather than guessing a mix.
fn blend(base: ratatui::style::Color, accent: ratatui::style::Color) -> ratatui::style::Color {
    const STRENGTH: u16 = 22; // percent of `accent` in the result
    match (base, accent) {
        (ratatui::style::Color::Rgb(br, bg_, bb), ratatui::style::Color::Rgb(ar, ag, ab)) => {
            let mix = |b: u8, a: u8| -> u8 {
                ((b as u16 * (100 - STRENGTH) + a as u16 * STRENGTH) / 100) as u8
            };
            ratatui::style::Color::Rgb(mix(br, ar), mix(bg_, ag), mix(bb, ab))
        }
        _ => base,
    }
}

/// Split `text` into chunks that each fit `budget` display columns.
///
/// Measured by display width rather than char count so CJK and emoji (two
/// columns each) cannot overflow the row. Always yields at least one chunk so
/// an empty line still emits its gutter.
fn wrap_chars(text: &str, budget: usize) -> Vec<String> {
    use unicode_width::UnicodeWidthChar;
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut w = 0usize;
    for c in text.chars() {
        let cw = c.width().unwrap_or(0).max(1);
        if w + cw > budget && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
            w = 0;
        }
        cur.push(c);
        w += cw;
    }
    out.push(cur);
    out
}

/// Split a line into word-ish tokens for intra-line diffing: runs of
/// identifier characters, and every other character on its own. Keeping
/// punctuation separate means renaming `foo` to `foo_bar` highlights the
/// suffix rather than the whole expression.
fn word_tokens(line: &str) -> Vec<&str> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
        if is_word(bytes[i]) {
            while i < bytes.len() && is_word(bytes[i]) {
                i += 1;
            }
        } else {
            // Advance one full char so multi-byte UTF-8 is never split.
            i += line[i..].chars().next().map_or(1, char::len_utf8);
        }
        out.push(&line[start..i]);
    }
    out
}

/// Which spans of a replaced line actually changed.
///
/// Returns byte ranges into `line` that differ from its counterpart. An empty
/// result means the two lines share no useful structure, and the caller should
/// fall back to colouring the whole row — highlighting everything is the same
/// as highlighting nothing.
fn changed_ranges(line: &str, other: &str) -> Vec<(usize, usize)> {
    let a = word_tokens(line);
    let b = word_tokens(other);
    // Guard against the quadratic LCS on pathological rows (minified JS, a
    // base64 blob); whole-row colouring is the honest answer there.
    if a.len() > 400 || b.len() > 400 {
        return Vec::new();
    }
    let ops = lcs_diff(&a, &b);

    // Walk the ops in order, tracking our byte offset in `line`. Only tokens
    // that exist in `line` advance it: Keep and Del for the old side. The
    // caller passes the counterpart as `other`, so Add rows are handled by
    // calling this with the arguments swapped.
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut offset = 0usize;
    for op in &ops {
        match op {
            DiffOp::Keep(t) => offset += t.len(),
            DiffOp::Del(t) => {
                let end = offset + t.len();
                // Merge with the previous range when they touch, so a run of
                // changed tokens becomes one highlight instead of many.
                match ranges.last_mut() {
                    Some(last) if last.1 == offset => last.1 = end,
                    _ => ranges.push((offset, end)),
                }
                offset = end;
            }
            DiffOp::Add(_) => {}
        }
    }
    // When almost the whole line is highlighted there is no signal left —
    // emphasising everything reads the same as emphasising nothing, and the
    // row's own tint already says "this changed". Measured on non-whitespace
    // bytes so a line that only shares its spaces still counts as a rewrite.
    let changed_bytes: usize = ranges
        .iter()
        .map(|(s, e)| line[*s..*e].chars().filter(|c| !c.is_whitespace()).count())
        .sum();
    let total_bytes = line.chars().filter(|c| !c.is_whitespace()).count();
    if total_bytes == 0 || changed_bytes * 100 >= total_bytes * 80 {
        return Vec::new();
    }
    ranges
}

/// For each op, the text of the line it replaced (or was replaced by).
///
/// LCS emits a modified line as a `Del` immediately followed by an `Add`
/// (possibly several of each when a hunk changes together). Those runs are
/// matched positionally: the first deletion pairs with the first addition and
/// so on, which is what a reader sees as "this line became that line". A run
/// with no counterpart — three deletions against one addition, say — leaves
/// the extras unpaired, because there is no single line they turned into.
fn pair_replacements(ops: &[&DiffOp]) -> Vec<Option<String>> {
    let mut pairs: Vec<Option<String>> = vec![None; ops.len()];
    let mut i = 0;
    while i < ops.len() {
        if !matches!(ops[i], DiffOp::Del(_)) {
            i += 1;
            continue;
        }
        let del_start = i;
        while i < ops.len() && matches!(ops[i], DiffOp::Del(_)) {
            i += 1;
        }
        let add_start = i;
        while i < ops.len() && matches!(ops[i], DiffOp::Add(_)) {
            i += 1;
        }
        let dels = del_start..add_start;
        let adds = add_start..i;
        for (d, a) in dels.clone().zip(adds.clone()) {
            if let (DiffOp::Del(dt), DiffOp::Add(at)) = (ops[d], ops[a]) {
                pairs[d] = Some(at.clone());
                pairs[a] = Some(dt.clone());
            }
        }
    }
    pairs
}

/// Stronger tint for the words that actually changed, layered over the row's
/// own tint so the emphasis reads as "more of the same colour" rather than a
/// different one.
fn blend2(base: ratatui::style::Color, accent: ratatui::style::Color) -> ratatui::style::Color {
    const STRENGTH: u16 = 46;
    match (base, accent) {
        (ratatui::style::Color::Rgb(br, bg_, bb), ratatui::style::Color::Rgb(ar, ag, ab)) => {
            let mix = |b: u8, a: u8| -> u8 {
                ((b as u16 * (100 - STRENGTH) + a as u16 * STRENGTH) / 100) as u8
            };
            ratatui::style::Color::Rgb(mix(br, ar), mix(bg_, ag), mix(bb, ab))
        }
        _ => base,
    }
}

/// Split one wrapped chunk into spans, applying `emph` to the parts that fall
/// inside `changed` (byte ranges relative to the whole post-indent line) and
/// `plain` to the rest. `chunk_start` is where this chunk begins in that line.
fn split_emphasis(
    chunk: &str,
    chunk_start: usize,
    changed: &[(usize, usize)],
    plain: Style,
    emph: Style,
) -> Vec<Span<'static>> {
    if changed.is_empty() {
        return vec![Span::styled(chunk.to_string(), plain)];
    }
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut buf = String::new();
    let mut buf_emph = false;
    for (i, c) in chunk.char_indices() {
        let abs = chunk_start + i;
        let is_emph = changed.iter().any(|(s, e)| abs >= *s && abs < *e);
        if is_emph != buf_emph && !buf.is_empty() {
            out.push(Span::styled(
                std::mem::take(&mut buf),
                if buf_emph { emph } else { plain },
            ));
        }
        buf_emph = is_emph;
        buf.push(c);
    }
    if !buf.is_empty() {
        out.push(Span::styled(buf, if buf_emph { emph } else { plain }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans_of(line: &str, other: &str) -> Vec<String> {
        changed_ranges(line, other)
            .into_iter()
            .map(|(s, e)| line[s..e].to_string())
            .collect()
    }

    #[test]
    fn one_word_rename_highlights_only_that_word() {
        let got = spans_of("let total = compute(a, b);", "let sum = compute(a, b);");
        assert_eq!(got, vec!["total"]);
    }

    #[test]
    fn appended_argument_highlights_the_tail_only() {
        let got = spans_of(
            "fn highlight(line: &str, state: &mut State) -> Vec<Tok> {",
            "fn highlight(line: &str) -> Vec<Tok> {",
        );
        assert!(
            got.iter().any(|s| s.contains("state")),
            "the added parameter must be highlighted, got {got:?}"
        );
        assert!(
            !got.iter().any(|s| s.contains("highlight")),
            "the unchanged function name must not be, got {got:?}"
        );
    }

    #[test]
    fn ranges_are_in_order_and_within_bounds() {
        let line = "a.b(c, d) + e.f(g)";
        let ranges = changed_ranges(line, "a.b(c) + e.f(h)");
        let mut prev_end = 0;
        for (s, e) in ranges {
            assert!(s >= prev_end, "ranges must not overlap or go backwards");
            assert!(e <= line.len(), "range must stay inside the line");
            assert!(s < e, "range must be non-empty");
            prev_end = e;
        }
    }

    /// A wholly rewritten line has no shared structure, so per-word emphasis
    /// would light up everything — which is the same as lighting up nothing.
    /// The caller relies on the empty vec to fall back to whole-row colour.
    #[test]
    fn total_rewrite_yields_no_emphasis() {
        assert!(spans_of("alpha beta", "gamma delta").is_empty());
    }

    #[test]
    fn identical_lines_have_no_changed_words() {
        assert!(spans_of("same text here", "same text here").is_empty());
    }

    #[test]
    fn multibyte_ranges_land_on_char_boundaries() {
        // Slicing on a non-boundary would panic; the assertion is that the
        // indexing inside spans_of succeeds at all.
        let got = spans_of("let 名前 = 1;", "let 名前 = 2;");
        assert_eq!(got, vec!["1"]);
    }

    #[test]
    fn pathological_line_falls_back_rather_than_hanging() {
        let long: String = (0..600).map(|i| format!("t{i} ")).collect();
        let other: String = (0..600).map(|i| format!("u{i} ")).collect();
        assert!(changed_ranges(&long, &other).is_empty());
    }

    #[test]
    fn pairing_matches_deletions_to_their_replacements() {
        let ops = [
            DiffOp::Keep("ctx".into()),
            DiffOp::Del("old one".into()),
            DiffOp::Del("old two".into()),
            DiffOp::Add("new one".into()),
            DiffOp::Add("new two".into()),
        ];
        let refs: Vec<&DiffOp> = ops.iter().collect();
        let pairs = pair_replacements(&refs);
        assert_eq!(pairs[0], None, "kept lines have no counterpart");
        assert_eq!(pairs[1], Some("new one".to_string()));
        assert_eq!(pairs[2], Some("new two".to_string()));
        assert_eq!(pairs[3], Some("old one".to_string()));
        assert_eq!(pairs[4], Some("old two".to_string()));
    }

    /// The real diff, not a hand-built op list: a replaced line must come out
    /// as its deletion followed by its addition. The pairing tests above build
    /// that order themselves, which is how an LCS emitting the addition first
    /// went unnoticed — the diff read backwards, and no replaced line was ever
    /// paired, so word-level emphasis never showed.
    #[test]
    fn a_replacement_reads_deletion_first_and_pairs() {
        let ops = lcs_diff(&["keep", "let side = 38;"], &["keep", "let side = 40;"]);
        let kinds: Vec<&str> = ops
            .iter()
            .map(|op| match op {
                DiffOp::Keep(_) => "keep",
                DiffOp::Del(_) => "del",
                DiffOp::Add(_) => "add",
            })
            .collect();
        assert_eq!(kinds, ["keep", "del", "add"]);
        let refs: Vec<&DiffOp> = ops.iter().collect();
        let pairs = pair_replacements(&refs);
        assert_eq!(pairs[1].as_deref(), Some("let side = 40;"));
        assert_eq!(pairs[2].as_deref(), Some("let side = 38;"));
    }

    #[test]
    fn unbalanced_hunk_leaves_extras_unpaired() {
        let ops = [
            DiffOp::Del("a".into()),
            DiffOp::Del("b".into()),
            DiffOp::Add("c".into()),
        ];
        let refs: Vec<&DiffOp> = ops.iter().collect();
        let pairs = pair_replacements(&refs);
        assert_eq!(pairs[0], Some("c".to_string()));
        assert_eq!(pairs[1], None, "no line left for the second deletion");
    }

    #[test]
    fn wrap_measures_display_width_not_chars() {
        // Each CJK char is two columns, so four of them fill a budget of 8.
        let chunks = wrap_chars("日本語です", 8);
        assert_eq!(chunks[0].chars().count(), 4);
    }

    #[test]
    fn wrap_always_yields_a_chunk_for_empty_input() {
        assert_eq!(wrap_chars("", 10), vec![String::new()]);
    }
}

/// Split the shell tool's `exit N\n…` prefix off its output.
///
/// Returns the code (when the prefix is present) and the remaining body. A
/// tool that does not emit the prefix — an older build, or a non-shell tool
/// routed here — passes through untouched rather than losing its first line.
fn split_exit_line(result: &str) -> (Option<&str>, &str) {
    let Some(rest) = result.strip_prefix("exit ") else {
        return (None, result);
    };
    let (code, body) = match rest.split_once('\n') {
        Some((code, body)) => (code, body),
        None => (rest, ""),
    };
    // `signal` is what the tool reports when a command was killed.
    let looks_like_code =
        !code.is_empty() && (code == "signal" || code.chars().all(|c| c.is_ascii_digit()));
    if looks_like_code {
        (Some(code), body)
    } else {
        (None, result)
    }
}

/// A one-line rendering of a shell command for the collapsed header.
///
/// Keeps the command readable instead of reducing it to its first word:
/// newlines and runs of whitespace collapse to single spaces so a heredoc or
/// a multi-line pipeline still fits on one row.
fn summarize_command(cmd: &str) -> String {
    let flat: String = cmd.split_whitespace().collect::<Vec<_>>().join(" ");
    trim(&flat, 64)
}

/// Re-indent a JSON payload for display, or None when it is not JSON.
///
/// Only objects and arrays are reformatted: a bare string or number is
/// already one line, and rewriting it would just strip its quotes. The
/// result is capped because a huge response should be scrolled, not
/// exploded into tens of thousands of rows the renderer then has to hold.
fn pretty_json(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if !(trimmed.starts_with('{') || trimmed.starts_with('[')) {
        return None;
    }
    if trimmed.len() > 256 * 1024 {
        return None;
    }
    let value: Value = serde_json::from_str(trimmed).ok()?;
    // Already multi-line? Then the server formatted it and we should not
    // second-guess the layout it chose.
    if raw.lines().count() > 1 {
        return None;
    }
    serde_json::to_string_pretty(&value).ok()
}

/// How long a still-running tool has been going, or None when it is too soon
/// to be worth saying.
///
/// Below the threshold the note would be noise on every fast call, and it
/// would also churn the render cache once a second for no benefit. Resolution
/// drops to whole seconds, then minutes, so the text changes as rarely as the
/// information does.
pub(super) fn elapsed_note(started: &std::time::Instant) -> Option<String> {
    const FLOOR: u64 = 2;
    let secs = started.elapsed().as_secs();
    if secs < FLOOR {
        return None;
    }
    Some(if secs < 60 {
        format!("{secs}s")
    } else {
        format!("{}m{:02}s", secs / 60, secs % 60)
    })
}

#[cfg(test)]
mod tool_view_tests {
    use super::*;

    /// Flatten a row back to one string, the way the old renderer did, so the
    /// assertions stay about content rather than layout.
    fn header_of(name: &str, args: &str, result: &str) -> String {
        let parts = match classify(name, args, result) {
            ToolRender::Detail { header, .. } => header,
            ToolRender::Summary(parts) => parts,
        };
        let mut out = format!("{} {} {}", parts.verb, parts.arg, parts.metric);
        if let Some(status) = parts.status {
            out.push_str(&format!(" {status}"));
        }
        out
    }

    #[test]
    fn exit_line_is_lifted_out_of_the_body() {
        let (code, body) = split_exit_line("exit 0\nhello\nworld");
        assert_eq!(code, Some("0"));
        assert_eq!(body, "hello\nworld");
    }

    #[test]
    fn a_killed_command_reports_signal() {
        let (code, body) = split_exit_line("exit signal\n");
        assert_eq!(code, Some("signal"));
        assert_eq!(body, "");
    }

    /// Output that does not carry the prefix must keep its first line — an
    /// older build, or another tool routed through the same path.
    #[test]
    fn output_without_the_prefix_is_untouched() {
        let (code, body) = split_exit_line("exiting the loop\nsecond line");
        assert_eq!(code, None);
        assert_eq!(body, "exiting the loop\nsecond line");
    }

    #[test]
    fn a_failing_command_shows_its_exit_code_in_the_header() {
        let header = header_of(
            "bash",
            r#"{"command":"cargo test"}"#,
            "exit 101\nfailures:\n  a",
        );
        assert!(
            header.contains("exit 101"),
            "a non-zero exit belongs in the header, got {header:?}"
        );
    }

    /// A successful command should not carry a redundant `exit 0`.
    #[test]
    fn a_successful_command_does_not_mention_its_exit_code() {
        let header = header_of("bash", r#"{"command":"ls"}"#, "exit 0\na\nb");
        assert!(!header.contains("exit"), "exit 0 is noise, got {header:?}");
    }

    #[test]
    fn the_header_keeps_the_whole_command_not_just_its_first_word() {
        let header = header_of(
            "bash",
            r#"{"command":"cargo test --workspace --all-features"}"#,
            "exit 0\n",
        );
        assert!(
            header.contains("cargo test --workspace"),
            "the header must identify WHICH cargo invocation this was, got {header:?}"
        );
    }

    #[test]
    fn a_multiline_command_is_flattened_for_the_header() {
        let header = header_of(
            "bash",
            "{\"command\":\"for f in *.rs; do\\n  echo $f\\ndone\"}",
            "exit 0\n",
        );
        assert!(
            !header.contains('\n'),
            "the header is one row, got {header:?}"
        );
        assert!(header.contains("for f in"), "got {header:?}");
    }

    #[test]
    fn mcp_json_is_pretty_printed() {
        let raw = r#"{"items":[{"id":1,"name":"a"}],"total":1}"#;
        let pretty = pretty_json(raw).expect("valid json object");
        assert!(
            pretty.lines().count() > 3,
            "a one-line JSON response should expand, got {pretty:?}"
        );
        // Round-trips to the same value: reformatting must not lose data.
        let a: serde_json::Value = serde_json::from_str(raw).unwrap();
        let b: serde_json::Value = serde_json::from_str(&pretty).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn non_json_output_is_left_alone() {
        assert!(pretty_json("just some text").is_none());
        assert!(pretty_json("").is_none());
        assert!(pretty_json("{not valid json").is_none());
    }

    /// A server that already formatted its reply chose that layout; do not
    /// reflow it.
    #[test]
    fn already_formatted_json_is_not_reflowed() {
        assert!(pretty_json("{\n  \"a\": 1\n}").is_none());
    }

    #[test]
    fn a_scalar_json_value_is_not_expanded() {
        assert!(pretty_json("\"just a string\"").is_none());
        assert!(pretty_json("42").is_none());
    }

    #[test]
    fn elapsed_is_silent_until_it_is_worth_saying() {
        let now = std::time::Instant::now();
        assert_eq!(elapsed_note(&now), None, "a fast call needs no timer");
    }

    #[test]
    fn elapsed_switches_to_minutes() {
        let long_ago = std::time::Instant::now() - std::time::Duration::from_secs(125);
        assert_eq!(elapsed_note(&long_ago).as_deref(), Some("2m05s"));
        let seconds = std::time::Instant::now() - std::time::Duration::from_secs(7);
        assert_eq!(elapsed_note(&seconds).as_deref(), Some("7s"));
    }
}

/// One consistent rendering for the search-style tools.
///
/// A result count of zero is worth stating plainly and has nothing to expand;
/// anything else gets the same header plus a tree of results, so `glob` and
/// `grep` read the same whether they matched once or a hundred times.
fn search_render<'a>(
    verb: &str,
    pattern: &str,
    items: Vec<String>,
    singular: &str,
    plural: &str,
) -> ToolRender<'a> {
    let n = items.len();
    let header = RowParts::new(verb, trim(pattern, 48), counted(n, singular, plural));
    if n == 0 {
        return ToolRender::Summary(header);
    }
    ToolRender::Detail {
        header,
        subtitle: None,
        body: ToolBody::Tree { items },
    }
}

#[cfg(test)]
mod search_view_tests {
    use super::*;

    fn render(name: &str, args: &str, result: &str) -> (String, bool) {
        match classify(name, args, result) {
            ToolRender::Detail { header, .. } => (flatten(header), true),
            ToolRender::Summary(parts) => (flatten(parts), false),
        }
    }

    fn flatten(parts: RowParts) -> String {
        format!("{} {} · {}", parts.verb, parts.arg, parts.metric)
    }

    /// One hit and many hits must look like the same tool — previously one
    /// produced a lowercase summary row and the other a capitalised tree.
    #[test]
    fn grep_looks_the_same_at_one_hit_and_many() {
        let (one, one_expandable) =
            render("grep", r#"{"pattern":"fn main"}"#, "src/main.rs:1:fn main");
        let (many, many_expandable) = render(
            "grep",
            r#"{"pattern":"fn main"}"#,
            "src/a.rs:1:fn main\nsrc/b.rs:2:fn main",
        );
        assert!(one.starts_with("grep") && many.starts_with("grep"));
        assert_eq!(
            one_expandable, many_expandable,
            "both counts should offer the same interaction"
        );
    }

    #[test]
    fn counts_are_grammatical() {
        let (one, _) = render("grep", r#"{"pattern":"x"}"#, "a.rs:1:x");
        assert!(
            one.contains("1 hit") && !one.contains("1 hits"),
            "got {one:?}"
        );
        let (two, _) = render("grep", r#"{"pattern":"x"}"#, "a.rs:1:x\nb.rs:1:x");
        assert!(two.contains("2 hits"), "got {two:?}");
    }

    /// Nothing found is worth saying plainly, and there is no tree to open.
    #[test]
    fn an_empty_search_is_a_summary_row() {
        let (text, expandable) = render("grep", r#"{"pattern":"nope"}"#, "");
        assert!(!expandable, "there is nothing to expand");
        assert!(text.contains("0 hits"), "got {text:?}");
    }

    #[test]
    fn glob_and_grep_share_one_shape() {
        let (g, _) = render("glob", r#"{"pattern":"**/*.rs"}"#, "a.rs\nb.rs");
        let (r, _) = render("grep", r#"{"pattern":"fn"}"#, "a.rs:1:fn\nb.rs:1:fn");
        assert!(g.starts_with("glob ") && r.starts_with("grep "));
        assert!(
            g.contains(" · 2 matches") && r.contains(" · 2 hits"),
            "got {g:?} / {r:?}"
        );
    }
}

/// Whether a tool's body is worth opening without being asked.
///
/// The line is whether the body tells the reader something the row does not.
/// A row already names the tool, what it acted on, and how much came back; a
/// body that only restates that is noise, and a long run of them buries the
/// conversation.
///
/// Collapsed does not mean hidden — every one of these keeps its chevron and
/// opens on a click, so nothing is lost, it just does not arrive uninvited.
pub(super) fn opens_by_default(name: &str, master_toggle: bool) -> bool {
    match name {
        // The paths only restate the pattern that produced them: "*.md
        // matched 4 files" is the whole story, and the agent is the one that
        // needs the list.
        "glob" => false,
        // Same shape — a count of files read, where the contents went to the
        // model rather than the reader.
        "read" => false,
        // Hits carry real content, but a search across a repo can return
        // hundreds of lines; the count answers "did it find anything" and the
        // body is there when the answer matters.
        "grep" => false,
        // Even `ls` in a large tree spills hundreds of lines.
        "bash" => false,
        // Everything else — a diff, a file being written, a todo list, an
        // MCP payload — is the reason the call was made.
        _ => master_toggle,
    }
}
