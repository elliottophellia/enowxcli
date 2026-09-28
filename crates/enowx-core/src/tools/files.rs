use super::{
    display_path, number_arg, resolve_existing, resolve_for_write, string_arg, Tool, ToolCtx,
    ToolOutput,
};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use serde_json::{json, Value};

pub(super) struct ReadTool;

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str {
        "read"
    }
    fn description(&self) -> &str {
        "Read a UTF-8 file with numbered lines. Use offset and limit for large files."
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
                "{:>width$}:{}\n",
                start + index + 1,
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
        "Create or overwrite one UTF-8 file inside the workspace."
    }
    fn parameters(&self) -> Value {
        json!({"type":"object","properties":{
            "path":{"type":"string"},"content":{"type":"string"}
        },"required":["path","content"],"additionalProperties":false})
    }
    async fn execute(&self, ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        let raw = string_arg(&args, "path")?;
        let content = string_arg(&args, "content")?;
        let path = resolve_for_write(&ctx.workspace, raw)?;
        // Kept so the result can say what changed, and the interface show it.
        let before = std::fs::read_to_string(&path).ok();
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
        Ok(output)
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
        crate::config::atomic_write(&path, text.as_bytes())?;
        let at: Vec<String> = lines.iter().map(ToString::to_string).collect();
        Ok(ToolOutput::ok(format!(
            "Updated {raw}: {} edits, at lines {}",
            lines.len(),
            at.join(", ")
        )))
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
        crate::config::atomic_write(&path, source.replacen(old, new, 1).as_bytes())?;
        Ok(ToolOutput::ok(format!(
            "Updated {raw} at line {start_line}"
        )))
    }
}
