use super::{resolve_existing, Tool, ToolCtx, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

/// The marks of generated work in interface code, found by `crate::ui_check`.
pub(super) struct UiCheckTool;

#[async_trait]
impl Tool for UiCheckTool {
    fn name(&self) -> &str {
        "ui_check"
    }
    fn description(&self) -> &str {
        "Search interface code (HTML, JSX/TSX, Vue, Svelte, CSS) for the marks of generated \
         work: invented figures, dead links and controls, removed focus, images without alt, \
         placeholder names, default gradients, glow and glass, buzzwords, generic actions, \
         emoji, em dashes, decorative caps, screen-height sections, several icon sets, \
         colours outside the tokens. Lists them by priority with file and line. Run it on \
         an existing interface before changing it, and on what you built before you report."
    }
    fn parameters(&self) -> Value {
        json!({"type":"object","properties":{
            "path":{"type":"string","description":"A file or directory; the workspace by default"}
        },"additionalProperties":false})
    }
    async fn execute(&self, ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        let raw = args.get("path").and_then(Value::as_str).unwrap_or(".");
        let root = resolve_existing(&ctx.workspace, raw)?;
        let files = crate::ui_check::interface_files(&root);
        let findings = crate::ui_check::check(&ctx.workspace, &files);
        Ok(ToolOutput::ok(crate::ui_check::report(
            &findings,
            files.len(),
        )))
    }
}
