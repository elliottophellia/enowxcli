//! Lightweight syntax highlighting for fenced code blocks.
//!
//! This is deliberately NOT a parser. A coding agent shows short excerpts —
//! a diff hunk, a function, a shell command — and a full grammar engine
//! (syntect + onig) would add a C dependency and megabytes of syntax
//! definitions to colour twenty lines. Instead we tokenize the handful of
//! lexical features that carry almost all of the visual signal: comments,
//! strings, numbers, and keywords.
//!
//! The tokenizer is single-pass and line-oriented, with one piece of state
//! (`in_block_comment`) carried across lines so `/* … */` survives a wrap.
//! Anything it does not recognise falls through as plain text, so an
//! unsupported language degrades to today's flat rendering rather than
//! breaking.

use crate::theme::Theme;
use ratatui::style::Style;

/// What a run of characters means. Colour is resolved from the theme at emit
/// time so `/theme` restyles code blocks along with everything else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tok {
    Plain,
    Comment,
    Str,
    Num,
    Keyword,
    /// Type-ish identifier: `UpperCamelCase`, or a known primitive.
    Type,
    /// Function name at a call or definition site (`name(`).
    Func,
}

impl Tok {
    pub fn style(self, theme: &Theme) -> Style {
        let fg = match self {
            Tok::Plain => theme.text,
            Tok::Comment => theme.faint,
            Tok::Str => theme.green,
            Tok::Num => theme.accent2,
            Tok::Keyword => theme.accent,
            Tok::Type => theme.yellow,
            Tok::Func => theme.accent2,
        };
        let base = Style::default().fg(fg);
        if self == Tok::Comment {
            base.add_modifier(ratatui::style::Modifier::ITALIC)
        } else {
            base
        }
    }
}

/// The comment and string conventions of one language family. Grouping by
/// family rather than by language keeps the table small: Rust, Go, C, Java,
/// JS and TS all lex the same way for our purposes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Syntax {
    line_comment: &'static [&'static str],
    block_comment: Option<(&'static str, &'static str)>,
    /// Quote characters that open a string. A backslash escapes the next
    /// character inside one.
    quotes: &'static [char],
    keywords: &'static [&'static str],
    /// Treat `UpperCamelCase` words as types. True for most curly-brace
    /// languages; false for shell, where capitals are usually variables.
    camel_types: bool,
}

const C_LIKE_KEYWORDS: &[&str] = &[
    "as",
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "default",
    "defer",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "extern",
    "false",
    "final",
    "finally",
    "fn",
    "for",
    "from",
    "func",
    "function",
    "go",
    "goto",
    "if",
    "impl",
    "implements",
    "import",
    "in",
    "instanceof",
    "interface",
    "let",
    "loop",
    "match",
    "mod",
    "move",
    "mut",
    "new",
    "nil",
    "null",
    "package",
    "priv",
    "pub",
    "public",
    "private",
    "protected",
    "ref",
    "return",
    "self",
    "static",
    "struct",
    "super",
    "switch",
    "this",
    "throw",
    "trait",
    "true",
    "try",
    "type",
    "typeof",
    "union",
    "unsafe",
    "use",
    "var",
    "void",
    "where",
    "while",
    "with",
    "yield",
];

const PY_KEYWORDS: &[&str] = &[
    "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif",
    "else", "except", "False", "finally", "for", "from", "global", "if", "import", "in", "is",
    "lambda", "None", "nonlocal", "not", "or", "pass", "raise", "return", "True", "try", "while",
    "with", "yield",
];

const SHELL_KEYWORDS: &[&str] = &[
    "case", "do", "done", "elif", "else", "esac", "fi", "for", "function", "if", "in", "local",
    "return", "select", "then", "until", "while", "export", "source", "alias", "set", "unset",
];

const SQL_KEYWORDS: &[&str] = &[
    "ALTER", "AND", "AS", "ASC", "BY", "CREATE", "DELETE", "DESC", "DISTINCT", "DROP", "EXISTS",
    "FROM", "GROUP", "HAVING", "IN", "INDEX", "INNER", "INSERT", "INTO", "JOIN", "LEFT", "LIMIT",
    "NOT", "NULL", "OFFSET", "ON", "OR", "ORDER", "OUTER", "SELECT", "SET", "TABLE", "UNION",
    "UPDATE", "VALUES", "WHERE", "WITH",
];

const C_LIKE: Syntax = Syntax {
    line_comment: &["//"],
    block_comment: Some(("/*", "*/")),
    quotes: &['"', '\'', '`'],
    keywords: C_LIKE_KEYWORDS,
    camel_types: true,
};

const PYTHON: Syntax = Syntax {
    line_comment: &["#"],
    block_comment: None,
    quotes: &['"', '\''],
    keywords: PY_KEYWORDS,
    camel_types: true,
};

const SHELL: Syntax = Syntax {
    line_comment: &["#"],
    block_comment: None,
    quotes: &['"', '\''],
    keywords: SHELL_KEYWORDS,
    camel_types: false,
};

const SQL: Syntax = Syntax {
    line_comment: &["--"],
    block_comment: Some(("/*", "*/")),
    quotes: &['"', '\''],
    keywords: SQL_KEYWORDS,
    camel_types: false,
};

/// Data formats: no keywords, but strings and numbers still carry meaning.
const DATA: Syntax = Syntax {
    line_comment: &["#"],
    block_comment: None,
    quotes: &['"', '\''],
    keywords: &["true", "false", "null"],
    camel_types: false,
};

/// Map a fence's language tag to a syntax family. Returns None for unknown
/// tags so the caller renders the block plain rather than guessing wrong.
pub fn lookup(lang: &str) -> Option<Syntax> {
    let lang = lang.trim().to_ascii_lowercase();
    // A fence can carry more than the language (```rust,no_run or ```js title=x).
    let lang = lang.split([',', ' ', ':']).next().unwrap_or("");
    Some(match lang {
        "rust" | "rs" | "go" | "golang" | "c" | "h" | "cpp" | "cc" | "hpp" | "c++" | "java"
        | "kt" | "kotlin" | "swift" | "scala" | "cs" | "csharp" | "js" | "jsx" | "mjs" | "cjs"
        | "javascript" | "ts" | "tsx" | "typescript" | "php" | "dart" | "zig" => C_LIKE,
        "py" | "python" | "rb" | "ruby" | "lua" | "r" | "perl" | "pl" => PYTHON,
        "sh" | "bash" | "zsh" | "shell" | "console" | "fish" | "dockerfile" | "makefile"
        | "make" => SHELL,
        "sql" | "postgres" | "postgresql" | "mysql" | "sqlite" => SQL,
        "json" | "json5" | "yaml" | "yml" | "toml" | "ini" | "conf" | "hcl" | "tf" => DATA,
        _ => return None,
    })
}

/// Carried between lines so a `/* … */` opened on one line keeps colouring the
/// next. Callers create one per fenced block and reuse it down the block.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct State {
    in_block_comment: bool,
}

/// Split one line into `(text, token)` runs. Runs are contiguous and in order,
/// and concatenating them reproduces the input exactly — callers rely on that
/// to keep wrapping and copy-to-clipboard lossless.
pub fn highlight(line: &str, syntax: &Syntax, state: &mut State) -> Vec<(String, Tok)> {
    let chars: Vec<char> = line.chars().collect();
    let mut out: Vec<(String, Tok)> = Vec::new();
    let mut buf = String::new();
    let mut i = 0;

    // Flush the plain-text accumulator, classifying whole words as we go so a
    // keyword only matches on a word boundary (`info` must not match `in`).
    macro_rules! flush {
        () => {
            if !buf.is_empty() {
                classify_words(&buf, syntax, &mut out);
                buf.clear();
            }
        };
    }

    while i < chars.len() {
        // Continuation of a multi-line block comment.
        if state.in_block_comment {
            let (close_open, close) = match syntax.block_comment {
                Some(pair) => pair,
                None => {
                    state.in_block_comment = false;
                    continue;
                }
            };
            let _ = close_open;
            let rest: String = chars[i..].iter().collect();
            if let Some(pos) = rest.find(close) {
                let end = i + rest[..pos].chars().count() + close.chars().count();
                let body: String = chars[i..end].iter().collect();
                out.push((body, Tok::Comment));
                state.in_block_comment = false;
                i = end;
            } else {
                let body: String = chars[i..].iter().collect();
                out.push((body, Tok::Comment));
                i = chars.len();
            }
            continue;
        }

        let rest: String = chars[i..].iter().collect();

        // Line comment runs to end of line.
        if let Some(marker) = syntax.line_comment.iter().find(|m| rest.starts_with(**m)) {
            let _ = marker;
            flush!();
            out.push((rest, Tok::Comment));
            break;
        }

        // Block comment opens.
        if let Some((open, close)) = syntax.block_comment {
            if let Some(after_open) = rest.strip_prefix(open) {
                flush!();
                if let Some(pos) = after_open.find(close) {
                    let take = open.chars().count()
                        + after_open[..pos].chars().count()
                        + close.chars().count();
                    let body: String = chars[i..i + take].iter().collect();
                    out.push((body, Tok::Comment));
                    i += take;
                } else {
                    out.push((rest, Tok::Comment));
                    state.in_block_comment = true;
                    i = chars.len();
                }
                continue;
            }
        }

        let c = chars[i];

        // String literal. A backslash escapes the next char, so `"a\"b"` stays
        // one run. An unterminated quote colours to end of line rather than
        // bleeding into the following lines.
        if syntax.quotes.contains(&c) {
            flush!();
            let mut j = i + 1;
            let mut escaped = false;
            while j < chars.len() {
                if escaped {
                    escaped = false;
                } else if chars[j] == '\\' {
                    escaped = true;
                } else if chars[j] == c {
                    j += 1;
                    break;
                }
                j += 1;
            }
            let body: String = chars[i..j.min(chars.len())].iter().collect();
            out.push((body, Tok::Str));
            i = j.min(chars.len());
            continue;
        }

        // Number: a digit that does not continue an identifier. `utf8` stays
        // one plain word; `0x1f`, `3.14`, `1_000` are numbers.
        if c.is_ascii_digit() && !buf.chars().last().is_some_and(is_ident_char) {
            flush!();
            let mut j = i;
            while j < chars.len()
                && (chars[j].is_ascii_alphanumeric() || chars[j] == '.' || chars[j] == '_')
            {
                j += 1;
            }
            let body: String = chars[i..j].iter().collect();
            out.push((body, Tok::Num));
            i = j;
            continue;
        }

        buf.push(c);
        i += 1;
    }
    flush!();
    merge_adjacent(out)
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Split a plain run into words and classify each. Non-word characters pass
/// through untouched so punctuation and spacing are preserved exactly.
fn classify_words(buf: &str, syntax: &Syntax, out: &mut Vec<(String, Tok)>) {
    let chars: Vec<char> = buf.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if is_ident_char(chars[i]) {
            let start = i;
            while i < chars.len() && is_ident_char(chars[i]) {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            // `name(` is a call or definition; colour the identifier.
            let is_call = chars.get(i) == Some(&'(');
            let tok = if syntax.keywords.contains(&word.as_str()) {
                Tok::Keyword
            } else if is_call {
                Tok::Func
            } else if syntax.camel_types && is_camel_type(&word) {
                Tok::Type
            } else {
                Tok::Plain
            };
            out.push((word, tok));
        } else {
            let start = i;
            while i < chars.len() && !is_ident_char(chars[i]) {
                i += 1;
            }
            out.push((chars[start..i].iter().collect(), Tok::Plain));
        }
    }
}

/// `UpperCamelCase` — a leading capital plus at least one lowercase letter, so
/// `HTTP` and `MAX` (constants) do not get type colouring.
fn is_camel_type(word: &str) -> bool {
    let mut chars = word.chars();
    match chars.next() {
        Some(c) if c.is_uppercase() => {}
        _ => return false,
    }
    word.chars().any(|c| c.is_lowercase())
}

/// Collapse neighbouring runs that share a token so the emitted span list is
/// as short as possible — ratatui allocates per span.
fn merge_adjacent(runs: Vec<(String, Tok)>) -> Vec<(String, Tok)> {
    let mut out: Vec<(String, Tok)> = Vec::with_capacity(runs.len());
    for (text, tok) in runs {
        if text.is_empty() {
            continue;
        }
        match out.last_mut() {
            Some((prev, prev_tok)) if *prev_tok == tok => prev.push_str(&text),
            _ => out.push((text, tok)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(line: &str, lang: &str) -> Vec<(String, Tok)> {
        let syntax = lookup(lang).expect("known language");
        let mut state = State::default();
        highlight(line, &syntax, &mut state)
    }

    /// The contract every caller depends on: runs concatenate back to the
    /// input. If this breaks, wrapping and copy silently lose characters.
    fn assert_lossless(line: &str, lang: &str) {
        let joined: String = toks(line, lang).into_iter().map(|(t, _)| t).collect();
        assert_eq!(joined, line, "runs must reproduce the input exactly");
    }

    #[test]
    fn lossless_across_languages() {
        for (line, lang) in [
            ("let x = \"hi\"; // done", "rust"),
            ("def f(a, b):  # add", "python"),
            ("echo \"$HOME\" # path", "bash"),
            ("SELECT * FROM t WHERE a = 1;", "sql"),
            ("{\"a\": 1, \"b\": null}", "json"),
            ("", "rust"),
            ("   ", "rust"),
            ("λ = 3 // unicode ident", "rust"),
        ] {
            assert_lossless(line, lang);
        }
    }

    /// Find the token covering the first occurrence of `needle`. Runs of the
    /// same token are merged before emit, so a lookup by exact run text would
    /// miss words that got joined to their neighbouring whitespace.
    fn tok_covering(runs: &[(String, Tok)], needle: &str) -> Tok {
        let mut offset = 0usize;
        let joined: String = runs.iter().map(|(t, _)| t.as_str()).collect();
        let at = joined.find(needle).expect("needle present in line");
        for (text, tok) in runs {
            let next = offset + text.len();
            if at >= offset && at < next {
                return *tok;
            }
            offset = next;
        }
        unreachable!("offset walk must cover the joined string")
    }

    #[test]
    fn keywords_only_match_whole_words() {
        let got = toks("information in x", "rust");
        // `information` contains "in" as a substring; that must not promote
        // the whole word to a keyword.
        assert_eq!(tok_covering(&got, "information"), Tok::Plain);
        // The standalone `in` is a real keyword. Anchor past the preceding
        // space, which belongs to the plain run before it.
        let at_in = got
            .iter()
            .map(|(t, _)| t.as_str())
            .collect::<String>()
            .rfind("in ")
            .expect("standalone `in` present");
        let _ = at_in;
        assert!(
            got.iter().any(|(t, k)| t == "in" && *k == Tok::Keyword),
            "standalone `in` must be a keyword run of its own"
        );
    }

    #[test]
    fn strings_consume_escapes() {
        let got = toks(r#"let s = "a\"b"; let t = 1;"#, "rust");
        let s = got.iter().find(|(_, k)| *k == Tok::Str).unwrap();
        assert_eq!(s.0, r#""a\"b""#, "escaped quote must not end the string");
    }

    #[test]
    fn unterminated_string_stops_at_end_of_line() {
        let got = toks("let s = \"oops", "rust");
        assert_eq!(got.last().unwrap().1, Tok::Str);
        assert_lossless("let s = \"oops", "rust");
    }

    #[test]
    fn block_comment_spans_lines() {
        let syntax = lookup("rust").unwrap();
        let mut state = State::default();
        let first = highlight("code(); /* open", &syntax, &mut state);
        assert!(state.in_block_comment, "state must carry to the next line");
        assert_eq!(first.last().unwrap().1, Tok::Comment);
        let second = highlight("still comment", &syntax, &mut state);
        assert!(second.iter().all(|(_, k)| *k == Tok::Comment));
        let third = highlight("done */ after();", &syntax, &mut state);
        assert!(!state.in_block_comment, "closing fence must clear state");
        assert!(third.iter().any(|(_, k)| *k != Tok::Comment));
    }

    #[test]
    fn numbers_do_not_split_identifiers() {
        let got = toks("let utf8 = 0x1f;", "rust");
        // The `8` in `utf8` must not break the identifier into `utf` + number.
        assert_eq!(
            tok_covering(&got, "utf8"),
            Tok::Plain,
            "utf8 is one identifier, not `utf` + 8"
        );
        assert!(got.iter().any(|(t, k)| t == "0x1f" && *k == Tok::Num));
    }

    #[test]
    fn shell_does_not_colour_capitals_as_types() {
        let got = toks("echo $HOME", "bash");
        assert!(got.iter().all(|(_, k)| *k != Tok::Type));
    }

    #[test]
    fn unknown_language_is_not_guessed() {
        assert!(lookup("brainfuck").is_none());
        assert!(lookup("").is_none());
    }

    #[test]
    fn fence_info_string_is_tolerated() {
        // ```rust,no_run and ```ts title=x both name a real language.
        assert!(lookup("rust,no_run").is_some());
        assert!(lookup("ts title=x").is_some());
    }
}
