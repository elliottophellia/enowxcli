//! Cutting a file into chunks that are short but whole.
//!
//! Code is cut along its syntax tree (tree-sitter, for the languages enx has
//! a grammar for): a function, a struct, a class method or a JSON key is one
//! unit, with the comments and attributes above it. Small units next to each
//! other are merged up to [`TARGET`]; a unit is only split when it is larger
//! than [`MAX`], and then along its own body (the methods of a class, the
//! statements of a function) before falling back to blank lines. Markdown is
//! cut at its headings, other text at its paragraphs. A cut never lands
//! mid-line, and a single line longer than a chunk is the only thing cut on
//! characters.
//!
//! Each chunk carries where it sits (`In: impl Rag`, `In: Install > macOS`)
//! and what it defines, so a chunk read alone, or embedded alone, still says
//! what it is.

use std::path::Path;

use super::Chunk;

/// Chunks are filled up to this many characters from small units.
pub const TARGET: usize = 1500;
/// A unit up to this size is kept whole; a larger one is split.
pub const MAX: usize = 4000;

/// A run of whole lines (0-based, inclusive) and what it is.
#[derive(Debug, Clone)]
struct Unit {
    start: usize,
    end: usize,
    scope: String,
    names: Vec<String>,
    /// A piece of one over-long line, cut on characters.
    piece: Option<String>,
}

impl Unit {
    fn new(start: usize, end: usize, scope: &str, names: Vec<String>) -> Self {
        Self {
            start,
            end,
            scope: scope.to_owned(),
            names,
            piece: None,
        }
    }
}

fn size(lines: &[&str], start: usize, end: usize) -> usize {
    lines[start..=end.min(lines.len() - 1)]
        .iter()
        .map(|l| l.chars().count() + 1)
        .sum()
}

/// `rel` cut into chunks; see the module comment.
pub fn chunk_file(rel: &str, text: &str) -> Vec<Chunk> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }
    let path = Path::new(rel);
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let units = match crate::syntax::language(path) {
        Some((language, _)) => code_units(&language, text, &lines),
        None if matches!(ext.as_str(), "md" | "mdx" | "markdown") => markdown_units(&lines),
        None => None,
    }
    .unwrap_or_else(|| paragraph_units(&lines, 0, lines.len() - 1, ""));
    let units = fit(units, &lines);
    merge(rel, units, &lines)
}

// ----- code -------------------------------------------------------------

fn is_comment(kind: &str) -> bool {
    kind.contains("comment")
}

fn is_attribute(kind: &str) -> bool {
    matches!(
        kind,
        "attribute_item" | "inner_attribute_item" | "decorator"
    )
}

/// A wrapper around the declaration that matters: `export class …`, a
/// decorated Python definition.
fn unwrap(node: tree_sitter::Node) -> tree_sitter::Node {
    for field in ["declaration", "definition"] {
        if matches!(node.kind(), "export_statement" | "decorated_definition") {
            if let Some(inner) = node.child_by_field_name(field) {
                return inner;
            }
        }
    }
    node
}

/// The node holding a unit's parts, when it has some: a class's methods, a
/// function's statements, an object's keys.
fn body(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let node = unwrap(node);
    for field in ["body", "value"] {
        if let Some(body) = node.child_by_field_name(field) {
            return Some(body);
        }
    }
    matches!(
        node.kind(),
        "object" | "array" | "block" | "declaration_list" | "class_body" | "statement_block"
    )
    .then_some(node)
}

/// What a unit defines: its name, or its first declarator's.
fn name(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let node = unwrap(node);
    let named = node
        .child_by_field_name("name")
        .or_else(|| node.child_by_field_name("key"))
        // `impl Foo`, `impl Trait for Foo`: the type.
        .or_else(|| node.child_by_field_name("type"))
        .or_else(|| {
            let mut cursor = node.walk();
            let declarator = node
                .named_children(&mut cursor)
                .find(|c| c.kind().ends_with("declarator") || c.kind() == "type_spec");
            declarator.and_then(|d| d.child_by_field_name("name"))
        })?;
    let text = named.utf8_text(source).ok()?.trim().trim_matches('"');
    (!text.is_empty() && text.len() <= 80).then(|| text.to_owned())
}

/// The first line of a unit, as the scope its parts sit in: `impl Rag`,
/// `class Cart(Base):`, `"scripts":`.
fn header(lines: &[&str], row: usize) -> String {
    let line = lines[row].trim().trim_end_matches('{').trim_end();
    let mut out: String = line.chars().take(100).collect();
    if line.chars().count() > 100 {
        out.push('…');
    } else if out.ends_with('(') {
        // A signature over several lines.
        out.push_str("…)");
    }
    out
}

fn join_scope(outer: &str, inner: &str) -> String {
    match (outer.is_empty(), inner.is_empty()) {
        (true, _) => inner.to_owned(),
        (_, true) => outer.to_owned(),
        _ => format!("{outer} > {inner}"),
    }
}

/// The last row a node covers, not counting a row it only ends at the
/// start of.
fn last_row(node: tree_sitter::Node) -> usize {
    let end = node.end_position();
    if end.column == 0 && end.row > node.start_position().row {
        end.row - 1
    } else {
        end.row
    }
}

fn code_units(language: &tree_sitter::Language, text: &str, lines: &[&str]) -> Option<Vec<Unit>> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(language).ok()?;
    let tree = parser.parse(text, None)?;
    let units = items(tree.root_node(), "", text.as_bytes(), lines, 0);
    (!units.is_empty()).then_some(units)
}

/// The units among `node`'s children, in order. A comment or attribute
/// goes with the unit after it.
fn items(
    node: tree_sitter::Node,
    scope: &str,
    source: &[u8],
    lines: &[&str],
    depth: usize,
) -> Vec<Unit> {
    let mut out: Vec<Unit> = Vec::new();
    let mut pending: Option<usize> = None;
    let mut cursor = node.walk();
    let children: Vec<_> = node.named_children(&mut cursor).collect();
    for child in children {
        let row = child.start_position().row;
        if is_comment(child.kind()) || is_attribute(child.kind()) {
            pending.get_or_insert(row);
            continue;
        }
        let start = pending.take().unwrap_or(row).min(row);
        let end = last_row(child).max(start).min(lines.len() - 1);
        let names: Vec<String> = name(child, source).into_iter().collect();
        if size(lines, start, end) > MAX && depth < 6 {
            if let Some(inner_node) = body(child) {
                let inner_scope = join_scope(scope, &header(lines, child.start_position().row));
                let mut inner = items(inner_node, &inner_scope, source, lines, depth + 1);
                if !inner.is_empty() {
                    // The signature and the closing line belong to the
                    // first and last part; nothing is left between.
                    let first = &mut inner[0];
                    first.start = start;
                    if first.names.is_empty() {
                        first.names = names.clone();
                    }
                    let last = inner.len() - 1;
                    inner[last].end = end;
                    out.extend(inner);
                    continue;
                }
            }
        }
        // Kept whole: what it holds is named too (a class's methods, an
        // impl's functions), so a search by any of them finds it.
        let mut names = names;
        if let Some(inner_node) = body(child).filter(|b| b.id() != child.id()) {
            let mut cursor = inner_node.walk();
            for member in inner_node.named_children(&mut cursor) {
                if names.len() >= 12 {
                    break;
                }
                if let Some(member) = name(member, source) {
                    if !names.contains(&member) {
                        names.push(member);
                    }
                }
            }
        }
        out.push(Unit::new(start, end, scope, names));
    }
    // Comments after the last unit.
    if let Some(start) = pending {
        let end = last_row(node).min(lines.len() - 1).max(start);
        out.push(Unit::new(start, end, scope, Vec::new()));
    }
    out
}

// ----- prose and everything else ----------------------------------------

/// Markdown by its headings, each section scoped by the headings above it.
/// A `#` inside a fenced block is code, not a heading.
fn markdown_units(lines: &[&str]) -> Option<Vec<Unit>> {
    let mut units = Vec::new();
    let mut path: Vec<(usize, String)> = Vec::new();
    let mut fenced = false;
    let mut start = 0;
    let mut scope = String::new();
    for (row, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        let level = trimmed.chars().take_while(|c| *c == '#').count();
        let heading = !fenced
            && (1..=6).contains(&level)
            && trimmed[level..].starts_with(' ')
            && line.len() - trimmed.len() < 4;
        if heading {
            if row > start {
                units.push(Unit::new(start, row - 1, &scope, Vec::new()));
            }
            path.retain(|(l, _)| *l < level);
            path.push((level, trimmed[level..].trim().to_owned()));
            scope = path
                .iter()
                .map(|(_, t)| t.as_str())
                .collect::<Vec<_>>()
                .join(" > ");
            start = row;
        }
    }
    units.push(Unit::new(start, lines.len() - 1, &scope, Vec::new()));
    units.retain(|u| lines[u.start..=u.end].iter().any(|l| !l.trim().is_empty()));
    Some(units)
}

/// Paragraphs: runs of lines between blank lines.
fn paragraph_units(lines: &[&str], from: usize, to: usize, scope: &str) -> Vec<Unit> {
    let mut units = Vec::new();
    let mut start: Option<usize> = None;
    for (row, line) in lines.iter().enumerate().take(to + 1).skip(from) {
        if line.trim().is_empty() {
            if let Some(s) = start.take() {
                units.push(Unit::new(s, row - 1, scope, Vec::new()));
            }
        } else if start.is_none() {
            start = Some(row);
        }
    }
    if let Some(s) = start {
        units.push(Unit::new(s, to, scope, Vec::new()));
    }
    units
}

// ----- sizing -----------------------------------------------------------

/// Every unit no larger than `MAX`: a larger one is cut into runs of lines
/// of about `TARGET`, at a blank line when one is near, and a single line
/// larger than `MAX` into pieces of characters.
fn fit(units: Vec<Unit>, lines: &[&str]) -> Vec<Unit> {
    let mut out = Vec::new();
    for unit in units {
        if size(lines, unit.start, unit.end) <= MAX {
            out.push(unit);
            continue;
        }
        let mut start = unit.start;
        let mut row = unit.start;
        let mut filled = 0;
        let mut last_blank: Option<usize> = None;
        let push = |out: &mut Vec<Unit>, start: usize, end: usize| {
            out.push(Unit::new(start, end, &unit.scope, unit.names.clone()));
        };
        while row <= unit.end {
            let line = lines[row];
            let width = line.chars().count() + 1;
            if width > MAX {
                if row > start {
                    push(&mut out, start, row - 1);
                }
                let chars: Vec<char> = line.chars().collect();
                for part in chars.chunks(TARGET) {
                    let mut piece = Unit::new(row, row, &unit.scope, unit.names.clone());
                    piece.piece = Some(part.iter().collect());
                    out.push(piece);
                }
                row += 1;
                start = row;
                filled = 0;
                last_blank = None;
                continue;
            }
            if filled + width > TARGET && row > start {
                // Back to the last blank line when it keeps most of the run.
                let cut = match last_blank {
                    Some(blank) if blank > start && size(lines, start, blank) * 2 > TARGET => blank,
                    _ => row - 1,
                };
                push(&mut out, start, cut);
                start = cut + 1;
                row = start;
                filled = 0;
                last_blank = None;
                continue;
            }
            if line.trim().is_empty() {
                last_blank = Some(row);
            }
            filled += width;
            row += 1;
        }
        if start <= unit.end {
            push(&mut out, start, unit.end);
        }
    }
    out
}

/// Neighbouring units of one scope, together up to `TARGET`, as chunks.
fn merge(rel: &str, units: Vec<Unit>, lines: &[&str]) -> Vec<Chunk> {
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut group: Vec<Unit> = Vec::new();
    let mut last_end: Option<usize> = None;
    let mut ids = std::collections::HashMap::<String, usize>::new();
    let mut flush = |group: &mut Vec<Unit>, chunks: &mut Vec<Chunk>| {
        let Some(first) = group.first() else { return };
        let start = first.start;
        let end = group.last().map(|u| u.end).unwrap_or(start);
        let body = match &first.piece {
            Some(piece) => format!("{piece}\n"),
            None => lines[start..=end]
                .iter()
                .map(|l| format!("{l}\n"))
                .collect::<String>(),
        };
        let mut names: Vec<String> = Vec::new();
        for unit in group.iter() {
            for name in &unit.names {
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
        }
        let chunk = Chunk::new(rel, start + 1, end + 1, &first.scope, names, body);
        // The id follows the content, so an edit elsewhere in the file
        // leaves this chunk's embedding alone.
        let seen = ids.entry(chunk.hash.clone()).or_insert(0);
        let id = if *seen == 0 {
            format!("{rel}#{}", &chunk.hash[..12])
        } else {
            format!("{rel}#{}~{seen}", &chunk.hash[..12])
        };
        *seen += 1;
        chunks.push(Chunk { id, ..chunk });
        group.clear();
    };
    for mut unit in units {
        // Never the same line twice.
        if unit.piece.is_none() {
            if let Some(last) = last_end {
                if unit.start <= last {
                    unit.start = last + 1;
                }
            }
            if unit.start > unit.end {
                continue;
            }
        }
        let unit_size = match &unit.piece {
            Some(p) => p.chars().count(),
            None => size(lines, unit.start, unit.end),
        };
        let group_size = match (group.first(), group.last()) {
            (Some(first), Some(last)) => size(lines, first.start, last.end),
            _ => 0,
        };
        let joins = !group.is_empty()
            && unit.piece.is_none()
            && group[0].piece.is_none()
            && group[0].scope == unit.scope
            && group_size + unit_size <= TARGET;
        if !joins {
            flush(&mut group, &mut chunks);
        }
        if unit.piece.is_none() {
            last_end = Some(unit.end);
        }
        let piece = unit.piece.is_some();
        group.push(unit);
        if piece {
            flush(&mut group, &mut chunks);
        }
    }
    flush(&mut group, &mut chunks);
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rust_fn(name: &str, body_lines: usize) -> String {
        let mut text = format!("/// Doc for {name}.\n#[inline]\nfn {name}() {{\n");
        for n in 0..body_lines {
            text.push_str(&format!(
                "    let value_{n} = compute_something_long({n});\n"
            ));
        }
        text.push_str("}\n\n");
        text
    }

    /// No line lost, none repeated, in order (trailing blank lines aside).
    fn covers_in_order(chunks: &[Chunk], lines: usize) {
        let mut next = 1;
        for chunk in chunks {
            assert!(chunk.start >= next, "{} starts before {next}", chunk.start);
            next = chunk.end + 1;
        }
        assert!(
            next > lines,
            "the last chunk ends at {} of {lines}",
            next - 1
        );
    }

    #[test]
    fn a_function_is_never_cut_and_keeps_its_comments() {
        let mut text = String::new();
        for name in ["alpha", "beta", "gamma", "delta"] {
            text.push_str(&rust_fn(name, 25));
        }
        let chunks = chunk_file("src/lib.rs", &text);
        for chunk in &chunks {
            let opens = chunk.body.matches('{').count();
            let closes = chunk.body.matches('}').count();
            assert_eq!(
                opens, closes,
                "a chunk holds whole functions:\n{}",
                chunk.body
            );
            if let Some(fn_line) = chunk.body.lines().position(|l| l.starts_with("fn ")) {
                assert!(fn_line >= 2, "doc and attribute stay above: {}", chunk.body);
            }
        }
        let names: Vec<&String> = chunks.iter().flat_map(|c| &c.names).collect();
        assert_eq!(names, ["alpha", "beta", "gamma", "delta"]);
        covers_in_order(&chunks, text.trim_end().lines().count());
    }

    #[test]
    fn small_items_are_merged_up_to_the_target() {
        let text: String = (0..60)
            .map(|n| format!("const C{n}: u32 = {n};\n"))
            .collect();
        let chunks = chunk_file("src/consts.rs", &text);
        assert_eq!(chunks.len(), 1, "{chunks:#?}");
        assert_eq!(chunks[0].names.len(), 60);
    }

    #[test]
    fn a_large_impl_is_cut_between_its_methods_and_says_where() {
        let mut text = String::from("impl Store {\n");
        for n in 0..6 {
            text.push_str(&format!("    fn method_{n}(&self) {{\n"));
            for m in 0..30 {
                text.push_str(&format!("        self.do_the_work_number({m}, {n});\n"));
            }
            text.push_str("    }\n\n");
        }
        text.push_str("}\n");
        let chunks = chunk_file("src/store.rs", &text);
        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert_eq!(chunk.scope, "impl Store", "{chunk:#?}");
            assert!(chunk.text().contains("In: impl Store"));
            assert!(chunk.body.chars().count() <= MAX + 1);
        }
        assert!(chunks[0].body.starts_with("impl Store {"));
        assert!(chunks.last().unwrap().body.trim_end().ends_with('}'));
        covers_in_order(&chunks, text.trim_end().lines().count());
    }

    #[test]
    fn a_huge_function_is_cut_between_statements() {
        let text = rust_fn("huge", 300);
        let chunks = chunk_file("src/huge.rs", &text);
        assert!(chunks.len() > 2);
        for chunk in &chunks {
            assert!(chunk.scope.contains("fn huge"), "{}", chunk.scope);
            assert!(chunk.body.chars().count() <= MAX + 1);
            for line in chunk.body.lines() {
                assert!(text.contains(line), "whole lines only");
            }
        }
        covers_in_order(&chunks, text.trim_end().lines().count());
    }

    #[test]
    fn markdown_is_cut_at_headings_and_keeps_their_path() {
        let text = "# Guide\n\nIntro.\n\n## Install\n\nRun it.\n\n```sh\n# not a heading\n```\n\n### macOS\n\nbrew it.\n";
        let chunks = chunk_file("docs/guide.md", text);
        let scopes: Vec<&str> = chunks.iter().map(|c| c.scope.as_str()).collect();
        assert_eq!(
            scopes,
            ["Guide", "Guide > Install", "Guide > Install > macOS"]
        );
        assert!(chunks[1].body.contains("# not a heading"));
    }

    #[test]
    fn plain_text_is_cut_at_paragraphs() {
        let paragraph = "word ".repeat(60) + "\n";
        let text: String = (0..12)
            .map(|_| format!("{paragraph}{paragraph}\n"))
            .collect();
        let chunks = chunk_file("notes.txt", &text);
        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(
                chunk.body.trim_end().ends_with("word"),
                "ends on a paragraph"
            );
        }
    }

    #[test]
    fn a_long_line_is_the_only_thing_cut_on_characters() {
        let line = "é".repeat(MAX * 2);
        let chunks = chunk_file("data.txt", &format!("{line}\n"));
        assert!(chunks.len() >= 2);
        assert!(chunks.iter().all(|c| c.start == 1 && c.end == 1));
        let joined: String = chunks
            .iter()
            .map(|c| c.body.trim_end_matches('\n'))
            .collect();
        assert_eq!(joined, line);
    }

    /// An edit above a function moves it but leaves its id and hash, so it
    /// is not embedded again.
    #[test]
    fn code_that_only_moved_keeps_its_id() {
        let before = format!("{}{}", rust_fn("first", 40), rust_fn("second", 40));
        let after = format!("// a new comment\n\n{before}");
        let a = chunk_file("src/m.rs", &before);
        let b = chunk_file("src/m.rs", &after);
        let second_a = a
            .iter()
            .find(|c| c.names.contains(&"second".to_owned()))
            .unwrap();
        let second_b = b
            .iter()
            .find(|c| c.names.contains(&"second".to_owned()))
            .unwrap();
        assert_eq!(second_a.id, second_b.id);
        assert_eq!(second_a.hash, second_b.hash);
        assert_ne!(second_a.start, second_b.start);
    }

    #[test]
    fn the_embedded_text_says_what_and_where() {
        let chunks = chunk_file("src/a.rs", &rust_fn("checkout", 3));
        let text = chunks[0].text();
        assert!(
            text.starts_with("File: src/a.rs\nDefines: checkout\n\n"),
            "{text}"
        );
        assert!(
            !text.contains("lines"),
            "no line numbers in what is embedded"
        );
    }

    #[test]
    fn json_is_cut_by_its_keys() {
        let mut text = String::from("{\n");
        for n in 0..40 {
            text.push_str(&format!(
                "  \"key_{n}\": {{ \"description\": \"{}\" }},\n",
                "x".repeat(150)
            ));
        }
        text.push_str("  \"last\": 1\n}\n");
        let chunks = chunk_file("data/config.json", &text);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|c| c.body.chars().count() <= MAX + 1));
        assert!(chunks.iter().any(|c| c.names.contains(&"key_0".to_owned())));
        covers_in_order(&chunks, text.trim_end().lines().count());
    }
}
