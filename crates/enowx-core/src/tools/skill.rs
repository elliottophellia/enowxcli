use super::{string_arg, Tool, ToolCtx, ToolOutput};
use crate::discovery::{Discovery, SkillEntry, SkillScope};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;

/// Reads a discovered skill's `SKILL.md` on demand so the model does not carry
/// every skill body in context, only the inventory advertised in the system prompt.
pub struct SkillReadTool {
    discovery: Arc<Discovery>,
    disabled: Arc<Vec<String>>,
}

impl SkillReadTool {
    pub fn new(discovery: Arc<Discovery>) -> Self {
        Self {
            discovery,
            disabled: Arc::new(Vec::new()),
        }
    }

    pub fn with_disabled(discovery: Arc<Discovery>, disabled: Vec<String>) -> Self {
        Self {
            discovery,
            disabled: Arc::new(disabled),
        }
    }

    fn find(&self, name: &str) -> Option<&SkillEntry> {
        let needle = name.to_ascii_lowercase();
        if self.disabled.iter().any(|d| d == &needle) {
            return None;
        }
        self.discovery.skills.iter().find(|s| s.name == needle)
    }
}

#[async_trait]
impl Tool for SkillReadTool {
    fn name(&self) -> &str {
        "skill_read"
    }
    fn description(&self) -> &str {
        "Read a discovered skill's SKILL.md by name, when the task needs the instructions \
         it holds. Most tasks need none; read only the one that applies."
    }
    fn parameters(&self) -> Value {
        json!({
            "type":"object",
            "properties":{"name":{"type":"string","description":"Skill name as listed in the system prompt"}},
            "required":["name"],
            "additionalProperties":false
        })
    }
    async fn execute(&self, ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        let name = string_arg(&args, "name")?;
        let Some(entry) = self.find(name) else {
            return Ok(ToolOutput::error(format!(
                "no skill named `{name}` was discovered"
            )));
        };
        let carried = ctx.skills.iter().any(|s| s == &entry.name);
        if !carried && entry.scope != SkillScope::Builtin && self.discovery.is_bound(&entry.name) {
            return Ok(ToolOutput::error(format!(
                "`{}` is bound to other agents",
                entry.name
            )));
        }
        if entry.scope == SkillScope::Builtin && !carried {
            return Ok(ToolOutput::error(format!(
                "`{}` is not one of your skills",
                entry.name
            )));
        }
        let body = match entry.scope {
            SkillScope::Builtin => crate::discovery::skills::builtin_source(&entry.name)
                .map(str::to_owned)
                .ok_or_else(|| anyhow::anyhow!("built-in skill `{}` is missing", entry.name))?,
            _ => std::fs::read_to_string(&entry.path)?,
        };
        Ok(ToolOutput::ok(format!(
            "# {} ({})\n{}",
            entry.name,
            entry.path.display(),
            body
        )))
    }
}

/// Binds a skill found on disk to the agents whose work it serves, so only
/// they are offered it. The orchestrator's tool.
pub struct SkillBindTool {
    discovery: Arc<Discovery>,
}

impl SkillBindTool {
    pub fn new(discovery: Arc<Discovery>) -> Self {
        Self { discovery }
    }
}

#[async_trait]
impl Tool for SkillBindTool {
    fn name(&self) -> &str {
        "skill_bind"
    }
    fn description(&self) -> &str {
        "Bind skills installed in the project or the user's home to the agents whose work \
         they serve; from then on only those agents are offered them. Bind every skill in one \
         call with `bindings`, not one call each. An empty `agents` list unbinds a skill, so \
         every agent is offered it again. Built-in skills come with their agents and cannot \
         be bound."
    }
    fn parameters(&self) -> Value {
        json!({
            "type":"object",
            "properties":{
                "bindings":{"type":"array","description":"Each skill and the agents it goes to","items":{
                    "type":"object",
                    "properties":{
                        "skill":{"type":"string","description":"Skill name as listed under the skills installed on this machine"},
                        "agents":{"type":"array","items":{"type":"string"},"description":"Agent names from the roster, for example [\"be\"] or [\"fe\",\"motion\"]"}
                    },
                    "required":["skill","agents"]
                }},
                "skill":{"type":"string","description":"One skill, when binding only one"},
                "agents":{"type":"array","items":{"type":"string"},"description":"Its agents, with `skill`"}
            },
            "additionalProperties":false
        })
    }
    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        let asked: Vec<Value> = match args["bindings"].as_array() {
            Some(list) if !list.is_empty() => list.clone(),
            _ if args["skill"].is_string() => vec![args.clone()],
            _ => {
                return Ok(ToolOutput::error(
                    "give `bindings`: each skill with its agents",
                ))
            }
        };
        // Everything is checked before anything changes, so a mistake in one
        // binding leaves them all as they were.
        let mut plan: Vec<(String, Vec<String>)> = Vec::new();
        for binding in &asked {
            let Some(name) = binding["skill"].as_str().map(str::to_ascii_lowercase) else {
                return Ok(ToolOutput::error("each binding needs a `skill`"));
            };
            let Some(entry) = self.discovery.skills.iter().find(|s| s.name == name) else {
                return Ok(ToolOutput::error(format!(
                    "no skill named `{name}` was discovered"
                )));
            };
            if entry.scope == SkillScope::Builtin {
                return Ok(ToolOutput::error(format!(
                    "`{name}` is built in: it comes with the agents that carry it"
                )));
            }
            let requested: Vec<String> = binding["agents"]
                .as_array()
                .map(|list| {
                    list.iter()
                        .filter_map(Value::as_str)
                        .map(|a| crate::agent_def::canonical_name(a).to_owned())
                        .collect()
                })
                .unwrap_or_default();
            let unknown: Vec<&str> = requested
                .iter()
                .filter(|a| !self.discovery.agents.iter().any(|d| &d.name == *a))
                .map(String::as_str)
                .collect();
            if !unknown.is_empty() {
                return Ok(ToolOutput::error(format!(
                    "no agent named {} in the roster",
                    unknown
                        .iter()
                        .map(|a| format!("`{a}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
            plan.push((name, requested));
        }
        let Ok(mut bindings) = self.discovery.bindings.write() else {
            return Ok(ToolOutput::error("skill bindings are unavailable"));
        };
        for (name, agents) in &plan {
            bindings.set(name, agents.clone());
        }
        bindings.save()?;
        let summary = plan
            .iter()
            .map(|(name, _)| match bindings.agents_for(name) {
                Some(agents) => format!(
                    "`{name}` is now offered only to {}",
                    agents
                        .iter()
                        .map(|a| format!("`{a}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                None => format!("`{name}` is offered to every agent again"),
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(ToolOutput::ok(summary))
    }
}
