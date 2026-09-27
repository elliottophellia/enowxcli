//! Markdown to terminal rows.
//!
//! Parsing is pulldown-cmark's: CommonMark plus the GitHub extensions a
//! model's replies actually use (tables, strikethrough, task lists, alert
//! quotes). This module only decides how each construct looks in a
//! fixed-width terminal with no images and no type scale:
//!
//! - One blank row between blocks, never two, and none leading or trailing,
//!   whatever spacing the source had.
//! - A newline inside a paragraph is kept, the way chat interfaces keep it.
//!   Report lines such as `DONE: …` / `CHANGED: …` would otherwise run
//!   together into one paragraph.
//! - Containers (quotes, list items) render their children narrower and then
//!   prefix every row, so nesting composes and no row crosses the right edge.
//! - Wrapping measures display width and breaks at a space, between wide
//!   (CJK) characters, or after `/` or `-` in a long path before it cuts.

use pulldown_cmark::{
    Alignment, BlockQuoteKind, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd,
};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthChar;

use crate::theme::Theme;

/// Render `text` as rows no wider than `width` display columns.
pub(crate) fn render_markdown(
    text: &str,
    width: usize,
    lines: &mut Vec<Line<'static>>,
    theme: &Theme,
) {
    let blocks = parse(text);
    let ctx = Ctx {
        theme,
        text: Style::default().fg(theme.text),
        depth: 0,
    };
    blocks_into(&blocks, width.max(1), &ctx, true, lines);
}

// ---------------------------------------------------------------------------
// Document model
// ---------------------------------------------------------------------------

/// Inline emphasis in effect for a run. Resolved to a `Style` only when the
/// run is drawn, because the base it layers over depends on the block: body
/// text, a heading, a quote, a table header.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Marks {
    emphasis: bool,
    strong: bool,
    strike: bool,
    link: bool,
    code: bool,
    /// Image alt text: there is no image to show, so the text stands in for
    /// it, dimmed so it does not read as prose.
    quiet: bool,
}

struct Run {
    text: String,
    marks: Marks,
}

enum Block {
    Paragraph(Vec<Run>),
    Heading(HeadingLevel, Vec<Run>),
    Code {
        lang: String,
        text: String,
    },
    Quote {
        kind: Option<BlockQuoteKind>,
        children: Vec<Block>,
    },
    List {
        start: Option<u64>,
        /// Items separated by blank lines in the source. A tight list stays
        /// packed; a loose one keeps a blank row between items.
        loose: bool,
        items: Vec<Item>,
    },
    Table {
        aligns: Vec<Alignment>,
        head: Vec<Vec<Run>>,
        rows: Vec<Vec<Vec<Run>>>,
    },
    Rule,
    Html(String),
}

struct Item {
    task: Option<bool>,
    children: Vec<Block>,
}

// ---------------------------------------------------------------------------
// Events to blocks
// ---------------------------------------------------------------------------

fn parse(text: &str) -> Vec<Block> {
    // No single-tilde subscript or `^` superscript: `~/.enx` and `2^10` are
    // far more common in a coding agent's replies than either.
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM;
    let mut builder = Builder::default();
    for event in Parser::new_ext(text, options) {
        builder.event(event);
    }
    builder.finish()
}

/// An open container. Blocks finish into the innermost one.
enum Frame {
    Quote(Option<BlockQuoteKind>, Vec<Block>),
    List {
        start: Option<u64>,
        loose: bool,
        items: Vec<Item>,
    },
    Item {
        task: Option<bool>,
        children: Vec<Block>,
    },
    Table {
        aligns: Vec<Alignment>,
        head: Vec<Vec<Run>>,
        rows: Vec<Vec<Vec<Run>>>,
        row: Vec<Vec<Run>>,
    },
}

/// What the inline runs being collected belong to.
enum Inline {
    Paragraph,
    Heading(HeadingLevel),
    Cell,
}

#[derive(Default)]
struct Builder {
    root: Vec<Block>,
    frames: Vec<Frame>,
    inline: Option<(Inline, Vec<Run>)>,
    emphasis: u16,
    strong: u16,
    strike: u16,
    link: u16,
    image: u16,
    /// Runs collected when each open image started, to tell an image with
    /// alt text from one without.
    image_starts: Vec<usize>,
    code: Option<(String, String)>,
    html: Option<String>,
    /// A source newline inside a paragraph, decided when the next text
    /// arrives; see `soft_break`.
    pending_break: bool,
}

impl Builder {
    fn event(&mut self, event: Event<'_>) {
        // Inside a fence every piece of text is code, whatever it looks like.
        if let Some((_, body)) = &mut self.code {
            match event {
                Event::Text(text) => body.push_str(&text),
                Event::End(TagEnd::CodeBlock) => {
                    if let Some((lang, text)) = self.code.take() {
                        self.push(Block::Code { lang, text });
                    }
                }
                _ => {}
            }
            return;
        }
        if let Some(html) = &mut self.html {
            match event {
                Event::Html(text) | Event::Text(text) => html.push_str(&text),
                Event::End(TagEnd::HtmlBlock) => {
                    if let Some(html) = self.html.take() {
                        self.push(Block::Html(html));
                    }
                }
                _ => {}
            }
            return;
        }
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.text(&text, Marks::default()),
            Event::Code(text) => self.text(
                &text,
                Marks {
                    code: true,
                    ..Marks::default()
                },
            ),
            Event::SoftBreak => self.pending_break = true,
            Event::HardBreak => {
                self.pending_break = false;
                self.text("\n", Marks::default());
            }
            Event::InlineHtml(html) | Event::Html(html) => self.inline_html(&html),
            Event::Rule => {
                self.close_paragraph();
                self.push(Block::Rule);
            }
            Event::TaskListMarker(done) => {
                if let Some(Frame::Item { task, .. }) = self
                    .frames
                    .iter_mut()
                    .rev()
                    .find(|frame| matches!(frame, Frame::Item { .. }))
                {
                    *task = Some(done);
                }
            }
            Event::FootnoteReference(label) => self.text(
                &format!("[{label}]"),
                Marks {
                    quiet: true,
                    ..Marks::default()
                },
            ),
            Event::InlineMath(text) | Event::DisplayMath(text) => self.text(
                &text,
                Marks {
                    code: true,
                    ..Marks::default()
                },
            ),
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {
                self.close_paragraph();
                // pulldown-cmark wraps an item's text in a paragraph only
                // when the list is loose, so this is how looseness shows.
                let n = self.frames.len();
                if n >= 2 && matches!(self.frames[n - 1], Frame::Item { .. }) {
                    if let Frame::List { loose, .. } = &mut self.frames[n - 2] {
                        *loose = true;
                    }
                }
                self.inline = Some((Inline::Paragraph, Vec::new()));
            }
            Tag::Heading { level, .. } => {
                self.close_paragraph();
                self.inline = Some((Inline::Heading(level), Vec::new()));
            }
            Tag::BlockQuote(kind) => {
                self.close_paragraph();
                self.frames.push(Frame::Quote(kind, Vec::new()));
            }
            Tag::CodeBlock(kind) => {
                self.close_paragraph();
                let lang = match kind {
                    // The info string can carry more than the language
                    // (`rust ignore`, `js title=app.js`); the first word is
                    // the language.
                    CodeBlockKind::Fenced(info) => info
                        .split(|c: char| c.is_whitespace() || c == ',' || c == '{')
                        .next()
                        .unwrap_or("")
                        .to_owned(),
                    CodeBlockKind::Indented => String::new(),
                };
                self.code = Some((lang, String::new()));
            }
            Tag::HtmlBlock => {
                self.close_paragraph();
                self.html = Some(String::new());
            }
            Tag::List(start) => {
                self.close_paragraph();
                self.frames.push(Frame::List {
                    start,
                    loose: false,
                    items: Vec::new(),
                });
            }
            Tag::Item => {
                self.close_paragraph();
                self.frames.push(Frame::Item {
                    task: None,
                    children: Vec::new(),
                });
            }
            Tag::Table(aligns) => {
                self.close_paragraph();
                self.frames.push(Frame::Table {
                    aligns,
                    head: Vec::new(),
                    rows: Vec::new(),
                    row: Vec::new(),
                });
            }
            Tag::TableHead | Tag::TableRow => {
                if let Some(Frame::Table { row, .. }) = self.frames.last_mut() {
                    row.clear();
                }
            }
            Tag::TableCell => self.inline = Some((Inline::Cell, Vec::new())),
            Tag::Emphasis => self.emphasis += 1,
            Tag::Strong => self.strong += 1,
            Tag::Strikethrough => self.strike += 1,
            Tag::Link { .. } => self.link += 1,
            Tag::Image { .. } => {
                self.image += 1;
                let runs = self.inline.as_ref().map_or(0, |(_, runs)| runs.len());
                self.image_starts.push(runs);
            }
            // Not enabled; listed so a new option cannot silently drop text.
            Tag::Superscript
            | Tag::Subscript
            | Tag::FootnoteDefinition(_)
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition
            | Tag::MetadataBlock(_) => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph | TagEnd::Heading(_) => self.finish_inline(),
            TagEnd::BlockQuote(_) => {
                self.close_paragraph();
                if let Some(Frame::Quote(kind, children)) = self.frames.pop() {
                    self.push(Block::Quote { kind, children });
                }
            }
            TagEnd::List(_) => {
                self.close_paragraph();
                if let Some(Frame::List {
                    start,
                    loose,
                    items,
                }) = self.frames.pop()
                {
                    self.push(Block::List {
                        start,
                        loose,
                        items,
                    });
                }
            }
            TagEnd::Item => {
                self.close_paragraph();
                if let Some(Frame::Item { task, children }) = self.frames.pop() {
                    if let Some(Frame::List { items, .. }) = self.frames.last_mut() {
                        items.push(Item { task, children });
                    }
                }
            }
            TagEnd::TableCell => {
                if let Some((Inline::Cell, runs)) = self.inline.take() {
                    if let Some(Frame::Table { row, .. }) = self.frames.last_mut() {
                        row.push(runs);
                    }
                }
            }
            TagEnd::TableHead => {
                if let Some(Frame::Table { head, row, .. }) = self.frames.last_mut() {
                    *head = std::mem::take(row);
                }
            }
            TagEnd::TableRow => {
                if let Some(Frame::Table { rows, row, .. }) = self.frames.last_mut() {
                    rows.push(std::mem::take(row));
                }
            }
            TagEnd::Table => {
                if let Some(Frame::Table {
                    aligns, head, rows, ..
                }) = self.frames.pop()
                {
                    self.push(Block::Table { aligns, head, rows });
                }
            }
            TagEnd::Emphasis => self.emphasis = self.emphasis.saturating_sub(1),
            TagEnd::Strong => self.strong = self.strong.saturating_sub(1),
            TagEnd::Strikethrough => self.strike = self.strike.saturating_sub(1),
            TagEnd::Link => self.link = self.link.saturating_sub(1),
            TagEnd::Image => {
                let before = self.image_starts.pop().unwrap_or(0);
                let now = self.inline.as_ref().map_or(0, |(_, runs)| runs.len());
                // An image with no alt text still leaves a mark, or the
                // sentence around it reads as if a word went missing.
                if now == before {
                    self.text("image", Marks::default());
                }
                self.image = self.image.saturating_sub(1);
            }
            _ => {}
        }
    }

    fn marks(&self) -> Marks {
        Marks {
            emphasis: self.emphasis > 0 || self.image > 0,
            strong: self.strong > 0,
            strike: self.strike > 0,
            link: self.link > 0,
            code: false,
            quiet: self.image > 0,
        }
    }

    fn text(&mut self, text: &str, extra: Marks) {
        let base = self.marks();
        let marks = Marks {
            emphasis: base.emphasis || extra.emphasis,
            strong: base.strong || extra.strong,
            strike: base.strike || extra.strike,
            link: base.link || extra.link,
            code: extra.code,
            quiet: base.quiet || extra.quiet,
        };
        let clean = sanitize(text);
        if clean.is_empty() {
            return;
        }
        if std::mem::take(&mut self.pending_break) {
            let separator = self.soft_break(clean.chars().next());
            self.append(separator, Marks::default());
        }
        self.append(&clean, marks);
    }

    fn append(&mut self, text: &str, marks: Marks) {
        // Text straight inside a list item (a tight list) has no paragraph
        // of its own; it gets one here.
        let (_, runs) = self
            .inline
            .get_or_insert_with(|| (Inline::Paragraph, Vec::new()));
        match runs.last_mut() {
            Some(last) if last.marks == marks => last.text.push_str(text),
            _ => runs.push(Run {
                text: text.to_owned(),
                marks,
            }),
        }
    }

    /// What a newline inside a paragraph becomes.
    ///
    /// Usually a newline: chat interfaces keep them, and a model's replies
    /// rely on it for line-per-item text (`DONE: …` / `CHANGED: …`). The
    /// exception is prose hard-wrapped in the source, a long line whose
    /// sentence carries on in lower case on the next; that is joined, or a
    /// panel narrower than the source leaves a one-word row at every break.
    fn soft_break(&self, next: Option<char>) -> &'static str {
        let line: usize = self.inline.as_ref().map_or(0, |(_, runs)| {
            runs.iter()
                .rev()
                .flat_map(|run| run.text.chars().rev())
                .take_while(|c| *c != '\n')
                .map(char_width)
                .sum()
        });
        if line >= HARD_WRAPPED && next.is_some_and(char::is_lowercase) {
            " "
        } else {
            "\n"
        }
    }

    fn inline_html(&mut self, html: &str) {
        let tag = html.trim();
        if tag.starts_with("<!--") {
            return;
        }
        let name: String = tag
            .trim_start_matches('<')
            .trim_start_matches('/')
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_ascii_lowercase();
        if name == "br" {
            self.text("\n", Marks::default());
            return;
        }
        // Formatting tags wrap text that reads fine without them.
        const DROPPED: [&str; 14] = [
            "kbd", "b", "i", "u", "em", "strong", "s", "del", "ins", "sub", "sup", "small", "mark",
            "span",
        ];
        if DROPPED.contains(&name.as_str()) {
            return;
        }
        // Anything else is most likely prose that happens to look like a
        // tag: `Vec<String>` written without backticks.
        self.text(html, Marks::default());
    }

    /// Finish a paragraph opened implicitly by an item's text.
    fn close_paragraph(&mut self) {
        if matches!(self.inline, Some((Inline::Paragraph, _))) {
            self.finish_inline();
        }
    }

    fn finish_inline(&mut self) {
        match self.inline.take() {
            Some((Inline::Paragraph, runs)) => {
                if runs.iter().any(|run| !run.text.trim().is_empty()) {
                    self.push(Block::Paragraph(trim_runs(runs)));
                }
            }
            Some((Inline::Heading(level), runs)) => {
                self.push(Block::Heading(level, trim_runs(runs)));
            }
            // A cell ends at its own tag; anything else is left alone.
            other => self.inline = other,
        }
    }

    fn push(&mut self, block: Block) {
        match self.frames.last_mut() {
            Some(Frame::Quote(_, children)) | Some(Frame::Item { children, .. }) => {
                children.push(block)
            }
            // Blocks never finish straight into a list or a table; if an odd
            // event order ever gets here, keep the content rather than lose it.
            _ => self.root.push(block),
        }
    }

    fn finish(mut self) -> Vec<Block> {
        self.close_paragraph();
        if let Some((lang, text)) = self.code.take() {
            self.push(Block::Code { lang, text });
        }
        if let Some(html) = self.html.take() {
            self.push(Block::Html(html));
        }
        // pulldown-cmark closes everything it opens; unwinding here only
        // guards against a truncated event stream.
        while let Some(frame) = self.frames.pop() {
            let block = match frame {
                Frame::Quote(kind, children) => Block::Quote { kind, children },
                Frame::List {
                    start,
                    loose,
                    items,
                } => Block::List {
                    start,
                    loose,
                    items,
                },
                Frame::Item { task, children } => {
                    if let Some(Frame::List { items, .. }) = self.frames.last_mut() {
                        items.push(Item { task, children });
                        continue;
                    }
                    Block::List {
                        start: None,
                        loose: false,
                        items: vec![Item { task, children }],
                    }
                }
                Frame::Table {
                    aligns, head, rows, ..
                } => Block::Table { aligns, head, rows },
            };
            self.push(block);
        }
        self.root
    }
}

/// A source line at least this wide, continued in lower case, is taken for
/// hard-wrapped prose.
const HARD_WRAPPED: usize = 60;

/// Tabs become spaces and control characters go: both have no reliable
/// width, and an escape byte reaching the terminal would be interpreted.
fn sanitize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\t' => out.push_str("    "),
            '\n' => out.push('\n'),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// Drop the break a paragraph can end on (a trailing `<br>`), and one it
/// can start on, so a block never opens or closes with an empty row.
fn trim_runs(mut runs: Vec<Run>) -> Vec<Run> {
    while let Some(last) = runs.last_mut() {
        let trimmed = last.text.trim_end_matches('\n').len();
        last.text.truncate(trimmed);
        if last.text.is_empty() {
            runs.pop();
        } else {
            break;
        }
    }
    while let Some(first) = runs.first_mut() {
        let trimmed = first.text.trim_start_matches('\n').to_owned();
        first.text = trimmed;
        if first.text.is_empty() {
            runs.remove(0);
        } else {
            break;
        }
    }
    runs
}

// ---------------------------------------------------------------------------
// Blocks to rows
// ---------------------------------------------------------------------------

struct Ctx<'a> {
    theme: &'a Theme,
    /// Style of body text here: a quote dims it.
    text: Style,
    /// How many lists deep, for the bullet glyph.
    depth: usize,
}

/// Blocks in order. `spaced` puts a blank row between them; a tight list
/// item's text and the list nested under it stay together.
fn blocks_into(
    blocks: &[Block],
    width: usize,
    ctx: &Ctx,
    spaced: bool,
    out: &mut Vec<Line<'static>>,
) {
    let mut previous: Option<&Block> = None;
    for block in blocks {
        // Drawn aside first: a block can come out empty (an HTML comment),
        // and an empty block must not leave its separator behind.
        let mut rows = Vec::new();
        block_into(block, width, ctx, &mut rows);
        if rows.is_empty() {
            continue;
        }
        if let Some(previous) = previous {
            let heading =
                matches!(block, Block::Heading(..)) || matches!(previous, Block::Heading(..));
            if spaced || heading {
                out.push(Line::default());
            }
        }
        out.append(&mut rows);
        previous = Some(block);
    }
}

fn block_into(block: &Block, width: usize, ctx: &Ctx, out: &mut Vec<Line<'static>>) {
    let theme = ctx.theme;
    match block {
        Block::Paragraph(runs) => runs_into(runs, ctx.text, width, theme, out),
        Block::Heading(level, runs) => {
            runs_into(runs, heading_style(*level, theme), width, theme, out)
        }
        Block::Code { lang, text } => code_into(lang, text, width, theme, out),
        Block::Quote { kind, children } => quote_into(*kind, children, width, ctx, out),
        Block::List {
            start,
            loose,
            items,
        } => list_into(*start, *loose, items, width, ctx, out),
        Block::Table { aligns, head, rows } => table_into(aligns, head, rows, width, ctx, out),
        Block::Rule => out.push(Line::styled(
            "─".repeat(width),
            Style::default().fg(theme.border),
        )),
        Block::Html(html) => {
            let style = Style::default().fg(theme.muted);
            let html = html.trim();
            if html.starts_with("<!--") && html.ends_with("-->") {
                return;
            }
            for raw in html.lines() {
                let raw = raw.trim_end();
                if raw.trim_start().starts_with("<!--") && raw.ends_with("-->") {
                    continue;
                }
                let cells: Vec<(char, Style)> = sanitize(raw).chars().map(|c| (c, style)).collect();
                for row in wrap(&cells, width) {
                    out.push(line_of(&row));
                }
            }
        }
    }
}

/// Headings have no size to differ by, so levels differ by colour and weight.
fn heading_style(level: HeadingLevel, theme: &Theme) -> Style {
    match level {
        HeadingLevel::H1 => Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        HeadingLevel::H2 => Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
        HeadingLevel::H3 => Style::default()
            .fg(theme.accent2)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
    }
}

/// The style a run is drawn in, layered over its block's base.
fn resolve(base: Style, marks: Marks, theme: &Theme) -> Style {
    let mut style = base;
    if marks.code {
        // Code keeps its own colour and ground whatever it sits in, since
        // that is what identifies it; the outer weight still applies.
        style = style.fg(theme.accent2).bg(theme.subtle);
    } else if marks.link {
        style = style.fg(theme.accent);
    } else if marks.strike || marks.quiet {
        style = style.fg(theme.muted);
    }
    if marks.link {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    if marks.strike {
        style = style.add_modifier(Modifier::CROSSED_OUT);
    }
    if marks.emphasis {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if marks.strong {
        style = style.add_modifier(Modifier::BOLD);
    }
    style
}

fn cells_of(runs: &[Run], base: Style, theme: &Theme) -> Vec<(char, Style)> {
    let mut cells = Vec::new();
    for run in runs {
        let style = resolve(base, run.marks, theme);
        cells.extend(run.text.chars().map(|c| (c, style)));
    }
    cells
}

fn runs_into(runs: &[Run], base: Style, width: usize, theme: &Theme, out: &mut Vec<Line<'static>>) {
    for row in wrap(&cells_of(runs, base, theme), width) {
        out.push(line_of(&row));
    }
}

fn code_into(lang: &str, text: &str, width: usize, theme: &Theme, out: &mut Vec<Line<'static>>) {
    let band = Style::default().bg(theme.subtle);
    // The language, when there is one, on a row of its own at the top of the
    // band. A block without one needs no label to be read as code.
    if !lang.is_empty() {
        let label: String = {
            let mut used = 0;
            lang.chars()
                .take_while(|c| {
                    used += char_width(*c);
                    used + 2 <= width
                })
                .collect()
        };
        let pad = width.saturating_sub(2 + str_width(&label));
        out.push(Line::from(vec![
            Span::styled("│ ", band.fg(theme.muted)),
            Span::styled(label, band.fg(theme.muted).add_modifier(Modifier::ITALIC)),
            Span::styled(" ".repeat(pad), band),
        ]));
    }
    let syntax = crate::syntax::lookup(lang);
    let mut state = crate::syntax::State::default();
    let body = text.strip_suffix('\n').unwrap_or(text);
    for raw in body.split('\n') {
        code_line(
            out,
            &sanitize(raw),
            width,
            syntax.as_ref(),
            &mut state,
            theme,
        );
    }
}

/// One source line of a code block: gutter, highlighted body, background
/// padded out to `width`.
///
/// Long lines soft-wrap rather than being cut with an ellipsis: the terminal
/// cannot scroll a block sideways, so a cut tail (a long signature, a deep
/// path) was simply unrecoverable. A continuation row's gutter says it
/// continues the line above.
pub(super) fn code_line(
    lines: &mut Vec<Line<'static>>,
    raw: &str,
    width: usize,
    syntax: Option<&crate::syntax::Syntax>,
    state: &mut crate::syntax::State,
    theme: &Theme,
) {
    const GUTTER: &str = "│ ";
    const CONT: &str = "│↳";
    let usable = width.saturating_sub(2).max(1);
    let band = Style::default().bg(theme.subtle);

    let runs: Vec<(String, crate::syntax::Tok)> = match syntax {
        Some(s) => crate::syntax::highlight(raw, s, state),
        None => vec![(raw.to_string(), crate::syntax::Tok::Plain)],
    };
    let mut cells: Vec<(char, crate::syntax::Tok)> = Vec::new();
    for (text, tok) in &runs {
        cells.extend(text.chars().map(|c| (c, *tok)));
    }
    if cells.is_empty() {
        lines.push(Line::from(vec![
            Span::styled(GUTTER, band.fg(theme.muted)),
            Span::styled(" ".repeat(usable), band),
        ]));
        return;
    }

    let mut index = 0;
    let mut first = true;
    while index < cells.len() {
        let mut used = 0usize;
        let mut end = index;
        while end < cells.len() {
            let w = char_width(cells[end].0);
            if used + w > usable {
                break;
            }
            used += w;
            end += 1;
        }
        // A single cell wider than the whole block would loop forever.
        if end == index {
            end = index + 1;
            used = usable;
        }
        let mut spans: Vec<Span<'static>> = Vec::with_capacity(4);
        spans.push(Span::styled(
            if first { GUTTER } else { CONT },
            band.fg(if first { theme.muted } else { theme.faint }),
        ));
        let mut run_start = index;
        while run_start < end {
            let tok = cells[run_start].1;
            let mut run_end = run_start;
            while run_end < end && cells[run_end].1 == tok {
                run_end += 1;
            }
            let body: String = cells[run_start..run_end].iter().map(|(c, _)| *c).collect();
            spans.push(Span::styled(body, tok.style(theme).bg(theme.subtle)));
            run_start = run_end;
        }
        spans.push(Span::styled(" ".repeat(usable.saturating_sub(used)), band));
        lines.push(Line::from(spans));
        index = end;
        first = false;
    }
}

fn quote_into(
    kind: Option<BlockQuoteKind>,
    children: &[Block],
    width: usize,
    ctx: &Ctx,
    out: &mut Vec<Line<'static>>,
) {
    let theme = ctx.theme;
    // A plain quote is set back: dimmed bar, dimmed text. An alert (GitHub's
    // `> [!WARNING]`) is the opposite, something the reader must not miss,
    // so it names itself in its own colour and keeps the text at full
    // strength.
    let (bar, label, text) = match kind {
        None => (
            Style::default().fg(theme.border),
            None,
            ctx.text.fg(theme.muted),
        ),
        Some(kind) => {
            let (name, colour) = match kind {
                BlockQuoteKind::Note => ("Note", theme.accent),
                BlockQuoteKind::Tip => ("Tip", theme.green),
                BlockQuoteKind::Important => ("Important", theme.accent2),
                BlockQuoteKind::Warning => ("Warning", theme.yellow),
                BlockQuoteKind::Caution => ("Caution", theme.red),
            };
            (Style::default().fg(colour), Some(name), ctx.text)
        }
    };
    let inner_width = width.saturating_sub(2).max(1);
    let mut inner: Vec<Line<'static>> = Vec::new();
    if let Some(label) = label {
        inner.push(Line::styled(label, bar.add_modifier(Modifier::BOLD)));
    }
    let child = Ctx {
        theme,
        text,
        depth: ctx.depth,
    };
    blocks_into(children, inner_width, &child, true, &mut inner);
    for line in inner {
        let mut spans = vec![Span::styled("│ ", bar)];
        spans.extend(line.spans);
        out.push(Line::from(spans));
    }
}

fn list_into(
    start: Option<u64>,
    loose: bool,
    items: &[Item],
    width: usize,
    ctx: &Ctx,
    out: &mut Vec<Line<'static>>,
) {
    let theme = ctx.theme;
    let marker = Style::default().fg(theme.muted);
    // Numbers right-align to the widest one, so `9.` and `10.` end on the
    // same column and every item's text starts on the same one.
    let digits = start.map_or(0, |first| {
        (first + items.len().saturating_sub(1) as u64)
            .to_string()
            .len()
    });
    let bullet = if ctx.depth.is_multiple_of(2) {
        "•"
    } else {
        "◦"
    };
    let child = Ctx {
        theme,
        text: ctx.text,
        depth: ctx.depth + 1,
    };
    for (index, item) in items.iter().enumerate() {
        if index > 0 && loose {
            out.push(Line::default());
        }
        let mut lead: Vec<Span<'static>> = Vec::new();
        match start {
            Some(first) => lead.push(Span::styled(
                format!("{:>digits$}. ", first + index as u64),
                marker,
            )),
            None if item.task.is_none() => lead.push(Span::styled(format!("{bullet} "), marker)),
            None => {}
        }
        match item.task {
            Some(true) => lead.push(Span::styled("☑ ", Style::default().fg(theme.green))),
            Some(false) => lead.push(Span::styled("☐ ", marker)),
            None => {}
        }
        let lead_width: usize = lead.iter().map(|span| str_width(&span.content)).sum();
        let mut inner: Vec<Line<'static>> = Vec::new();
        blocks_into(
            &item.children,
            width.saturating_sub(lead_width).max(1),
            &child,
            loose,
            &mut inner,
        );
        if inner.is_empty() {
            inner.push(Line::default());
        }
        for (row, line) in inner.into_iter().enumerate() {
            if row == 0 {
                let mut spans = lead.clone();
                spans.extend(line.spans);
                out.push(Line::from(spans));
            } else if line.spans.is_empty() {
                out.push(Line::default());
            } else {
                let mut spans = vec![Span::raw(" ".repeat(lead_width))];
                spans.extend(line.spans);
                out.push(Line::from(spans));
            }
        }
    }
}

fn table_into(
    aligns: &[Alignment],
    head: &[Vec<Run>],
    rows: &[Vec<Vec<Run>>],
    width: usize,
    ctx: &Ctx,
    out: &mut Vec<Line<'static>>,
) {
    let theme = ctx.theme;
    let columns = aligns
        .len()
        .max(head.len())
        .max(rows.iter().map(Vec::len).max().unwrap_or(0));
    if columns == 0 {
        return;
    }
    let header_style = ctx.text.add_modifier(Modifier::BOLD);
    let cell = |row: &[Vec<Run>], column: usize, base: Style| -> Vec<(char, Style)> {
        row.get(column)
            .map(|runs| cells_of(runs, base, theme))
            .unwrap_or_default()
    };
    let head_cells: Vec<Vec<(char, Style)>> =
        (0..columns).map(|c| cell(head, c, header_style)).collect();
    let body_cells: Vec<Vec<Vec<(char, Style)>>> = rows
        .iter()
        .map(|row| (0..columns).map(|c| cell(row, c, ctx.text)).collect())
        .collect();
    let has_head = head_cells.iter().any(|cells| !cells.is_empty());

    // Each column costs its text plus a space either side and one border;
    // the row adds one more border at the end.
    let chrome = 3 * columns + 1;
    let Some(widths) = width
        .checked_sub(chrome)
        .and_then(|available| column_widths(&head_cells, &body_cells, available))
    else {
        stacked_table(&head_cells, &body_cells, width, ctx, out);
        return;
    };

    let border = Style::default().fg(theme.border);
    let rule = |left: &str, mid: &str, right: &str| -> Line<'static> {
        let mut text = String::from(left);
        for (index, w) in widths.iter().enumerate() {
            text.push_str(&"─".repeat(w + 2));
            text.push_str(if index + 1 == columns { right } else { mid });
        }
        Line::styled(text, border)
    };
    let wrapped = |cells: &[Vec<(char, Style)>]| -> Vec<Vec<Vec<(char, Style)>>> {
        cells
            .iter()
            .zip(&widths)
            .map(|(cells, w)| wrap(cells, *w))
            .collect()
    };
    let draw_row = |cells: &[Vec<(char, Style)>], out: &mut Vec<Line<'static>>| {
        let columns_rows = wrapped(cells);
        let height = columns_rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
        for line in 0..height {
            let mut spans = vec![Span::styled("│", border)];
            for (column, w) in widths.iter().enumerate() {
                let content = columns_rows[column].get(line).cloned().unwrap_or_default();
                let used: usize = content.iter().map(|(c, _)| char_width(*c)).sum();
                let pad = w.saturating_sub(used);
                let (left, right) = match aligns.get(column) {
                    Some(Alignment::Right) => (pad, 0),
                    Some(Alignment::Center) => (pad / 2, pad - pad / 2),
                    _ => (0, pad),
                };
                spans.push(Span::raw(" ".repeat(1 + left)));
                spans.extend(line_of(&content).spans);
                spans.push(Span::raw(" ".repeat(right + 1)));
                spans.push(Span::styled("│", border));
            }
            out.push(Line::from(spans));
        }
    };

    out.push(rule("╭", "┬", "╮"));
    if has_head {
        draw_row(&head_cells, out);
        if !body_cells.is_empty() {
            out.push(rule("├", "┼", "┤"));
        }
    }
    // Rows of one line each read fine packed. Once a cell wraps, the eye can
    // no longer tell where one row ends, so every row gets a rule under it.
    let tall = body_cells.iter().any(|row| {
        row.iter()
            .zip(&widths)
            .any(|(cells, w)| wrap(cells, *w).len() > 1)
    });
    for (index, row) in body_cells.iter().enumerate() {
        if index > 0 && tall {
            out.push(rule("├", "┼", "┤"));
        }
        draw_row(row, out);
    }
    out.push(rule("╰", "┴", "╯"));
}

/// Longest token a column may break. Past this a token is a path or a URL,
/// which has break points of its own and reads fine split; an ordinary word
/// broken across rows does not.
const UNBREAKABLE: usize = 20;

/// Share `available` columns out: each column at its natural width while
/// that fits, otherwise the widest give way first, down to their longest
/// word. `None` when even that does not fit, and the table should stack.
fn column_widths(
    head: &[Vec<(char, Style)>],
    body: &[Vec<Vec<(char, Style)>>],
    available: usize,
) -> Option<Vec<usize>> {
    let columns = head.len();
    let mut natural = vec![1usize; columns];
    let mut word = vec![1usize; columns];
    for row in std::iter::once(head).chain(body.iter().map(Vec::as_slice)) {
        for (column, cells) in row.iter().enumerate() {
            let mut line = 0;
            let mut current = 0;
            for (c, _) in cells {
                if *c == '\n' {
                    line = 0;
                    current = 0;
                    continue;
                }
                let w = char_width(*c);
                line += w;
                natural[column] = natural[column].max(line);
                if *c == ' ' {
                    current = 0;
                } else {
                    current += w;
                    word[column] = word[column].max(current);
                }
            }
        }
    }
    let floor: Vec<usize> = (0..columns)
        .map(|c| word[c].min(natural[c]).min(UNBREAKABLE))
        .collect();
    if floor.iter().sum::<usize>() > available {
        return None;
    }
    let mut widths = natural;
    let mut excess = widths.iter().sum::<usize>().saturating_sub(available);
    while excess > 0 {
        let column = (0..columns)
            .filter(|&c| widths[c] > floor[c])
            .max_by_key(|&c| widths[c])?;
        widths[column] -= 1;
        excess -= 1;
    }
    Some(widths)
}

/// Too narrow for columns: each row becomes a small block of `header: value`
/// lines, rows separated by a blank line.
fn stacked_table(
    head: &[Vec<(char, Style)>],
    body: &[Vec<Vec<(char, Style)>>],
    width: usize,
    ctx: &Ctx,
    out: &mut Vec<Line<'static>>,
) {
    let label = Style::default().fg(ctx.theme.muted);
    for (index, row) in body.iter().enumerate() {
        if index > 0 {
            out.push(Line::default());
        }
        for (column, cells) in row.iter().enumerate() {
            let mut line: Vec<(char, Style)> = head
                .get(column)
                .map(|name| name.iter().map(|(c, _)| (*c, label)).collect())
                .unwrap_or_default();
            if !line.is_empty() {
                line.extend(": ".chars().map(|c| (c, label)));
            }
            line.extend(cells.iter().copied());
            // Continuation rows sit in by two, so a wrapped value is not read
            // as the next field.
            for (index, row) in wrap(&line, width.saturating_sub(2)).iter().enumerate() {
                let mut row = line_of(row);
                if index > 0 {
                    row.spans.insert(0, Span::raw("  "));
                }
                out.push(row);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Wrapping
// ---------------------------------------------------------------------------

fn char_width(c: char) -> usize {
    c.width().unwrap_or(0).max(1)
}

fn str_width(text: &str) -> usize {
    text.chars().map(char_width).sum()
}

fn is_wide(c: char) -> bool {
    c.width().unwrap_or(0) > 1
}

/// Break styled characters into rows no wider than `width`. A `\n` always
/// ends a row.
fn wrap(cells: &[(char, Style)], width: usize) -> Vec<Vec<(char, Style)>> {
    let width = width.max(1);
    let mut rows = Vec::new();
    for segment in cells.split(|(c, _)| *c == '\n') {
        wrap_segment(segment, width, &mut rows);
    }
    rows
}

fn wrap_segment(cells: &[(char, Style)], width: usize, rows: &mut Vec<Vec<(char, Style)>>) {
    if cells.is_empty() {
        rows.push(Vec::new());
        return;
    }
    let mut start = 0;
    loop {
        let mut end = start;
        let mut used = 0;
        while end < cells.len() {
            let w = char_width(cells[end].0);
            if used + w > width {
                break;
            }
            used += w;
            end += 1;
        }
        if end == cells.len() {
            rows.push(trim_end(&cells[start..end]));
            return;
        }
        if end == start {
            // One character wider than the whole row: it gets a row alone.
            end = start + 1;
        } else if let Some(at) = break_before(cells, start, end) {
            end = at;
        }
        rows.push(trim_end(&cells[start..end]));
        start = end;
        // The space a row broke at is not carried onto the next one.
        while start < cells.len() && cells[start].0 == ' ' {
            start += 1;
        }
        if start == cells.len() {
            return;
        }
    }
}

/// Where to end a row that would overflow at `end`: after the last space,
/// or between wide characters (scripts without spaces break anywhere), and
/// failing both after a `/` or `-` in a long path or name. `None` cuts at
/// `end`.
fn break_before(cells: &[(char, Style)], start: usize, end: usize) -> Option<usize> {
    let soft = |at: usize| {
        let (before, after) = (cells[at - 1].0, cells[at].0);
        after == ' ' || before == ' ' || is_wide(before) || is_wide(after)
    };
    if let Some(at) = (start + 1..=end).rev().find(|&at| soft(at)) {
        return Some(at);
    }
    (start + 1..=end)
        .rev()
        .find(|&at| matches!(cells[at - 1].0, '/' | '-' | '_' | '.' | ','))
}

fn trim_end(cells: &[(char, Style)]) -> Vec<(char, Style)> {
    let mut end = cells.len();
    while end > 0 && cells[end - 1].0 == ' ' {
        end -= 1;
    }
    cells[..end].to_vec()
}

/// Group consecutive characters of one style into spans.
fn line_of(cells: &[(char, Style)]) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut current = String::new();
    let mut style = Style::default();
    for (c, cell_style) in cells {
        if *cell_style != style && !current.is_empty() {
            spans.push(Span::styled(std::mem::take(&mut current), style));
        }
        style = *cell_style;
        current.push(*c);
    }
    if !current.is_empty() {
        spans.push(Span::styled(current, style));
    }
    Line::from(spans)
}
