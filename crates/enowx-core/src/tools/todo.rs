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
            "items":{"type":"array","items":{"type":"string"}},
            "item":{"type":"string"}
        },"required":["op"],"additionalProperties":false})
    }
    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        let op = string_arg(&args, "op")?;
        let mut items = self.items.lock().await;
        match op {
            "set" => {
                let raw = args
                    .get("items")
                    .and_then(Value::as_array)
                    .ok_or_else(|| anyhow::anyhow!("set requires items"))?;
                *items = raw
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|text| TodoItem {
                        text: text.to_string(),
                        done: false,
                    })
                    .collect();
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

/// The item a `done` names: its exact text, or failing that the one item
/// whose text starts with it, ignoring case. Models shorten long items when
/// they tick them off, and refusing those turned each tick into a retry.
fn find_item<'a>(items: &'a mut [TodoItem], target: &str) -> Option<&'a mut TodoItem> {
    let target = target.trim();
    if let Some(index) = items.iter().position(|item| item.text == target) {
        return items.get_mut(index);
    }
    let lower = target.to_lowercase();
    let mut matches = items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.text.to_lowercase().starts_with(&lower));
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
        }
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
