//! Routing: the decisions the orchestrator makes, and the rules that bound
//! them.
//!
//! The loop calls into here to answer "may this agent do that, and to whom",
//! so the rules live in one testable place rather than scattered through the
//! run loop. Executing a delegation is the loop's job; deciding whether it is
//! allowed is this module's.
//!
//! See `docs/agents.md`, sections "Delegation rules" and "The orchestrator's
//! prompt".

use serde::{Deserialize, Serialize};

use crate::agent_def::{canonical_name, AgentDef, Tier, ORCHESTRATOR};

/// What the orchestrator (or a specialist handing back) asked for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Switch {
    /// The specialist takes over the session and carries on to the end.
    Handoff { to: String, reason: String },
    /// The specialist works in a branch and returns a summary.
    Delegate(Delegation),
}

/// One delegated piece of work.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delegation {
    pub to: String,
    /// The briefing. A sub-agent starts from this and nothing else, so it has
    /// to carry the whole task — a vague brief produces a specialist that
    /// re-derives what the orchestrator already knew.
    pub task: String,
    /// Tier the orchestrator chose for this call, overriding the agent's own.
    pub tier: Option<Tier>,
    /// Paths or globs this delegation intends to change.
    pub writes: Vec<String>,
    /// Paths it expects to read. Overlapping reads are harmless.
    pub reads: Vec<String>,
    /// An earlier delegation's session to continue instead of starting a new
    /// one: the specialist keeps its transcript, what it read and changed,
    /// and takes `task` as its next message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<String>,
}

/// Why a switch was refused. Returned to the model so it can correct itself
/// rather than retrying the same call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    UnknownAgent {
        name: String,
        known: Vec<String>,
    },
    NotPermitted {
        from: String,
        to: String,
        handoff: bool,
    },
    SelfCall {
        name: String,
    },
    EmptyTask,
}

impl Refusal {
    /// A sentence the model can act on. Naming the alternatives matters: a
    /// bare "not allowed" invites the same call again with a different typo.
    pub fn message(&self) -> String {
        match self {
            Self::UnknownAgent { name, known } => {
                format!("no agent named `{name}`. Available: {}", known.join(", "))
            }
            Self::NotPermitted {
                from,
                to,
                handoff: true,
            } => format!(
                "`{from}` may not hand the conversation to `{to}`. A specialist hands it \
                 back to `{ORCHESTRATOR}`, which routes it on."
            ),
            Self::NotPermitted { from, to, .. } => format!(
                "`{from}` may not delegate to `{to}`. Only the orchestrator delegates \
                 freely; a specialist may call `librarian` and nothing else. Do the work \
                 yourself or report back so the orchestrator can re-route it."
            ),
            Self::SelfCall { name } => {
                format!("`{name}` cannot delegate to itself; that would not terminate")
            }
            Self::EmptyTask => {
                "a delegation needs a task description; the sub-agent sees nothing else".into()
            }
        }
    }
}

/// Decide whether `from` may switch to the requested agent.
///
/// `roster` is the full set of known agents; `from` is the agent asking.
pub fn authorise(from: &AgentDef, switch: &Switch, roster: &[AgentDef]) -> Result<(), Refusal> {
    let (to, handoff) = match switch {
        Switch::Handoff { to, .. } => (to, true),
        Switch::Delegate(d) => {
            if d.task.trim().is_empty() {
                return Err(Refusal::EmptyTask);
            }
            (&d.to, false)
        }
    };
    let to = canonical_name(to);

    if to == from.name {
        return Err(Refusal::SelfCall {
            name: to.to_owned(),
        });
    }
    if !roster.iter().any(|a| a.name == to) {
        return Err(Refusal::UnknownAgent {
            name: to.to_owned(),
            // Agents that exist only to be invoked by the loop are not
            // routing targets, so suggesting them would be misleading.
            known: roster
                .iter()
                .filter(|a| a.is_routable() && a.name != ORCHESTRATOR)
                .map(|a| a.name.clone())
                .collect(),
        });
    }
    let permitted = if handoff {
        from.delegation.may_hand_off_to(to)
    } else {
        from.delegation.may_call(to)
    };
    if !permitted {
        return Err(Refusal::NotPermitted {
            from: from.name.clone(),
            to: to.to_owned(),
            handoff,
        });
    }
    Ok(())
}

/// Whether two delegations may run at the same time.
///
/// Only overlapping *writes* conflict. Reads may overlap freely — two agents
/// reading the same file cannot corrupt each other.
pub fn conflicts(a: &Delegation, b: &Delegation) -> bool {
    a.writes
        .iter()
        .any(|x| b.writes.iter().any(|y| globs_overlap(x, y)))
}

/// Group delegations into waves that may each run in parallel.
///
/// A delegation joins the first wave where it conflicts with nothing. Nothing
/// is refused: a router whose delegation was rejected would have to re-plan,
/// and it has no better information than this scheduler does. The conflicting
/// ones simply run later.
pub fn schedule(delegations: Vec<Delegation>) -> Vec<Vec<Delegation>> {
    let mut waves: Vec<Vec<Delegation>> = Vec::new();
    for d in delegations {
        match waves
            .iter_mut()
            .find(|wave| !wave.iter().any(|other| conflicts(other, &d)))
        {
            Some(wave) => wave.push(d),
            None => waves.push(vec![d]),
        }
    }
    waves
}

/// Whether a path is inside a declared contract.
///
/// The contract is enforced, not advisory: the scheduler serialised other work
/// on the strength of this declaration, so a write outside it reintroduces the
/// very conflict the declaration ruled out.
///
/// An empty contract permits nothing. A delegation that declares no writes has
/// said it will not write.
pub fn contract_permits(writes: &[String], path: &str) -> bool {
    writes.iter().any(|pattern| matches_glob(pattern, path))
}

/// Minimal glob matching: `**` spans separators, `*` does not.
///
/// Hand-rolled rather than pulling in a matcher, because the question here is
/// only whether two declared path patterns could touch the same file.
fn matches_glob(pattern: &str, path: &str) -> bool {
    let pattern = pattern.trim_start_matches("./");
    let path = path.trim_start_matches("./");
    glob_inner(pattern.as_bytes(), path.as_bytes())
}

fn glob_inner(pattern: &[u8], path: &[u8]) -> bool {
    if pattern.is_empty() {
        return path.is_empty();
    }
    if pattern.starts_with(b"**") {
        // `**` may consume anything, separators included.
        let rest = pattern[2..].strip_prefix(b"/").unwrap_or(&pattern[2..]);
        if rest.is_empty() {
            return true;
        }
        for i in 0..=path.len() {
            if glob_inner(rest, &path[i..]) {
                return true;
            }
        }
        return false;
    }
    match pattern[0] {
        b'*' => {
            // A single star stops at a separator.
            for i in 0..=path.len() {
                if path[..i].contains(&b'/') {
                    break;
                }
                if glob_inner(&pattern[1..], &path[i..]) {
                    return true;
                }
            }
            false
        }
        c => !path.is_empty() && path[0] == c && glob_inner(&pattern[1..], &path[1..]),
    }
}

/// Whether two patterns could name the same file.
///
/// Used to decide scheduling, where a false positive costs parallelism and a
/// false negative costs correctness — so this errs toward reporting overlap.
fn globs_overlap(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    // A concrete path on either side can be tested against the other directly.
    if !a.contains('*') {
        return matches_glob(b, a);
    }
    if !b.contains('*') {
        return matches_glob(a, b);
    }
    // Two patterns: compare the literal prefix before the first wildcard. If
    // one prefix contains the other they can reach a common subtree.
    let prefix = |s: &str| s.split('*').next().unwrap_or("").to_owned();
    let (pa, pb) = (prefix(a), prefix(b));
    pa.starts_with(&pb) || pb.starts_with(&pa)
}

/// The message a sub-agent's report reaches its caller in. The interface
/// reads it back out when a session is replayed, so both sides go through
/// this pair rather than each spelling the format.
pub fn report_message(agent: &str, summary: &str) -> String {
    format!("[delegation to `{agent}` finished]\n{summary}")
}

/// A delegation's report as its caller reads it. One that finished with its
/// report has had its transcript cleared, so it is told without a session to
/// resume; one that did not finish names the session that holds its work.
pub fn report_message_of(report: &crate::event::DelegationReport, kept: bool) -> String {
    if kept {
        report_message_for(&report.agent, &report.session_id, &report.summary)
    } else {
        report_message(&report.agent, &report.summary)
    }
}

/// A report with the session it came from, so the orchestrator can have the
/// same specialist continue rather than start over.
pub fn report_message_for(agent: &str, session_id: &str, summary: &str) -> String {
    report_message(
        agent,
        &format!(
            "{summary}\n\n(session `{session_id}`: to have `{agent}` continue this work, \
             delegate to it again with `resume` set to this id)"
        ),
    )
}

/// The agent and report in a `report_message`; None for any other text.
pub fn parse_report_message(content: &str) -> Option<(&str, &str)> {
    let rest = content.strip_prefix("[delegation to `")?;
    let (agent, rest) = rest.split_once('`')?;
    let summary = rest.strip_prefix(" finished]")?;
    Some((agent, summary.strip_prefix('\n').unwrap_or(summary)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_def::{builtin_agents, Delegation as Perm};

    fn roster() -> Vec<AgentDef> {
        builtin_agents()
    }

    fn agent(name: &str) -> AgentDef {
        roster().into_iter().find(|a| a.name == name).unwrap()
    }

    fn delegate_to(name: &str) -> Switch {
        Switch::Delegate(Delegation {
            to: name.into(),
            task: "do the thing".into(),
            ..Default::default()
        })
    }

    #[test]
    fn the_orchestrator_may_delegate_to_a_specialist() {
        assert!(authorise(&agent(ORCHESTRATOR), &delegate_to("fe"), &roster()).is_ok());
    }

    /// Depth is one level: a tree of delegating specialists reaches forty
    /// agents at three levels, which no task justifies.
    #[test]
    fn a_specialist_may_not_delegate_to_another_specialist() {
        let err = authorise(&agent("fe"), &delegate_to("be"), &roster()).unwrap_err();
        assert!(matches!(err, Refusal::NotPermitted { .. }));
        assert!(
            err.message().contains("librarian"),
            "the refusal should say what IS allowed: {}",
            err.message()
        );
    }

    /// The one exception, and the reason for it: librarian is read-only and
    /// exists to keep a specialist's context clean.
    #[test]
    fn a_specialist_may_call_the_librarian() {
        assert!(authorise(&agent("fe"), &delegate_to("librarian"), &roster()).is_ok());
    }

    #[test]
    fn the_librarian_is_a_leaf() {
        assert_eq!(agent("librarian").delegation, Perm::None);
        assert!(authorise(&agent("librarian"), &delegate_to("fe"), &roster()).is_err());
    }

    #[test]
    fn an_unknown_agent_is_refused_with_the_alternatives() {
        let err = authorise(&agent(ORCHESTRATOR), &delegate_to("frontend"), &roster()).unwrap_err();
        let msg = err.message();
        assert!(msg.contains("frontend"), "{msg}");
        assert!(msg.contains("fe"), "the real name should be offered: {msg}");
        assert!(
            !msg.contains("compactor"),
            "the compactor is not a routing target: {msg}"
        );
    }

    #[test]
    fn an_agent_cannot_delegate_to_itself() {
        let err =
            authorise(&agent(ORCHESTRATOR), &delegate_to(ORCHESTRATOR), &roster()).unwrap_err();
        assert!(matches!(err, Refusal::SelfCall { .. }));
        // Its old name is still itself.
        let err = authorise(&agent(ORCHESTRATOR), &delegate_to("router"), &roster()).unwrap_err();
        assert!(matches!(err, Refusal::SelfCall { .. }));
    }

    /// A sub-agent sees the briefing and nothing else, so an empty one leaves
    /// it with no task at all.
    #[test]
    fn a_delegation_without_a_task_is_refused() {
        let switch = Switch::Delegate(Delegation {
            to: "fe".into(),
            task: "   ".into(),
            ..Default::default()
        });
        assert_eq!(
            authorise(&agent(ORCHESTRATOR), &switch, &roster()).unwrap_err(),
            Refusal::EmptyTask
        );
    }

    #[test]
    fn a_handoff_needs_no_task() {
        let switch = Switch::Handoff {
            to: "fe".into(),
            reason: "the whole request is frontend".into(),
        };
        assert!(authorise(&agent(ORCHESTRATOR), &switch, &roster()).is_ok());
    }

    /// A specialist holding the conversation can give it back, so a user who
    /// moves on to another domain is not stuck with it; and only back.
    #[test]
    fn a_specialist_may_hand_back_to_the_orchestrator_only() {
        let back = Switch::Handoff {
            to: ORCHESTRATOR.into(),
            reason: "the user asked about the database".into(),
        };
        assert!(authorise(&agent("fe"), &back, &roster()).is_ok());
        let sideways = Switch::Handoff {
            to: "be".into(),
            reason: "backend work".into(),
        };
        let err = authorise(&agent("fe"), &sideways, &roster()).unwrap_err();
        assert!(
            err.message().contains(ORCHESTRATOR),
            "the refusal names the way out: {}",
            err.message()
        );
        assert!(authorise(&agent("librarian"), &back, &roster()).is_err());
    }

    // --- contracts and scheduling ---

    fn writing(to: &str, writes: &[&str]) -> Delegation {
        Delegation {
            to: to.into(),
            task: "work".into(),
            writes: writes.iter().map(|w| (*w).to_owned()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn separate_areas_do_not_conflict() {
        assert!(!conflicts(
            &writing("fe", &["src/ui/**"]),
            &writing("be", &["src/api/**"])
        ));
    }

    #[test]
    fn the_same_area_conflicts() {
        assert!(conflicts(
            &writing("fe", &["src/ui/**"]),
            &writing("test", &["src/ui/**"])
        ));
    }

    #[test]
    fn a_concrete_path_inside_a_glob_conflicts() {
        assert!(conflicts(
            &writing("fe", &["src/ui/**"]),
            &writing("test", &["src/ui/button.tsx"])
        ));
    }

    /// Reads never conflict — two agents reading a file cannot corrupt it.
    #[test]
    fn overlapping_reads_are_fine() {
        let a = Delegation {
            to: "fe".into(),
            task: "t".into(),
            reads: vec!["src/types.rs".into()],
            writes: vec!["src/ui/**".into()],
            ..Default::default()
        };
        let b = Delegation {
            to: "be".into(),
            task: "t".into(),
            reads: vec!["src/types.rs".into()],
            writes: vec!["src/api/**".into()],
            ..Default::default()
        };
        assert!(!conflicts(&a, &b));
    }

    /// Nothing is refused: the conflicting one runs later, the rest together.
    #[test]
    fn conflicting_work_is_serialised_and_the_rest_runs_together() {
        let waves = schedule(vec![
            writing("fe", &["src/ui/**"]),
            writing("be", &["src/api/**"]),
            writing("docs", &["README.md"]),
            writing("test", &["src/ui/**"]), // overlaps fe
        ]);
        assert_eq!(waves.len(), 2, "one extra wave for the overlap: {waves:?}");
        assert_eq!(waves[0].len(), 3, "the independent three run together");
        assert_eq!(waves[1].len(), 1);
        assert_eq!(waves[1][0].to, "test");
    }

    #[test]
    fn independent_work_needs_only_one_wave() {
        let waves = schedule(vec![
            writing("fe", &["src/ui/**"]),
            writing("be", &["src/api/**"]),
        ]);
        assert_eq!(waves.len(), 1);
    }

    #[test]
    fn scheduling_nothing_yields_nothing() {
        assert!(schedule(Vec::new()).is_empty());
    }

    // --- contract enforcement ---

    #[test]
    fn a_write_inside_the_contract_is_permitted() {
        let contract = vec!["src/ui/**".to_owned()];
        assert!(contract_permits(&contract, "src/ui/button.tsx"));
        assert!(contract_permits(&contract, "src/ui/nested/deep.tsx"));
    }

    #[test]
    fn a_write_outside_the_contract_is_refused() {
        let contract = vec!["src/ui/**".to_owned()];
        assert!(!contract_permits(&contract, "src/api/handler.rs"));
        assert!(!contract_permits(&contract, "README.md"));
    }

    /// A delegation that declared no writes has said it will not write.
    #[test]
    fn an_empty_contract_permits_nothing() {
        assert!(!contract_permits(&[], "anything.rs"));
    }

    #[test]
    fn a_single_star_stops_at_a_separator() {
        let contract = vec!["src/*.rs".to_owned()];
        assert!(contract_permits(&contract, "src/main.rs"));
        assert!(
            !contract_permits(&contract, "src/ui/button.rs"),
            "a single star must not span directories"
        );
    }

    #[test]
    fn a_leading_dot_slash_is_ignored() {
        assert!(contract_permits(&["./src/**".to_owned()], "src/main.rs"));
        assert!(contract_permits(&["src/**".to_owned()], "./src/main.rs"));
    }

    #[test]
    fn an_exact_path_contract_matches_only_itself() {
        let contract = vec!["src/main.rs".to_owned()];
        assert!(contract_permits(&contract, "src/main.rs"));
        assert!(!contract_permits(&contract, "src/main.rs.bak"));
        assert!(!contract_permits(&contract, "src/other.rs"));
    }
}

/// Wire schemas for the routing tools.
///
/// These are not ordinary `Tool` implementations: executing one changes which
/// agent holds the session, or runs a whole nested turn. Both are outside what
/// a `ToolCtx` can reach, so the loop intercepts them by name before dispatch
/// rather than registering them with the others.
/// Who an agent may hand the conversation to, for the tools it is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandOff {
    /// No handoff tool: a delegated sub-agent reports back instead.
    No,
    /// The orchestrator: any specialist.
    Anyone,
    /// A specialist holding the user's conversation: back to the orchestrator.
    BackToOrchestrator,
}

/// `stop_delegation`, for an agent that delegates: stop one of its
/// delegations still at work.
pub fn stop_delegation_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "stop_delegation",
            "description":
                "Stop one of your delegations that is still at work: when it is going the \
                 wrong way, its task is no longer needed, or another agent's work makes it \
                 wrong. It stops at its next step and its report comes back saying it was \
                 stopped and why, with what it had changed; its transcript is kept for \
                 `delegation_log` or `resume`. Not for one that is slow but right.",
            "parameters": {
                "type": "object",
                "properties": {
                    "session": {"type": "string", "description": "its session id, from the list of delegations still at work"},
                    "reason": {"type": "string", "description": "why, in one sentence; it goes into its report"}
                },
                "required": ["session", "reason"]
            }
        }
    })
}

/// `delegation_log`, for an agent that delegates: read what one of its
/// delegations did, from its transcript.
pub fn delegation_log_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "delegation_log",
            "description":
                "Read what one of your delegations did: what it was asked, what it said, \
                 the tools it called and what they returned. Use it when a delegation was \
                 stopped, failed or came back without a full report, before you brief the \
                 next one: start the new brief where that work stopped, and do not redo what \
                 it already changed.",
            "parameters": {
                "type": "object",
                "properties": {
                    "session": {"type": "string", "description": "the session id its report gave"}
                },
                "required": ["session"]
            }
        }
    })
}

pub fn routing_schemas(hand_off: HandOff) -> Vec<serde_json::Value> {
    use serde_json::json;
    let delegate = json!({
        "type": "function",
        "function": {
            "name": "delegate",
            "description":
                "Give a piece of work to a specialist. It starts from your briefing \
                 alone, works in its own context, and returns a summary. Call it \
                 several times in one step to run parts at the same time, several to \
                 the same specialist if you like: parts that do not depend on each \
                 other go out together, never one after another. A file one agent is \
                 editing is closed to the others until it finishes.",
            "parameters": {
                "type": "object",
                "properties": {
                    "agent": {"type": "string", "description": "which specialist"},
                    "task": {
                        "type": "string",
                        "description":
                            "the briefing; the specialist sees nothing else, so state \
                             the goal, the constraints, and what done looks like"
                    },
                    "tier": {
                        "type": "string",
                        "description":
                            "cheap | balanced | strong. Start one lower than feels \
                             right; the ladder promotes on failure."
                    },
                    "resume": {
                        "type": "string",
                        "description":
                            "the session id an earlier report gave, to have that same \
                             specialist continue its work (after a failure, running out \
                             of steps, or for a follow-up) instead of starting over: it \
                             keeps everything it read and changed, and `task` is its next \
                             message, saying what is left"
                    },
                },
                "required": ["agent", "task"]
            }
        }
    });
    match hand_off {
        HandOff::No => return vec![delegate],
        HandOff::BackToOrchestrator => {
            return vec![
                delegate,
                json!({
                    "type": "function",
                    "function": {
                        "name": "handoff",
                        "description":
                            "Hand the conversation back to the orchestrator when the \
                             user asks for something outside your domain. It carries on \
                             from here and routes the request to the right specialist. \
                             Not needed to finish: when the orchestrator handed you the \
                             conversation, it goes back by itself once you have answered.",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "agent": {"type": "string", "enum": [ORCHESTRATOR]},
                                "reason": {
                                    "type": "string",
                                    "description": "what the user asked for that is not yours"
                                }
                            },
                            "required": ["agent", "reason"]
                        }
                    }
                }),
            ];
        }
        HandOff::Anyone => {}
    }
    vec![
        delegate,
        json!({
            "type": "function",
            "function": {
                "name": "handoff",
                "description":
                    "Give the conversation to a specialist for this request. It \
                     inherits everything said so far, does the work and answers the \
                     user, and the conversation comes back to you when its turn ends. \
                     Use this when the request is one specialist's work; for a piece \
                     of a larger plan, or a one-off result, delegate instead.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "agent": {"type": "string"},
                        "reason": {
                            "type": "string",
                            "description":
                                "why this specialist; shown to the user, who otherwise \
                                 sees the voice change for no stated reason"
                        }
                    },
                    "required": ["agent", "reason"]
                }
            }
        }),
    ]
}

/// Whether `name` is one of the routing tools the loop intercepts.
pub fn is_routing_tool(name: &str) -> bool {
    name == "handoff" || name == "delegate"
}

/// Read a routing call's arguments into a `Switch`.
///
/// Returns None when the shape is wrong; the loop reports that to the model as
/// an ordinary tool error so it can correct the call.
pub fn parse_switch(name: &str, args: &serde_json::Value) -> Option<Switch> {
    let text = |key: &str| {
        args.get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_owned()
    };
    let list = |key: &str| -> Vec<String> {
        args.get(key)
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(|s| s.trim().to_owned())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    };
    match name {
        "handoff" => Some(Switch::Handoff {
            to: text("agent"),
            reason: text("reason"),
        }),
        "delegate" => Some(Switch::Delegate(Delegation {
            to: text("agent"),
            task: text("task"),
            tier: args
                .get("tier")
                .and_then(serde_json::Value::as_str)
                .and_then(Tier::parse),
            writes: list("writes"),
            reads: list("reads"),
            resume: Some(text("resume")).filter(|id| !id.is_empty()),
        })),
        _ => None,
    }
}

#[cfg(test)]
mod wire_tests {
    use super::*;
    use serde_json::json;

    fn names(hand_off: HandOff) -> Vec<String> {
        routing_schemas(hand_off)
            .iter()
            .map(|s| s["function"]["name"].as_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn the_orchestrator_is_offered_both_tools() {
        assert_eq!(names(HandOff::Anyone), vec!["delegate", "handoff"]);
    }

    /// A delegated sub-agent reports back; it has no conversation to hand.
    #[test]
    fn a_delegated_specialist_is_offered_delegate_only() {
        assert_eq!(names(HandOff::No), vec!["delegate"]);
    }

    /// A specialist holding the conversation can only give it back.
    #[test]
    fn a_specialist_holding_the_conversation_can_hand_it_back() {
        assert_eq!(
            names(HandOff::BackToOrchestrator),
            vec!["delegate", "handoff"]
        );
        let schema = &routing_schemas(HandOff::BackToOrchestrator)[1];
        assert_eq!(
            schema["function"]["parameters"]["properties"]["agent"]["enum"],
            json!([ORCHESTRATOR])
        );
    }

    #[test]
    fn a_delegation_parses_with_its_contract() {
        let switch = parse_switch(
            "delegate",
            &json!({
                "agent": "fe",
                "task": "fix the layout",
                "tier": "cheap",
                "writes": ["src/ui/**", ""],
                "reads": ["src/types.rs"]
            }),
        )
        .expect("parses");
        let Switch::Delegate(d) = switch else {
            panic!("expected a delegation")
        };
        assert_eq!(d.to, "fe");
        assert_eq!(d.tier, Some(Tier::Cheap));
        assert_eq!(d.writes, vec!["src/ui/**"], "blank entries are dropped");
        assert_eq!(d.reads, vec!["src/types.rs"]);
    }

    #[test]
    fn a_handoff_parses() {
        let switch = parse_switch(
            "handoff",
            &json!({"agent": "be", "reason": "it is an API bug"}),
        )
        .expect("parses");
        assert_eq!(
            switch,
            Switch::Handoff {
                to: "be".into(),
                reason: "it is an API bug".into()
            }
        );
    }

    /// Missing fields yield empty strings rather than failing to parse, so the
    /// refusal that follows can name what was actually wrong.
    #[test]
    fn a_malformed_call_still_parses_into_something_refusable() {
        let switch = parse_switch("delegate", &json!({"agent": "fe"})).expect("parses");
        let Switch::Delegate(d) = switch else {
            panic!("expected a delegation")
        };
        assert!(d.task.is_empty());
    }

    #[test]
    fn an_unknown_tool_is_not_a_switch() {
        assert!(parse_switch("read", &json!({})).is_none());
        assert!(!is_routing_tool("read"));
        assert!(is_routing_tool("handoff"));
        assert!(is_routing_tool("delegate"));
    }

    #[test]
    fn a_report_message_reads_back() {
        let message = report_message("fe", "DONE: it\nCHANGED: a.rs");
        assert_eq!(
            parse_report_message(&message),
            Some(("fe", "DONE: it\nCHANGED: a.rs"))
        );
        assert_eq!(parse_report_message("an ordinary user message"), None);
    }
}
