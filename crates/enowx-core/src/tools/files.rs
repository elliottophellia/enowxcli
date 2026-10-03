use super::{
    display_path, number_arg, resolve_existing, resolve_for_write, string_arg, Tool, ToolCtx,
    ToolOutput,
};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::Path;

pub(super) struct ReadTool;

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str {
        "read"
    }
    fn description(&self) -> &str {
        "Read a UTF-8 file. Each line comes as `12#a3f:text`: its number and an anchor that \
         `edit_lines` uses to name it. Use offset and limit for large files. An image (png, \
         jpg, gif, webp) is shown to you when your model can see images, including the \
         screenshots `preview` saves."
    }
    fn parameters(&self) -> Value {
        json!({"type":"object","properties":{
            "path":{"type":"string","description":"Path relative to the workspace"},
            "offset":{"type":"integer","minimum":1,"description":"First line, 1-based"},
            "limit":{"type":"integer","minimum":1,"maximum":1000,"description":"Maximum lines"}
        },"required":["path"],"additionalProperties":false})
    }
    async fn execute(&self, ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        let raw = string_arg(&args, "path")?;
        if let Some(mime) = image_type(Path::new(raw)) {
            return Ok(read_image(ctx, raw, mime));
        }
        let path = resolve_existing(&ctx.workspace, raw)?;
        let offset = number_arg(&args, "offset", 1, usize::MAX).max(1);
        let limit = number_arg(&args, "limit", 240, 1000);
        let meta = std::fs::metadata(&path)?;
        if meta.is_dir() {
            let mut entries = Vec::new();
            for entry in std::fs::read_dir(&path)? {
                let entry = entry?;
                let mut name = entry.file_name().to_string_lossy().into_owned();
                if entry.file_type()?.is_dir() {
                    name.push('/');
                }
                entries.push(name);
            }
            entries.sort();
            return Ok(ToolOutput::ok(entries.join("\n")));
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {} as UTF-8", path.display()))?;
        let lines: Vec<&str> = text.lines().collect();
        let start = offset.saturating_sub(1).min(lines.len());
        let end = (start + limit).min(lines.len());
        let width = end.max(1).to_string().len();
        let mut out = String::new();
        for (index, line) in lines[start..end].iter().enumerate() {
            out.push_str(&format!(
                "{:>width$}#{}:{}\n",
                start + index + 1,
                anchor(line),
                line,
                width = width
            ));
        }
        if end < lines.len() {
            out.push_str(&format!(
                "[Showing lines {}-{} of {}. Use offset={} to continue.]",
                start + 1,
                end,
                lines.len(),
                end + 1
            ));
        }
        Ok(ToolOutput::ok(out.trim_end().to_string()))
    }
}

pub(super) struct WriteTool;

#[async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &str {
        "write"
    }
    fn description(&self) -> &str {
        "Create or overwrite one UTF-8 file inside the workspace. Keep each call to about \
         150 lines: a longer reply takes minutes to generate and the provider can cut it \
         off, and then nothing is written at all. Write a longer file in parts: `write` its \
         first part ending in a marker comment (`<!-- next -->`, `// next`), then replace \
         the marker with the next part and a new marker using `edit`, one part per step."
    }
    fn parameters(&self) -> Value {
        json!({"type":"object","properties":{
            "path":{"type":"string"},"content":{"type":"string"}
        },"required":["path","content"],"additionalProperties":false})
    }
    async fn execute(&self, ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        let raw = string_arg(&args, "path")?;
        let content = string_arg(&args, "content")?;
        // Empty content is almost always a truncated tool call, not a file the
        // user wants blanked: say so plainly so the model does not read "0
        // lines" as success and lose the content it meant to write.
        if content.is_empty() {
            return Ok(ToolOutput::error(format!(
                "no content: `write` was called with an empty `content` for {raw}. If the file \
                 is large, the call was likely cut off. Write it in parts: create it with the \
                 first section, then add each following section with `edit`. Do not send the \
                 whole file in one call."
            )));
        }
        let path = resolve_for_write(&ctx.workspace, raw)?;
        // Kept so the result can say what changed, and the interface show it.
        let before = std::fs::read_to_string(&path).ok();
        let reminders = match rules_on(ctx, &path, before.as_deref().unwrap_or(""), content) {
            Ok(reminders) => reminders,
            Err(refusal) => return Ok(refusal),
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Emit the content line-by-line so the UI can render a typewriter-
        // style preview while the file is being staged. Actual disk write is
        // atomic at the end so a crash mid-stream cannot corrupt the file.
        if let Some(tx) = &ctx.progress {
            let mut sent = 0usize;
            for line in content.split_inclusive('\n') {
                if ctx.cancel.is_cancelled() {
                    break;
                }
                let _ = tx.send((ctx.call_id.clone(), line.to_string())).await;
                sent += 1;
                // Throttle so the UI has time to render each chunk without
                // dropping frames. 8 ms per line keeps a 1000-line file under
                // 8 s; the actual disk write below happens instantly.
                if sent.is_multiple_of(4) {
                    tokio::time::sleep(std::time::Duration::from_millis(8)).await;
                }
            }
        }
        let (content, mended) =
            match syntax_on(ctx, &path, before.as_deref(), content.to_owned(), true).await {
                Ok(checked) => checked,
                Err(refusal) => return Ok(refusal),
            };
        let content = content.as_str();
        crate::config::atomic_write(&path, content.as_bytes())?;
        // Format the freshly written file when the language has a known
        // formatter. If the binary is absent, report it so the UI can offer
        // to install it; the write itself still succeeds either way.
        if let Some(fmt) = crate::format::formatter_for(&path) {
            if !crate::format::is_available(fmt.bin) {
                // Note the missing formatter into the tool result. The event
                // channel is one-way (tool → runtime → UI) via `progress`;
                // we reuse it with a sentinel prefix the UI can parse.
                if let Some(tx) = &ctx.progress {
                    let _ = tx
                        .send((
                            ctx.call_id.clone(),
                            format!(
                                "\n\x1f__ENX_FMT_MISSING__ {} {} {} {}\n",
                                fmt.language,
                                fmt.bin,
                                fmt.install_hint,
                                fmt.install_cmd.join(" "),
                            ),
                        ))
                        .await;
                }
            } else {
                let _ = crate::format::format_file(&fmt, &path).await;
            }
        }
        let shown = display_path(&ctx.workspace, &path);
        let lines = content.lines().count();
        let mut output = match &before {
            None => ToolOutput::ok(format!("Created {shown} ({lines} lines)")),
            Some(old) => {
                let (added, removed) = line_changes(old, content);
                ToolOutput::ok(format!(
                    "Replaced {shown} ({lines} lines; +{added} -{removed})"
                ))
            }
        };
        // A file too large to diff on screen is not worth carrying there.
        output.before = before.filter(|old| old.len() <= 512 * 1024);
        output.content.push_str(&reminders);
        output.content.push_str(&mended);
        Ok(with_diagnostics(ctx, &path, output).await)
    }
}

/// Lines `new` has that `old` did not, and the reverse, counted as multisets:
/// exact for additions and removals, and linear where a real diff of two
/// large files would be quadratic.
fn line_changes(old: &str, new: &str) -> (usize, usize) {
    let mut count: std::collections::HashMap<&str, isize> = std::collections::HashMap::new();
    for line in old.lines() {
        *count.entry(line).or_default() -= 1;
    }
    for line in new.lines() {
        *count.entry(line).or_default() += 1;
    }
    let added = count
        .values()
        .filter(|n| **n > 0)
        .map(|n| *n as usize)
        .sum();
    let removed = count
        .values()
        .filter(|n| **n < 0)
        .map(|n| (-*n) as usize)
        .sum();
    (added, removed)
}

/// Several replacements in one file, in one call. Each call re-sends the
/// whole context to the model, and a page restyled one `edit` at a time took
/// twenty-one of them.
pub(super) struct MultiEditTool;

#[async_trait]
impl Tool for MultiEditTool {
    fn name(&self) -> &str {
        "multi_edit"
    }
    fn description(&self) -> &str {
        "Make several replacements in one UTF-8 file in one call: each edit is an \
         exact, unique text block and its replacement, applied in order. Use it \
         instead of several `edit` calls on the same file. All or nothing: if any \
         edit fails, the file is left unchanged and the failing edit is named."
    }
    fn parameters(&self) -> Value {
        json!({"type":"object","properties":{
            "path":{"type":"string"},
            "edits":{"type":"array","minItems":1,"items":{"type":"object","properties":{
                "old_text":{"type":"string"},"new_text":{"type":"string"}
            },"required":["old_text","new_text"]}}
        },"required":["path","edits"],"additionalProperties":false})
    }
    async fn execute(&self, ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        let raw = string_arg(&args, "path")?;
        let edits = args
            .get("edits")
            .and_then(Value::as_array)
            .filter(|edits| !edits.is_empty())
            .ok_or_else(|| anyhow::anyhow!("edits must be a non-empty list"))?;
        let path = resolve_existing(&ctx.workspace, raw)?;
        let mut text = std::fs::read_to_string(&path)?;
        let mut lines = Vec::new();
        for (index, edit) in edits.iter().enumerate() {
            let n = index + 1;
            let old = edit
                .get("old_text")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow::anyhow!("edit {n} has no old_text"))?;
            let new = edit
                .get("new_text")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow::anyhow!("edit {n} has no new_text"))?;
            if old.is_empty() {
                anyhow::bail!("edit {n}: old_text must not be empty; nothing was changed");
            }
            match text.matches(old).count() {
                0 => {
                    anyhow::bail!("edit {n}: old_text was not found in {raw}; nothing was changed")
                }
                1 => {}
                count => anyhow::bail!(
                    "edit {n}: old_text matched {count} places in {raw}; include more \
                     context. Nothing was changed"
                ),
            }
            let at = text.find(old).unwrap_or(0);
            lines.push(text[..at].bytes().filter(|b| *b == b'\n').count() + 1);
            text = text.replacen(old, new, 1);
        }
        let original = std::fs::read_to_string(&path)?;
        let reminders = match rules_on(ctx, &path, &original, &text) {
            Ok(reminders) => reminders,
            Err(refusal) => return Ok(refusal),
        };
        let (text, mended) = match syntax_on(ctx, &path, Some(&original), text, false).await {
            Ok(checked) => checked,
            Err(refusal) => return Ok(refusal),
        };
        crate::config::atomic_write(&path, text.as_bytes())?;
        let at: Vec<String> = lines.iter().map(ToString::to_string).collect();
        let output = ToolOutput::ok(format!(
            "Updated {raw}: {} edits, at lines {}{reminders}{mended}",
            lines.len(),
            at.join(", ")
        ));
        Ok(with_diagnostics(ctx, &path, output).await)
    }
}

pub(super) struct EditTool;

#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &str {
        "edit"
    }
    fn description(&self) -> &str {
        "Replace one exact, unique text block in a UTF-8 file. Fails if old_text is absent or ambiguous."
    }
    fn parameters(&self) -> Value {
        json!({"type":"object","properties":{
            "path":{"type":"string"},"old_text":{"type":"string"},"new_text":{"type":"string"}
        },"required":["path","old_text","new_text"],"additionalProperties":false})
    }
    async fn execute(&self, ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        let raw = string_arg(&args, "path")?;
        let old = string_arg(&args, "old_text")?;
        let new = string_arg(&args, "new_text")?;
        if old.is_empty() {
            anyhow::bail!("old_text must not be empty");
        }
        let path = resolve_existing(&ctx.workspace, raw)?;
        let source = std::fs::read_to_string(&path)?;
        let matches = source.matches(old).count();
        if matches == 0 {
            anyhow::bail!("old_text was not found in {raw}");
        }
        if matches > 1 {
            anyhow::bail!("old_text matched {matches} places in {raw}; include more context");
        }
        // 1-indexed line where the replacement begins, so the UI can show
        // real file line numbers in the diff view instead of `1..`.
        let start_byte = source.find(old).unwrap_or(0);
        let start_line = source[..start_byte].bytes().filter(|b| *b == b'\n').count() + 1;
        let updated = source.replacen(old, new, 1);
        let reminders = match rules_on(ctx, &path, &source, &updated) {
            Ok(reminders) => reminders,
            Err(refusal) => return Ok(refusal),
        };
        let (updated, mended) = match syntax_on(ctx, &path, Some(&source), updated, false).await {
            Ok(checked) => checked,
            Err(refusal) => return Ok(refusal),
        };
        crate::config::atomic_write(&path, updated.as_bytes())?;
        let output = ToolOutput::ok(format!(
            "Updated {raw} at line {start_line}{reminders}{mended}"
        ));
        Ok(with_diagnostics(ctx, &path, output).await)
    }
}

/// The line that starts what the language servers said, after the tool's own
/// result. The interface splits on it.
pub const DIAGNOSTICS_HEADING: &str = "\n\nDiagnostics\n";

/// Add what the language servers say about the file just written, when
/// checking is on and a server covers the file.
async fn with_diagnostics(ctx: &ToolCtx, path: &Path, mut output: ToolOutput) -> ToolOutput {
    let Some(lsp) = &ctx.lsp else {
        return output;
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return output;
    };
    let report = tokio::select! {
        report = lsp.check(path, &text, crate::lsp::Wait::Edit) => report,
        _ = ctx.cancel.cancelled() => None,
    };
    if let Some(report) = report {
        output.content.push_str(DIAGNOSTICS_HEADING);
        output.content.push_str(&report);
    }
    output
}

/// Type errors and lint warnings for one file, from its language server,
/// waiting until the server has finished checking.
pub(super) struct DiagnosticsTool;

#[async_trait]
impl Tool for DiagnosticsTool {
    fn name(&self) -> &str {
        "diagnostics"
    }
    fn description(&self) -> &str {
        "Type errors and lint warnings for a file from the project's language server \
         (rust-analyzer with clippy, typescript-language-server, pyright, ruff, gopls). \
         write and edit already add what the server says first; call this when it was still \
         checking, or before changing a file you suspect is broken. Waits until the check \
         finishes."
    }
    fn parameters(&self) -> Value {
        json!({"type":"object","properties":{
            "path":{"type":"string","description":"The file to check"}
        },"required":["path"],"additionalProperties":false})
    }
    async fn execute(&self, ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        let raw = string_arg(&args, "path")?;
        let path = resolve_existing(&ctx.workspace, raw)?;
        let Some(lsp) = &ctx.lsp else {
            return Ok(ToolOutput::error(
                "language server checks are off (agent.lsp = false in the config)",
            ));
        };
        let text = std::fs::read_to_string(&path)?;
        let report = tokio::select! {
            report = lsp.check(&path, &text, crate::lsp::Wait::Full) => report,
            _ = ctx.cancel.cancelled() => return Ok(ToolOutput::error("cancelled")),
        };
        Ok(match report {
            Some(report) => ToolOutput::ok(report),
            None => ToolOutput::ok(format!(
                "No language server covers {raw}: checking runs for Rust, TypeScript and \
                 JavaScript, Python and Go."
            )),
        })
    }
}

/// Where each planning document lives, relative to the workspace. `DESIGN.md`
/// stays at the root, where the interface agents read it before every change.
pub fn plan_doc_path(doc: &str, feature: Option<&str>) -> Option<String> {
    let name = match doc.trim().to_ascii_lowercase().as_str() {
        "design" => return Some("DESIGN.md".to_owned()),
        "prd" => "PRD",
        "architecture" => "ARCHITECTURE",
        "erd" => "ERD",
        "api" => "API",
        "plan" => "PLAN",
        _ => return None,
    };
    let feature: String = feature
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    Some(if feature.is_empty() {
        format!("docs/plan/{name}.md")
    } else {
        format!("docs/plan/{name}-{feature}.md")
    })
}

/// The orchestrator's one writing tool: the planning documents and nothing
/// else, so it can write the plan without being able to edit code.
pub(super) struct PlanWriteTool;

#[async_trait]
impl Tool for PlanWriteTool {
    fn name(&self) -> &str {
        "plan_write"
    }
    fn description(&self) -> &str {
        "Write a planning document the specialists will read: `prd` (docs/plan/PRD.md), \
         `design` (DESIGN.md at the root), `architecture`, `erd`, `api` or `plan` \
         (docs/plan/*.md). `feature` names a document for one feature in an existing \
         project (docs/plan/PRD-invoices.md). Replaces the file: read it first and keep what \
         still holds. The `brainstorm` skill's parts hold each document's template."
    }
    fn parameters(&self) -> Value {
        json!({"type":"object","properties":{
            "doc":{"type":"string","enum":["prd","design","architecture","erd","api","plan"]},
            "content":{"type":"string","description":"The whole document, in Markdown"},
            "feature":{"type":"string","description":"Optional: a feature name for a per-feature document"}
        },"required":["doc","content"],"additionalProperties":false})
    }
    async fn execute(&self, ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        let doc = string_arg(&args, "doc")?;
        let content = string_arg(&args, "content")?;
        let feature = args.get("feature").and_then(Value::as_str);
        let Some(relative) = plan_doc_path(doc, feature) else {
            return Ok(ToolOutput::error(format!(
                "`{doc}` is not a planning document: use prd, design, architecture, erd, api or plan"
            )));
        };
        let path = resolve_for_write(&ctx.workspace, &relative)?;
        let before = std::fs::read_to_string(&path).ok();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        crate::config::atomic_write(&path, content.as_bytes())?;
        let lines = content.lines().count();
        let mut output = ToolOutput::ok(match &before {
            None => format!("Created {relative} ({lines} lines)"),
            Some(old) => {
                let (added, removed) = line_changes(old, content);
                format!("Replaced {relative} ({lines} lines; +{added} -{removed})")
            }
        });
        output.before = before;
        Ok(output)
    }
}

/// The rules on the text a call adds to `path`: `Err` with the refusal when
/// one blocks it, else the reminders to add to the result (empty when none).
fn rules_on(ctx: &ToolCtx, path: &Path, old: &str, new: &str) -> Result<String, ToolOutput> {
    let rules = crate::rules::load(&ctx.workspace);
    let relative = path.strip_prefix(&ctx.workspace).unwrap_or(path);
    let verdict = crate::rules::check(&rules, relative, &crate::rules::added_lines(old, new));
    if !verdict.blocked.is_empty() {
        return Err(ToolOutput::error(crate::rules::refusal(&verdict.blocked)));
    }
    Ok(if verdict.reminders.is_empty() {
        String::new()
    } else {
        crate::rules::reminder(&verdict.reminders)
    })
}

/// Where a changed file's syntax stands: `Err` with the refusal when an edit
/// broke it beyond mending, else the text to write and a note when it was
/// mended. `whole` is a write of the whole file: written even when broken,
/// with the error said, since refusing would make the model send it all again.
async fn syntax_on(
    ctx: &ToolCtx,
    path: &Path,
    old: Option<&str>,
    new: String,
    whole: bool,
) -> Result<(String, String), ToolOutput> {
    let shown = display_path(&ctx.workspace, path);
    match crate::syntax::guard(path, old, &new, ctx.repair.as_deref()).await {
        crate::syntax::Outcome::Fine => Ok((new, String::new())),
        crate::syntax::Outcome::Repaired { text, region, at } => Ok((
            text,
            format!(
                "\n\nThe change left {shown} unparsable, so its region was mended from line {at}; \
                 the file now holds:\n{region}\nRead it before editing that region again."
            ),
        )),
        crate::syntax::Outcome::Broken(broken) if whole => Ok((
            new,
            format!(
                "\n\nSyntax: {shown} does not parse ({} at line {}, column {}). Fix it next.",
                broken.what, broken.line, broken.column
            ),
        )),
        crate::syntax::Outcome::Broken(broken) => Err(ToolOutput::error(crate::syntax::refusal(
            &shown, &new, &broken,
        ))),
    }
}

/// What the language server knows about code: where a symbol is defined,
/// every place it is used, its type and docs, what a file declares, and a
/// rename that updates every reference.
pub(super) struct LspTool;

#[async_trait]
impl Tool for LspTool {
    fn name(&self) -> &str {
        "lsp"
    }
    fn description(&self) -> &str {
        "Ask the project's language server about code instead of searching text. `definition` \
         (where a symbol is defined), `references` (every use), `hover` (its type and docs), \
         `symbols` (what a file declares, with lines), `rename` (renames it in every file, with \
         `new_name`). Name the place by `path`, `line` (1-based) and `symbol` (the name on that \
         line) or `column`. Rust, TypeScript and JavaScript, Python and Go."
    }
    fn parameters(&self) -> Value {
        json!({"type":"object","properties":{
            "op":{"type":"string","enum":["definition","references","hover","symbols","rename"]},
            "path":{"type":"string"},
            "line":{"type":"integer","minimum":1},
            "symbol":{"type":"string","description":"The symbol's name as written on that line"},
            "column":{"type":"integer","minimum":1},
            "new_name":{"type":"string","description":"For rename"}
        },"required":["op","path"],"additionalProperties":false})
    }
    async fn execute(&self, ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        let Some(ask) = crate::lsp::Ask::parse(string_arg(&args, "op")?) else {
            return Ok(ToolOutput::error(
                "op is one of definition, references, hover, symbols, rename",
            ));
        };
        let raw = string_arg(&args, "path")?;
        let path = resolve_existing(&ctx.workspace, raw)?;
        let Some(lsp) = &ctx.lsp else {
            return Ok(ToolOutput::error(
                "language servers are off (agent.lsp = false in the config)",
            ));
        };
        let text = std::fs::read_to_string(&path)?;
        let at = args["line"].as_u64().and_then(|line| {
            crate::lsp::position(
                &text,
                line as usize,
                args["column"].as_u64().map(|c| c as usize),
                args["symbol"].as_str(),
            )
        });
        if ask != crate::lsp::Ask::Symbols && at.is_none() {
            return Ok(ToolOutput::error(
                "give `line` and the `symbol` on it (or `column`); the symbol must be written on that line",
            ));
        }
        let answer = tokio::select! {
            answer = lsp.ask(&path, ask, at, args["new_name"].as_str()) => answer,
            _ = ctx.cancel.cancelled() => return Ok(ToolOutput::error("cancelled")),
        };
        Ok(match answer {
            Ok(text) => ToolOutput::ok(text),
            Err(error) => ToolOutput::error(format!("{error:#}")),
        })
    }
}

/// A line's anchor: three hex digits of a hash of its text (trailing spaces
/// ignored). With the line number it names a line in `edit_lines`, and it
/// tells a stale edit (the file changed since it was read) from a good one.
pub fn anchor(line: &str) -> String {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in line.trim_end().bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    format!("{:03x}", hash & 0xfff)
}

/// Find the line an anchor `N#abc` names in `lines` (0-based). A line that
/// moved since the file was read is found near where it was, when its text
/// is there once.
fn locate(lines: &[&str], raw: &str) -> Result<usize, String> {
    let (number, tag) = raw
        .trim()
        .split_once('#')
        .ok_or_else(|| format!("`{raw}` is not an anchor: write it as `12#a3f`, from `read`"))?;
    let number: usize = number
        .trim()
        .parse()
        .map_err(|_| format!("`{raw}` has no line number"))?;
    let tag = tag.trim().trim_end_matches(':').to_ascii_lowercase();
    if number >= 1 && number <= lines.len() && anchor(lines[number - 1]) == tag {
        return Ok(number - 1);
    }
    let near: Vec<usize> = (number.saturating_sub(40)..(number + 40).min(lines.len() + 1))
        .filter(|n| *n >= 1 && anchor(lines[n - 1]) == tag)
        .map(|n| n - 1)
        .collect();
    if near.len() == 1 {
        return Ok(near[0]);
    }
    let now = lines
        .get(number.wrapping_sub(1))
        .map(|l| {
            format!(
                "line {number} is now `{}` ({number}#{})",
                l.trim(),
                anchor(l)
            )
        })
        .unwrap_or_else(|| format!("the file has {} lines", lines.len()));
    Err(format!(
        "`{raw}` no longer matches: {now}. Read the lines again and use their anchors."
    ))
}

/// Edits by line anchor: replace, insert or delete whole lines without
/// repeating the old text.
pub(super) struct EditLinesTool;

#[async_trait]
impl Tool for EditLinesTool {
    fn name(&self) -> &str {
        "edit_lines"
    }
    fn description(&self) -> &str {
        "Change whole lines of a file you have read, naming them by the anchors `read` shows \
         (`12#a3f`): no need to repeat the old text. Each edit: `op` replace (lines `from` to \
         `to`, inclusive; `to` omitted is one line), insert_after, insert_before or delete, and \
         `text` for the new lines (no anchors in it). All anchors refer to the file as read; \
         edits may not overlap. The result shows the changed lines with their new anchors."
    }
    fn parameters(&self) -> Value {
        json!({"type":"object","properties":{
            "path":{"type":"string"},
            "edits":{"type":"array","minItems":1,"items":{"type":"object","properties":{
                "op":{"type":"string","enum":["replace","insert_after","insert_before","delete"]},
                "from":{"type":"string","description":"Anchor of the first line, as `12#a3f`"},
                "to":{"type":"string","description":"Anchor of the last line, for replace and delete"},
                "text":{"type":"string","description":"The new lines, for replace and inserts"}
            },"required":["op","from"]}}
        },"required":["path","edits"],"additionalProperties":false})
    }
    async fn execute(&self, ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        let raw = string_arg(&args, "path")?;
        let path = resolve_existing(&ctx.workspace, raw)?;
        let source = std::fs::read_to_string(&path)?;
        let lines: Vec<&str> = source.lines().collect();
        let edits = args["edits"].as_array().cloned().unwrap_or_default();
        if edits.is_empty() {
            return Ok(ToolOutput::error("edits must be a non-empty list"));
        }
        // Each edit as the half-open range of old lines it replaces and the
        // lines that take their place.
        let mut plans: Vec<(usize, usize, Vec<String>)> = Vec::new();
        for (index, edit) in edits.iter().enumerate() {
            let n = index + 1;
            let op = edit["op"].as_str().unwrap_or("replace");
            let from = match locate(&lines, edit["from"].as_str().unwrap_or("")) {
                Ok(line) => line,
                Err(error) => return Ok(ToolOutput::error(format!("edit {n}: {error}"))),
            };
            let to = match edit["to"].as_str() {
                Some(raw) if !raw.trim().is_empty() => match locate(&lines, raw) {
                    Ok(line) => line,
                    Err(error) => return Ok(ToolOutput::error(format!("edit {n}: {error}"))),
                },
                _ => from,
            };
            if to < from {
                return Ok(ToolOutput::error(format!(
                    "edit {n}: `to` comes before `from`"
                )));
            }
            let text: Vec<String> = edit["text"]
                .as_str()
                .unwrap_or("")
                .lines()
                .map(str::to_owned)
                .collect();
            let plan = match op {
                "replace" => (from, to + 1, text),
                "delete" => (from, to + 1, Vec::new()),
                "insert_after" => (from + 1, from + 1, text),
                "insert_before" => (from, from, text),
                other => return Ok(ToolOutput::error(format!("edit {n}: unknown op `{other}`"))),
            };
            plans.push(plan);
        }
        plans.sort_by_key(|p| (p.0, p.1));
        for pair in plans.windows(2) {
            if pair[1].0 < pair[0].1 {
                return Ok(ToolOutput::error(
                    "two edits touch the same lines: merge them into one",
                ));
            }
        }
        let mut result: Vec<String> = lines.iter().map(|l| (*l).to_owned()).collect();
        for (start, end, text) in plans.iter().rev() {
            result.splice(*start..*end, text.iter().cloned());
        }
        let mut updated = result.join("\n");
        if source.ends_with('\n') || source.is_empty() {
            updated.push('\n');
        }
        let reminders = match rules_on(ctx, &path, &source, &updated) {
            Ok(reminders) => reminders,
            Err(refusal) => return Ok(refusal),
        };
        let (updated, mended) = match syntax_on(ctx, &path, Some(&source), updated, false).await {
            Ok(checked) => checked,
            Err(refusal) => return Ok(refusal),
        };
        crate::config::atomic_write(&path, updated.as_bytes())?;
        // Each change as a small diff: a line of context on each side, the
        // lines removed (`-`), and the lines that took their place (`+`)
        // with their new anchors, so the next edit needs no new read and the
        // interface can show what changed. Spans were applied last first;
        // shift each by what the edits above it added or removed.
        let new_lines: Vec<&str> = updated.lines().collect();
        let mut shown = String::new();
        let mut delta: isize = 0;
        let mut ordered: Vec<(usize, usize, usize)> = plans
            .iter()
            .map(|(start, end, text)| (*start, end - start, text.len()))
            .collect();
        ordered.sort();
        let anchored = |i: usize| format!("{:>4}#{}:{}", i + 1, anchor(new_lines[i]), new_lines[i]);
        for (start, removed, added) in ordered {
            let at = (start as isize + delta).max(0) as usize;
            if at > 0 && at - 1 < new_lines.len() {
                shown.push_str(&format!(" {}\n", anchored(at - 1)));
            }
            for old in &lines[start..start + removed] {
                shown.push_str(&format!("-         {old}\n"));
            }
            for i in at..(at + added).min(new_lines.len()) {
                shown.push_str(&format!("+{}\n", anchored(i)));
            }
            if at + added < new_lines.len() {
                shown.push_str(&format!(" {}\n", anchored(at + added)));
            }
            shown.push_str("  …\n");
            delta += added as isize - removed as isize;
        }
        let output = ToolOutput::ok(format!(
            "Updated {raw}: {} edits (- removed; + added, with the new anchors)\n{}{reminders}{mended}",
            plans.len(),
            shown.trim_end()
        ));
        Ok(with_diagnostics(ctx, &path, output).await)
    }
}

/// The media type of a picture `read` can show, by its extension.
fn image_type(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

/// Whether `path` is a picture enx saved itself: `preview`'s screenshots,
/// in a folder of the temp directory. Those may be read from outside the
/// workspace.
fn enx_picture(path: &Path) -> bool {
    let Ok(canonical) = path.canonicalize() else {
        return false;
    };
    let temp = std::env::temp_dir()
        .canonicalize()
        .unwrap_or_else(|_| std::env::temp_dir());
    canonical.starts_with(&temp)
        && canonical.components().any(|part| {
            part.as_os_str()
                .to_string_lossy()
                .starts_with("enx-preview-")
        })
}

/// A picture for the model to look at.
fn read_image(ctx: &ToolCtx, raw: &str, mime: &str) -> ToolOutput {
    use base64::Engine as _;
    const LIMIT: u64 = 8 * 1024 * 1024;
    if !ctx.vision {
        return ToolOutput::error(
            "The model in use cannot see images (the catalogue lists no image input for it). \
             Work from `preview`'s measurements instead, or set the model's `vision = true` in \
             config.toml when it can.",
        );
    }
    let candidate = Path::new(raw);
    let path = if candidate.is_absolute() && enx_picture(candidate) {
        candidate.to_path_buf()
    } else {
        match resolve_existing(&ctx.workspace, raw) {
            Ok(path) => path,
            Err(error) => return ToolOutput::error(format!("{error:#}")),
        }
    };
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    if size > LIMIT {
        return ToolOutput::error(format!(
            "{raw} is {} MB; images up to 8 MB can be shown",
            size / (1024 * 1024)
        ));
    }
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => return ToolOutput::error(format!("reading {raw}: {error}")),
    };
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| raw.to_owned());
    let mut output = ToolOutput::ok(format!(
        "{name} ({} KB) is attached below for you to look at.",
        bytes.len() / 1024
    ));
    output.images.push(crate::message::Attachment {
        name,
        data_url: format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        ),
    });
    output
}

/// An `edit` call in the shape `edit_lines` takes (line anchors in `from` or
/// `lines`, an `op`, `text`), as that tool's arguments. A model that has read
/// anchors sometimes sends them to `edit`; this runs what it meant.
pub(super) fn anchored_edit(args: &Value) -> Option<Value> {
    if args.get("old_text").is_some() {
        return None;
    }
    let anchor = args
        .get("from")
        .or_else(|| args.get("lines"))
        .and_then(Value::as_str)?;
    if !anchor.contains('#') {
        return None;
    }
    let (from, to) = match anchor.split_once(['-', '–']) {
        Some((from, to)) if to.contains('#') => (from.trim(), Some(to.trim())),
        _ => (anchor.trim(), args.get("to").and_then(Value::as_str)),
    };
    let mut edit = json!({
        "op": args.get("op").and_then(Value::as_str).unwrap_or("replace"),
        "from": from,
        "text": args.get("text").or_else(|| args.get("new_text")).cloned().unwrap_or(json!("")),
    });
    if let Some(to) = to {
        edit["to"] = json!(to);
    }
    Some(json!({ "path": args.get("path").cloned().unwrap_or(Value::Null), "edits": [edit] }))
}
