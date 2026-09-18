//! Routing: the decisions a router makes, and the rules that bound them.
//!
//! The loop calls into here to answer "may this agent do that, and to whom",
//! so the rules live in one testable place rather than scattered through the
//! run loop. Executing a delegation is the loop's job; deciding whether it is
//! allowed is this module's.
//!
//! See `docs/agents.md`, sections "Delegation rules" and "The router's prompt".

use serde::{Deserialize, Serialize};

use crate::agent_def::{AgentDef, Tier};

/// What the router asked for.
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
    /// re-derives what the router already knew.
    pub task: String,
    /// Tier the router chose for this call, overriding the agent's own.
    pub tier: Option<Tier>,
    /// Paths or globs this delegation intends to change.
    pub writes: Vec<String>,
    /// Paths it expects to read. Overlapping reads are harmless.
    pub reads: Vec<String>,
}

/// Why a switch was refused. Returned to the model so it can correct itself
/// rather than retrying the same call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    UnknownAgent { name: String, known: Vec<String> },
    NotPermitted { from: String, to: String },
    SelfCall { name: String },
    EmptyTask,
}

impl Refusal {
    /// A sentence the model can act on. Naming the alternatives matters: a
    /// bare "not allowed" invites the same call again with a different typo.
    pub fn message(&self) -> String {
        match self {
            Self::UnknownAgent { name, known } => format!(
                "no agent named `{name}`. Available: {}",
                known.join(", ")
            ),
            Self::NotPermitted { from, to } => format!(
                "`{from}` may not delegate to `{to}`. Only the router delegates freely; \
                 a specialist may call `librarian` and nothing else. Do the work yourself \
                 or report back so the router can re-route it."
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
    let to = match switch {
        Switch::Handoff { to, .. } => to,
        Switch::Delegate(d) => {
            if d.task.trim().is_empty() {
                return Err(Refusal::EmptyTask);
            }
            &d.to
        }
    };
    let to = to.trim();

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
                .filter(|a| a.name != "compactor" && a.name != "router")
                .map(|a| a.name.clone())
                .collect(),
        });
    }
    if !from.delegation.may_call(to) {
        return Err(Refusal::NotPermitted {
            from: from.name.clone(),
            to: to.to_owned(),
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
    fn the_router_may_delegate_to_a_specialist() {
        assert!(authorise(&agent("router"), &delegate_to("fe"), &roster()).is_ok());
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
        let err = authorise(&agent("router"), &delegate_to("frontend"), &roster()).unwrap_err();
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
        let err = authorise(&agent("router"), &delegate_to("router"), &roster()).unwrap_err();
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
            authorise(&agent("router"), &switch, &roster()).unwrap_err(),
            Refusal::EmptyTask
        );
    }

    #[test]
    fn a_handoff_needs_no_task() {
        let switch = Switch::Handoff {
            to: "fe".into(),
            reason: "the whole request is frontend".into(),
        };
        assert!(authorise(&agent("router"), &switch, &roster()).is_ok());
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
