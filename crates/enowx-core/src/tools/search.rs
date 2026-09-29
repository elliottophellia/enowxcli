use super::{display_path, number_arg, resolve_existing, string_arg, Tool, ToolCtx, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use ignore::WalkBuilder;
use regex::Regex;
use serde_json::{json, Value};
use std::path::Path;

pub(super) struct GlobTool;

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }
    fn description(&self) -> &str {
        "List workspace paths matching a glob pattern, respecting .gitignore."
    }
    fn parameters(&self) -> Value {
        json!({"type":"object","properties":{
            "pattern":{"type":"string","description":"Glob such as src/**/*.rs"},
            "limit":{"type":"integer","minimum":1,"maximum":1000}
        },"required":["pattern"],"additionalProperties":false})
    }
    async fn execute(&self, ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        let pattern = string_arg(&args, "pattern")?;
        let limit = number_arg(&args, "limit", 200, 1000);
        let matcher = globset::Glob::new(pattern)?.compile_matcher();
        let mut out = Vec::new();
        for entry in WalkBuilder::new(&ctx.workspace)
            .hidden(false)
            .git_ignore(true)
            // A folder that is not a git repository still has its .gitignore
            // honoured: without this, a fresh Next.js project searched all of
            // node_modules.
            .require_git(false)
            .filter_entry(|entry| !is_never_searched(entry.file_name()))
            .build()
            .flatten()
        {
            let path = entry.path();
            if path == ctx.workspace {
                continue;
            }
            let Ok(relative) = path.strip_prefix(&ctx.workspace) else {
                continue;
            };
            if matcher.is_match(relative) {
                let mut shown = relative.to_string_lossy().into_owned();
                if entry.file_type().is_some_and(|kind| kind.is_dir()) {
                    shown.push('/');
                }
                out.push(shown);
                if out.len() >= limit {
                    break;
                }
            }
        }
        out.sort();
        if out.is_empty() {
            return Ok(ToolOutput::ok("No matches."));
        }
        Ok(ToolOutput::ok(out.join("\n")))
    }
}

pub(super) struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }
    fn description(&self) -> &str {
        "Search UTF-8 workspace files with a Rust regular expression. Returns file, line, and matching text."
    }
    fn parameters(&self) -> Value {
        json!({"type":"object","properties":{
            "pattern":{"type":"string"},"path":{"type":"string","description":"File or directory, default workspace root"},
            "case_sensitive":{"type":"boolean"},"limit":{"type":"integer","minimum":1,"maximum":1000}
        },"required":["pattern"],"additionalProperties":false})
    }
    async fn execute(&self, ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        let pattern = string_arg(&args, "pattern")?;
        let case = args
            .get("case_sensitive")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let regex = regex::RegexBuilder::new(pattern)
            .case_insensitive(!case)
            .build()?;
        let raw = args.get("path").and_then(Value::as_str).unwrap_or(".");
        let root = resolve_existing(&ctx.workspace, raw)?;
        let limit = number_arg(&args, "limit", 200, 1000);
        let mut hits = Vec::new();
        if root.is_file() {
            scan_file(&ctx.workspace, &root, &regex, limit, &mut hits);
        } else {
            for entry in WalkBuilder::new(root)
                .hidden(false)
                .git_ignore(true)
                // A folder that is not a git repository still has its .gitignore
                // honoured: without this, a fresh Next.js project searched all of
                // node_modules.
                .require_git(false)
                .filter_entry(|entry| !is_never_searched(entry.file_name()))
                .build()
                .flatten()
            {
                if entry.file_type().is_some_and(|kind| kind.is_file()) {
                    scan_file(&ctx.workspace, entry.path(), &regex, limit, &mut hits);
                }
                if hits.len() >= limit {
                    break;
                }
            }
        }
        if hits.is_empty() {
            return Ok(ToolOutput::ok("No matches."));
        }
        Ok(ToolOutput::ok(hits.join("\n")))
    }
}

/// Directories no search walks into: installed dependencies and git's own
/// store. They are never the code being asked about, and one `node_modules`
/// holds more text than any context window.
fn is_never_searched(name: &std::ffi::OsStr) -> bool {
    matches!(name.to_str(), Some("node_modules" | ".git"))
}

/// Files past this size are generated or data, not code to read by line.
const MAX_SEARCHED_BYTES: u64 = 2 * 1024 * 1024;

/// The most of one matching line a hit shows. A source map or a minified
/// bundle is one line of hundreds of kilobytes: 200 such hits once made a
/// request of 4.7 million tokens.
const MAX_HIT_CHARS: usize = 240;

fn scan_file(root: &Path, path: &Path, regex: &Regex, limit: usize, hits: &mut Vec<String>) {
    if hits.len() >= limit {
        return;
    }
    if std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_SEARCHED_BYTES) {
        return;
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    for (index, line) in text.lines().enumerate() {
        if let Some(found) = regex.find(line) {
            hits.push(format!(
                "{}:{}:{}",
                display_path(root, path),
                index + 1,
                excerpt(line, found.start())
            ));
            if hits.len() >= limit {
                break;
            }
        }
    }
}

/// The part of a long line around a match, marked where it was cut.
fn excerpt(line: &str, at: usize) -> String {
    let total = line.chars().count();
    if total <= MAX_HIT_CHARS {
        return line.to_owned();
    }
    let at = line[..at].chars().count();
    let start = at.saturating_sub(MAX_HIT_CHARS / 4);
    let body: String = line.chars().skip(start).take(MAX_HIT_CHARS).collect();
    let before = if start > 0 { "…" } else { "" };
    let after = if start + MAX_HIT_CHARS < total {
        "…"
    } else {
        ""
    };
    format!("{before}{body}{after}")
}

#[cfg(test)]
mod excerpt_tests {
    use super::*;

    #[test]
    fn a_long_line_is_cut_around_its_match() {
        let line = format!("{}Shell{}", "a".repeat(10_000), "b".repeat(10_000));
        let cut = excerpt(&line, line.find("Shell").unwrap());
        assert!(cut.contains("Shell"));
        assert!(cut.chars().count() <= MAX_HIT_CHARS + 2);
        assert!(cut.starts_with('…') && cut.ends_with('…'));
        assert_eq!(excerpt("short line", 0), "short line");
    }

    /// The case that made a 4.7-million-token request: a project that is not
    /// a git repository, with source maps in node_modules.
    #[tokio::test]
    async fn a_search_stays_out_of_node_modules_and_long_lines() {
        let root = std::env::temp_dir().join(format!("enx-grep-{}", uuid::Uuid::new_v4()));
        let deps = root.join("node_modules/next/dist");
        std::fs::create_dir_all(&deps).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        let huge = format!(
            "{{\"sourcesContent\":\"{}Shell{}\"}}",
            "x".repeat(300_000),
            "y".repeat(300_000)
        );
        std::fs::write(deps.join("render.js.map"), &huge).unwrap();
        std::fs::write(
            root.join("src/app.ts"),
            format!("const a = 1;\n{huge}\nconst Shell = 2;\n"),
        )
        .unwrap();
        let ctx = crate::tools::ToolCtx {
            workspace: root.clone(),
            shell_timeout: std::time::Duration::from_secs(5),
            cancel: tokio_util::sync::CancellationToken::new(),
            progress: None,
            call_id: String::new(),
            skills: Vec::new(),
            lsp: None,
            repair: None,
            vision: false,
        };
        let out = GrepTool
            .execute(&ctx, serde_json::json!({ "pattern": "Shell" }))
            .await
            .unwrap();
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            !out.content.contains("node_modules"),
            "{}",
            &out.content[..200.min(out.content.len())]
        );
        assert!(out.content.contains("src/app.ts:3:const Shell = 2;"));
        assert!(
            out.content.len() < 1_000,
            "long lines are cut: {} bytes",
            out.content.len()
        );
    }

    #[test]
    fn dependencies_are_never_searched() {
        assert!(is_never_searched(std::ffi::OsStr::new("node_modules")));
        assert!(is_never_searched(std::ffi::OsStr::new(".git")));
        assert!(!is_never_searched(std::ffi::OsStr::new("src")));
    }
}
