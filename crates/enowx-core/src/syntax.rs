//! Whether code still parses after an edit, and mending it when an edit
//! broke it.
//!
//! A model editing by text often drops a brace, repeats a line or cuts an
//! expression in half. The file is parsed before and after the change (with
//! tree-sitter, in process, no language server needed); when it parsed
//! before and does not after, the edit is what broke it. A small model is
//! then shown the region as it was and as it became, and asked for the region
//! with the intended change and valid syntax. When that fails too, the edit
//! is refused with the error's place, so the agent fixes it with the file
//! still whole.

use std::path::Path;

use async_trait::async_trait;

/// The grammar for a file, by its extension.
pub(crate) fn language(path: &Path) -> Option<(tree_sitter::Language, &'static str)> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "rs" => (tree_sitter_rust::LANGUAGE.into(), "Rust"),
        "ts" | "mts" | "cts" => (
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            "TypeScript",
        ),
        "tsx" => (tree_sitter_typescript::LANGUAGE_TSX.into(), "TSX"),
        "js" | "mjs" | "cjs" | "jsx" => (tree_sitter_javascript::LANGUAGE.into(), "JavaScript"),
        "py" | "pyi" => (tree_sitter_python::LANGUAGE.into(), "Python"),
        "go" => (tree_sitter_go::LANGUAGE.into(), "Go"),
        "json" => (tree_sitter_json::LANGUAGE.into(), "JSON"),
        "css" => (tree_sitter_css::LANGUAGE.into(), "CSS"),
        _ => return None,
    })
}

/// Where a file stops parsing, 1-based, and what the parser met there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Broken {
    pub line: usize,
    pub column: usize,
    pub what: String,
}

/// Whether `text` parses as the language of `path`: None when enx has no
/// grammar for it, `Some(None)` when it parses, `Some(Some(error))` when not.
pub fn check(path: &Path, text: &str) -> Option<Option<Broken>> {
    let (language, _) = language(path)?;
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).ok()?;
    let tree = parser.parse(text, None)?;
    let root = tree.root_node();
    if !root.has_error() {
        return Some(None);
    }
    // The first error or missing node in reading order.
    let mut cursor = root.walk();
    let mut found = None;
    'walk: loop {
        let node = cursor.node();
        if node.is_error() || node.is_missing() {
            found = Some(node);
            break;
        }
        if node.has_error() && cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                continue 'walk;
            }
            if !cursor.goto_parent() {
                break 'walk;
            }
        }
    }
    let node = found.unwrap_or(root);
    let at = node.start_position();
    let what = if node.is_missing() {
        format!("missing `{}`", node.kind())
    } else {
        let snippet: String = text[node.byte_range()]
            .chars()
            .take(30)
            .collect::<String>()
            .replace('\n', " ");
        format!("unexpected `{}`", snippet.trim())
    };
    Some(Some(Broken {
        line: at.row + 1,
        column: at.column + 1,
        what,
    }))
}

pub fn language_name(path: &Path) -> &'static str {
    language(path).map_or("code", |(_, name)| name)
}

/// Asks a model to mend a region an edit left unparsable.
#[async_trait]
pub trait Repair: Send + Sync {
    /// The region as `after` should have been: the change it makes, with
    /// valid syntax. `previous` is an earlier answer that still did not parse.
    async fn repair(
        &self,
        language: &str,
        before: &str,
        after: &str,
        previous: Option<&str>,
    ) -> Option<String>;
}

/// The lines two texts differ in, with `context` lines around: the
/// half-open line ranges in `old` and `new`.
fn changed_region(old: &str, new: &str, context: usize) -> (usize, usize, usize, usize) {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    let mut head = 0;
    while head < a.len() && head < b.len() && a[head] == b[head] {
        head += 1;
    }
    let mut tail = 0;
    while tail < a.len() - head
        && tail < b.len() - head
        && a[a.len() - 1 - tail] == b[b.len() - 1 - tail]
    {
        tail += 1;
    }
    let start = head.saturating_sub(context);
    (
        start,
        (a.len() - tail + context).min(a.len()),
        start,
        (b.len() - tail + context).min(b.len()),
    )
}

fn lines(text: &str, from: usize, to: usize) -> String {
    text.lines()
        .skip(from)
        .take(to.saturating_sub(from))
        .collect::<Vec<_>>()
        .join("\n")
}

fn splice(text: &str, from: usize, to: usize, region: &str) -> String {
    let all: Vec<&str> = text.lines().collect();
    let mut out: Vec<&str> = all[..from.min(all.len())].to_vec();
    out.extend(region.lines());
    out.extend(&all[to.min(all.len())..]);
    let mut joined = out.join("\n");
    if text.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

/// What became of a change whose result was checked.
#[derive(Debug)]
pub enum Outcome {
    /// It parses, or enx cannot tell for this language, or it did not parse
    /// before either.
    Fine,
    /// It broke the syntax and was mended: the new text, and the region as
    /// written.
    Repaired {
        text: String,
        region: String,
        at: usize,
    },
    /// It broke the syntax and could not be mended.
    Broken(Broken),
}

/// Check `new` against `old` (None for a new file) and mend it when the
/// change broke it and `repair` is there.
pub async fn guard(
    path: &Path,
    old: Option<&str>,
    new: &str,
    repair: Option<&dyn Repair>,
) -> Outcome {
    let Some(after) = check(path, new) else {
        return Outcome::Fine;
    };
    let Some(broken) = after else {
        return Outcome::Fine;
    };
    // A file that did not parse before is not this change's doing.
    if let Some(old) = old {
        if matches!(check(path, old), Some(Some(_))) {
            return Outcome::Fine;
        }
    }
    let Some(repair) = repair else {
        return Outcome::Broken(broken);
    };
    let old = old.unwrap_or("");
    let (a_from, a_to, b_from, b_to) = changed_region(old, new, 3);
    let before = lines(old, a_from, a_to);
    let after_region = lines(new, b_from, b_to);
    let mut previous: Option<String> = None;
    for _ in 0..2 {
        let Some(answer) = repair
            .repair(
                language_name(path),
                &before,
                &after_region,
                previous.as_deref(),
            )
            .await
        else {
            break;
        };
        let answer = strip_fence(&answer);
        let mended = splice(new, b_from, b_to, &answer);
        if matches!(check(path, &mended), Some(None)) {
            return Outcome::Repaired {
                text: mended,
                region: answer,
                at: b_from + 1,
            };
        }
        previous = Some(answer);
    }
    Outcome::Broken(broken)
}

fn strip_fence(answer: &str) -> String {
    let trimmed = answer.trim_matches('\n');
    let mut lines: Vec<&str> = trimmed.lines().collect();
    if lines
        .first()
        .is_some_and(|l| l.trim_start().starts_with("```"))
    {
        lines.remove(0);
        if lines
            .last()
            .is_some_and(|l| l.trim_start().starts_with("```"))
        {
            lines.pop();
        }
    }
    lines.join("\n")
}

/// The refusal an unmendable edit gets: where it broke, with the lines
/// around it numbered.
pub fn refusal(path_shown: &str, new: &str, broken: &Broken) -> String {
    let from = broken.line.saturating_sub(3);
    let near: Vec<String> = new
        .lines()
        .enumerate()
        .skip(from)
        .take(5)
        .map(|(i, l)| format!("{:>5}:{l}", i + 1))
        .collect();
    format!(
        "Not changed: after this edit {path_shown} no longer parses ({} at line {}, column {}):\n{}\n\
         Nothing was written. Send the edit again with the syntax whole: balanced brackets, \
         complete statements, no repeated or cut lines.",
        broken.what,
        broken.line,
        broken.column,
        near.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed(&'static str);

    #[async_trait]
    impl Repair for Fixed {
        async fn repair(&self, _: &str, _: &str, _: &str, _: Option<&str>) -> Option<String> {
            Some(self.0.to_owned())
        }
    }

    #[test]
    fn a_dropped_brace_is_found_where_it_breaks() {
        let good = "fn main() {\n    let x = 1;\n}\n";
        assert_eq!(check(Path::new("a.rs"), good), Some(None));
        let bad = "fn main() {\n    let x = 1;\n";
        let broken = check(Path::new("a.rs"), bad).unwrap().unwrap();
        assert!(broken.what.contains("missing") || broken.what.contains("unexpected"));
        assert!(check(Path::new("notes.md"), "{").is_none(), "no grammar");
        for (path, text) in [
            ("a.ts", "const a: number = 1;\n"),
            (
                "a.tsx",
                "export const A = () => <div className=\"x\">hi</div>;\n",
            ),
            ("a.jsx", "const A = () => <p>{1}</p>;\n"),
            ("a.py", "def f(x):\n    return x\n"),
            ("a.go", "package main\n\nfunc main() {}\n"),
            ("a.json", "{\"a\": [1, 2]}\n"),
            ("a.css", "a { color: red; }\n"),
        ] {
            assert_eq!(check(Path::new(path), text), Some(None), "{path}");
        }
        assert!(matches!(
            check(Path::new("a.json"), "{\"a\": [1, 2}"),
            Some(Some(_))
        ));
    }

    #[tokio::test]
    async fn a_broken_edit_is_mended_or_refused() {
        let old = "fn main() {\n    let x = 1;\n    println!(\"{x}\");\n}\n";
        let new = "fn main() {\n    let x = 2;\n    println!(\"{x}\";\n}\n";
        let path = Path::new("src/main.rs");
        assert!(matches!(
            guard(path, Some(old), new, None).await,
            Outcome::Broken(_)
        ));
        let fixed = Fixed("fn main() {\n    let x = 2;\n    println!(\"{x}\");\n}");
        match guard(path, Some(old), new, Some(&fixed)).await {
            Outcome::Repaired { text, .. } => {
                assert!(text.contains("let x = 2;") && text.contains("println!(\"{x}\");"));
                assert!(text.ends_with('\n'));
            }
            other => panic!("expected a repair, got {other:?}"),
        }
        // A file already broken before is not this edit's doing.
        let already = "fn main( {\n";
        assert!(matches!(
            guard(path, Some(already), "fn main( {\n// note\n", None).await,
            Outcome::Fine
        ));
        let refusal = refusal("src/main.rs", new, &check(path, new).unwrap().unwrap());
        assert!(refusal.starts_with("Not changed"), "{refusal}");
    }
}
