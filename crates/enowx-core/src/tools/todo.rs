use super::{string_arg, Tool, ToolCtx, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::Mutex;

#[derive(Default)]
pub(super) struct TodoTool {
    items: Mutex<Vec<TodoItem>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct TodoItem {
    text: String,
    done: bool,
}

#[async_trait]
impl Tool for TodoTool {
    fn name(&self) -> &str {
        "todo"
    }
    fn description(&self) -> &str {
        "A checklist the user can see, for work of four or more steps. Set it once \
         (op=set with items); when steps finish, mark them together (op=done with items). \
         Every change costs a model call, so skip it for small tasks."
    }
    fn parameters(&self) -> Value {
        json!({"type":"object","properties":{
            "op":{"type":"string","enum":["set","done","view","clear"]},
            "items":{
                "type":"array",
                "description":"Each step, a short string. A step with sub-steps may instead be an object {\"text\": \"the step\", \"items\": [ ... ]}; sub-steps are kept, indented under it.",
                "items":{"type":["string","object"]}
            },
            "item":{"type":"string"}
        },"required":["op"],"additionalProperties":false})
    }
    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        // A call with items and no `op` is a plan being set: failing it only
        // costs a second call that says the same thing with `op` added.
        let op = match args.get("op").and_then(Value::as_str) {
            Some(_) => string_arg(&args, "op")?,
            None if args.get("items").is_some() => "set",
            None => string_arg(&args, "op")?,
        };
        let mut items = self.items.lock().await;
        match op {
            "set" => {
                let raw = args
                    .get("items")
                    .and_then(Value::as_array)
                    .ok_or_else(|| anyhow::anyhow!("set requires items"))?;
                // Nested steps are flattened with indentation rather than
                // dropped: the model gets the hierarchy it asked for instead
                // of a silently shorter list.
                let mut flat = Vec::new();
                for value in raw {
                    flatten(value, 0, &mut flat);
                }
                if flat.is_empty() {
                    anyhow::bail!("set requires at least one item");
                }
                *items = flat;
            }
            "done" => {
                // Several at once: one call per finished step was a model
                // round trip per tick.
                let mut targets: Vec<&str> = args
                    .get("items")
                    .and_then(Value::as_array)
                    .map(|raw| raw.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                if let Some(one) = args.get("item").and_then(Value::as_str) {
                    targets.push(one);
                }
                if targets.is_empty() {
                    anyhow::bail!("done requires item or items");
                }
                let mut unknown = Vec::new();
                for target in targets {
                    match find_item(&mut items, target) {
                        Some(item) => item.done = true,
                        None => unknown.push(target),
                    }
                }
                if !unknown.is_empty() {
                    anyhow::bail!(
                        "unknown todo item: {}. The list is:\n{}",
                        unknown.join(", "),
                        items
                            .iter()
                            .map(|item| item.text.as_str())
                            .collect::<Vec<_>>()
                            .join("\n")
                    );
                }
            }
            "clear" => items.clear(),
            "view" => {}
            other => anyhow::bail!("unknown todo operation: {other}"),
        }
        let open = items.iter().filter(|item| !item.done).count();
        let mut out = items
            .iter()
            .map(|item| format!("{} {}", if item.done { "[x]" } else { "[ ]" }, item.text))
            .collect::<Vec<_>>()
            .join("\n");
        if out.is_empty() {
            out = "No tasks.".to_string();
        }
        out.push_str(&format!("\n{open} remaining"));
        Ok(ToolOutput::ok(out))
    }
}

/// Turn one `items` entry into todo lines, keeping any sub-steps indented
/// under their parent. An entry is a plain string, or an object naming the
/// step (`text`/`title`/`task`/`label`/`name`/`step`) with optional nested
/// steps (`items`/`subtasks`/`steps`/`children`), marked done by `done`.
fn flatten(value: &Value, depth: usize, out: &mut Vec<TodoItem>) {
    let indent = "  ".repeat(depth);
    match value {
        Value::String(text) => {
            // A single string may itself carry newlines; each line is a step.
            for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
                out.push(TodoItem {
                    text: format!("{indent}{line}"),
                    done: false,
                });
            }
        }
        Value::Object(map) => {
            let text = ["text", "title", "task", "label", "name", "step"]
                .iter()
                .find_map(|key| map.get(*key).and_then(Value::as_str))
                .unwrap_or("")
                .trim();
            if !text.is_empty() {
                out.push(TodoItem {
                    text: format!("{indent}{text}"),
                    done: map.get("done").and_then(Value::as_bool).unwrap_or(false),
                });
            }
            for key in ["items", "subtasks", "substeps", "steps", "children"] {
                if let Some(children) = map.get(key).and_then(Value::as_array) {
                    for child in children {
                        flatten(child, depth + 1, out);
                    }
                }
            }
        }
        Value::Array(entries) => {
            for entry in entries {
                flatten(entry, depth, out);
            }
        }
        _ => {}
    }
}

/// The item a `done` names: its exact text, or failing that the one item
/// whose text starts with it, ignoring case. Models shorten long items when
/// they tick them off, and refusing those turned each tick into a retry.
fn find_item<'a>(items: &'a mut [TodoItem], target: &str) -> Option<&'a mut TodoItem> {
    let target = target.trim();
    // Items are stored with their indentation; a `done` names the step, not
    // its depth, so both sides are compared with leading spaces removed.
    if let Some(index) = items
        .iter()
        .position(|item| item.text.trim_start() == target)
    {
        return items.get_mut(index);
    }
    let lower = target.to_lowercase();
    let mut matches = items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.text.trim_start().to_lowercase().starts_with(&lower));
    match (matches.next(), matches.next()) {
        (Some((index, _)), None) if !lower.is_empty() => items.get_mut(index),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> ToolCtx {
        ToolCtx {
            workspace: std::env::temp_dir(),
            shell_timeout: std::time::Duration::from_secs(1),
            cancel: tokio_util::sync::CancellationToken::new(),
            progress: None,
            call_id: String::new(),
            skills: Vec::new(),
            lsp: None,
            repair: None,
            vision: false,
            cloudflare_token: None,
        }
    }

    #[tokio::test]
    async fn items_without_an_op_set_the_plan() {
        let tool = TodoTool::default();
        let out = tool
            .execute(&ctx(), json!({"items":["scaffold","build"]}))
            .await
            .unwrap();
        assert!(!out.is_error);
        assert!(out.content.contains("2 remaining"), "{}", out.content);
        assert!(tool.execute(&ctx(), json!({})).await.is_err());
    }

    #[tokio::test]
    async fn several_items_are_marked_done_in_one_call() {
        let tool = TodoTool::default();
        tool.execute(
            &ctx(),
            json!({"op":"set","items":["write a","write b","check"]}),
        )
        .await
        .unwrap();
        let out = tool
            .execute(&ctx(), json!({"op":"done","items":["write a","write b"]}))
            .await
            .unwrap();
        assert!(out.content.contains("[x] write a"), "{}", out.content);
        assert!(out.content.contains("[x] write b"), "{}", out.content);
        assert!(out.content.contains("[ ] check"), "{}", out.content);
        assert!(out.content.contains("1 remaining"), "{}", out.content);
    }

    #[tokio::test]
    async fn nested_steps_are_kept_indented_not_dropped() {
        let tool = TodoTool::default();
        let out = tool
            .execute(
                &ctx(),
                json!({"op":"set","items":[
                    {"text":"Wave 1: foundation","items":[
                        "humanize",
                        {"title":"account combo","done":true}
                    ]},
                    "Wave 2: integration"
                ]}),
            )
            .await
            .unwrap();
        assert!(!out.is_error, "{}", out.content);
        assert!(
            out.content.contains("[ ] Wave 1: foundation"),
            "{}",
            out.content
        );
        assert!(
            out.content.contains("[ ]   humanize"),
            "indented: {}",
            out.content
        );
        assert!(
            out.content.contains("[x]   account combo"),
            "done kept: {}",
            out.content
        );
        assert!(
            out.content.contains("[ ] Wave 2: integration"),
            "{}",
            out.content
        );
        assert!(out.content.contains("3 remaining"), "{}", out.content);
    }

    /// A nested step is ticked off by its own text, whatever its depth.
    #[tokio::test]
    async fn a_nested_step_is_marked_done_by_its_text() {
        let tool = TodoTool::default();
        tool.execute(
            &ctx(),
            json!({"op":"set","items":[{"text":"Wave 1","items":["humanize","proxy"]}]}),
        )
        .await
        .unwrap();
        let out = tool
            .execute(&ctx(), json!({"op":"done","item":"humanize"}))
            .await
            .unwrap();
        assert!(out.content.contains("[x]   humanize"), "{}", out.content);
    }

    #[tokio::test]
    async fn a_shortened_item_still_matches_when_it_is_unambiguous() {
        let tool = TodoTool::default();
        tool.execute(
            &ctx(),
            json!({"op":"set","items":["Write index.html with the hero","Write style.css"]}),
        )
        .await
        .unwrap();
        let out = tool
            .execute(&ctx(), json!({"op":"done","item":"write index.html"}))
            .await
            .unwrap();
        assert!(
            out.content.contains("[x] Write index.html"),
            "{}",
            out.content
        );
        // Ambiguous: both start with "write".
        assert!(tool
            .execute(&ctx(), json!({"op":"done","item":"write"}))
            .await
            .is_err());
    }
}
