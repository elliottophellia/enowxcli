//! Agent definitions: who can work, what they may touch, and on which model.
//!
//! These replace the fixed `Role` enum. An agent is data — a name, a
//! description the router chooses from, a tool surface, and a prompt — so the
//! roster can be extended by adding a file rather than by recompiling. Shipped
//! agents are compiled in as defaults; discovered ones override them by name.
//!
//! See `docs/agents.md` for the design this implements.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// How much model a piece of work deserves.
///
/// Agents name a tier rather than a model so a definition survives changing
/// provider: the mapping from tier to model id lives in config, and an
/// unmapped tier falls back to the active model rather than failing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// Mechanical work against a clear specification.
    Cheap,
    /// Normal implementation: known shape, known cause.
    #[default]
    Balanced,
    /// Work needing judgement, where a wrong answer is expensive.
    Strong,
}

impl Tier {
    pub fn id(self) -> &'static str {
        match self {
            Self::Cheap => "cheap",
            Self::Balanced => "balanced",
            Self::Strong => "strong",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "cheap" | "fast" | "small" => Some(Self::Cheap),
            "balanced" | "default" | "medium" => Some(Self::Balanced),
            "strong" | "smart" | "large" => Some(Self::Strong),
            _ => None,
        }
    }

    /// The next tier down, or None at the bottom.
    ///
    /// The fallback ladder walks this way: when a model cannot be reached at
    /// all, a weaker one is better than nothing, and the user is told.
    pub fn lower(self) -> Option<Self> {
        match self {
            Self::Strong => Some(Self::Balanced),
            Self::Balanced => Some(Self::Cheap),
            Self::Cheap => None,
        }
    }
}

/// What an agent is allowed to do and how it is told to behave.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentDef {
    /// How the router addresses it.
    pub name: String,
    /// One line. This is what the router sees when choosing, so it is written
    /// to be matched against a request rather than read for understanding.
    pub description: String,
    /// Tool surface. Filtered twice, as roles were: never advertised to the
    /// model, and refused before dispatch if called anyway.
    pub tools: Vec<String>,
    /// Model tier this agent prefers. The router may override per call.
    pub tier: Tier,
    /// The system prompt body.
    pub prompt: String,
    /// What this agent may do to the roster.
    pub delegation: Delegation,
}

/// Who an agent may call.
///
/// Depth is one level: a tree of delegating specialists reaches forty agents
/// and ~600k tokens at three levels, which no task justifies. The single
/// exception is `librarian` — it is read-only so it cannot corrupt anything,
/// and it exists to keep a specialist's context clean. Forbidding it would
/// push that reading into the specialist's own window, which is the cost the
/// architecture is trying to avoid.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Delegation {
    /// May hand off and delegate to anyone. The router alone.
    Router,
    /// May call `librarian` only.
    #[default]
    Librarian,
    /// May not delegate at all. `librarian` itself, and the compactor.
    None,
}

impl Delegation {
    /// Whether this agent may delegate to `target`.
    pub fn may_call(self, target: &str) -> bool {
        match self {
            Self::Router => true,
            Self::Librarian => target == "librarian",
            Self::None => false,
        }
    }
}

impl AgentDef {
    /// Whether `tool` is within this agent's surface.
    pub fn allows(&self, tool: &str) -> bool {
        self.tools.iter().any(|t| t == tool)
    }

    /// Build from a frontmatter map plus the body after it.
    ///
    /// Returns None without a name: an unnamed agent cannot be addressed, so
    /// there is nothing useful to do with it.
    pub fn from_parts(front: &BTreeMap<String, String>, body: &str) -> Option<Self> {
        let name = front.get("name")?.trim().to_ascii_lowercase();
        if name.is_empty() {
            return None;
        }
        let tools: Vec<String> = front
            .get("tools")
            .map(|raw| {
                raw.split(',')
                    .map(|t| t.trim().to_ascii_lowercase())
                    .filter(|t| !t.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        Some(Self {
            name,
            description: front.get("description").cloned().unwrap_or_default(),
            tools,
            tier: front
                .get("tier")
                .and_then(|t| Tier::parse(t))
                .unwrap_or_default(),
            prompt: body.trim().to_owned(),
            delegation: front
                .get("delegation")
                .and_then(|d| match d.trim().to_ascii_lowercase().as_str() {
                    "router" => Some(Delegation::Router),
                    "librarian" => Some(Delegation::Librarian),
                    "none" => Some(Delegation::None),
                    _ => None,
                })
                .unwrap_or_default(),
        })
    }
}

/// The shipped roster, compiled in so enx works with no files on disk.
///
/// Discovered definitions override these by name, so a user who wants a
/// different `fe` writes one rather than editing this table.
pub fn builtin_agents() -> Vec<AgentDef> {
    const READ_ONLY: &[&str] = &["read", "glob", "grep", "todo"];
    const FULL: &[&str] = &["read", "write", "edit", "glob", "grep", "bash", "todo"];

    let make = |name: &str,
                description: &str,
                tools: &[&str],
                tier: Tier,
                delegation: Delegation,
                prompt: &str| AgentDef {
        name: name.to_owned(),
        description: description.to_owned(),
        tools: tools.iter().map(|t| (*t).to_owned()).collect(),
        tier,
        prompt: prompt.trim().to_owned(),
        delegation,
    };

    vec![
        make(
            "router",
            "routes work to a specialist; does not implement",
            &["read", "glob", "grep"],
            Tier::Cheap,
            Delegation::Router,
            ROUTER_PROMPT,
        ),
        // Domain — which part of the stack.
        make(
            "fe",
            "frontend, React/Vue/Svelte, CSS, components, accessibility, bundlers",
            FULL,
            Tier::Balanced,
            Delegation::Librarian,
            "You are a frontend specialist: component structure, styling, accessibility, \
             browser behaviour, and build tooling. When the project has components, read \
             them before adding one — match its conventions rather than importing your own. \
             An empty workspace has nothing to read: start writing.\n\
             Use the stack the project already has. For a new project with none stated, \
             choose the simplest that does the job: a static page is HTML and CSS, with \
             JavaScript only for behaviour it needs. Write each file once, complete, and \
             check it by reading it back or running the project's build. A static page \
             needs no server and no validator script to check.",
        ),
        make(
            "be",
            "backend, APIs, services, business logic, auth",
            FULL,
            Tier::Balanced,
            Delegation::Librarian,
            "You are a backend specialist: APIs, services, business logic, and auth. \
             Trace a request end to end before changing it. Treat error paths as part \
             of the feature, not an afterthought.",
        ),
        make(
            "db",
            "schema, migrations, queries, indexing, data modelling",
            FULL,
            Tier::Balanced,
            Delegation::Librarian,
            "You are a data specialist: schema, migrations, queries, and indexing. \
             A migration must be reversible or say plainly why it is not. Check what \
             an index costs on write before adding it for a read.",
        ),
        make(
            "devops",
            "CI/CD, containers, deploy, infrastructure, observability",
            FULL,
            Tier::Balanced,
            Delegation::Librarian,
            "You are an infrastructure specialist: CI, containers, deployment, and \
             observability. Prefer changes that fail loudly in CI over ones that fail \
             quietly in production.",
        ),
        make(
            "mobile",
            "iOS, Android, React Native, Flutter, native APIs",
            FULL,
            Tier::Balanced,
            Delegation::Librarian,
            "You are a mobile specialist: platform APIs, lifecycle, and the constraints \
             of a device — memory, battery, and intermittent network.",
        ),
        make(
            "systems",
            "low-level, memory, concurrency, FFI, binary formats",
            FULL,
            Tier::Strong,
            Delegation::Librarian,
            "You are a systems specialist: memory, concurrency, FFI, and binary \
             formats. Be explicit about ownership, lifetimes, and what happens under \
             contention. Unsafe code needs a stated invariant.",
        ),
        // Cross-cutting — applies to any domain.
        make(
            "librarian",
            "gathers material: reads widely, returns excerpts with paths and lines",
            READ_ONLY,
            Tier::Cheap,
            Delegation::None,
            "You gather material. Read what you are asked about and return the \
             relevant excerpts with their paths and line numbers.\n\
             Do NOT draw conclusions, propose changes, or answer the underlying \
             question — that is the caller's job. Return what is there, filtered. \
             If nothing matches, say so rather than returning the nearest thing.",
        ),
        make(
            "research",
            "answers questions about a codebase; read-only, never modifies",
            &["read", "glob", "grep", "fetch", "todo"],
            Tier::Balanced,
            Delegation::Librarian,
            "You investigate and report. Map the ground with glob/grep, then read the \
             ranges that answer the question.\n\
             Separate what you verified from what you inferred, and mark inferences \
             plainly. Give paths and line numbers so the next step is actionable.",
        ),
        make(
            "review",
            "reads diffs and code for defects; never edits",
            &["read", "glob", "grep", "bash", "todo"],
            Tier::Strong,
            Delegation::Librarian,
            "You review code. Report defects: what breaks, under what input, and where.\n\
             You do not edit — a review that rewrites the code is not a review. Rank \
             by consequence, not by how easy the fix is. Say plainly when you find \
             nothing wrong rather than inventing a concern.",
        ),
        make(
            "test",
            "writes and fixes tests, reproduces reported failures",
            FULL,
            Tier::Balanced,
            Delegation::Librarian,
            "You write and fix tests. Reproduce a reported failure before fixing it, \
             and make the test fail for the stated reason before you make it pass.\n\
             A test that cannot fail is worse than no test.",
        ),
        make(
            "docs",
            "READMEs, changelogs, API docs, comments",
            &["read", "write", "edit", "glob", "grep", "todo"],
            Tier::Balanced,
            Delegation::Librarian,
            "You write documentation. Read the code you are describing before \
             describing it; do not infer behaviour from names.\n\
             No filler sections, no marketing language, no invented statistics. \
             Structure follows the content.",
        ),
        make(
            "security",
            "auth, secrets, injection, dependency risk",
            &["read", "glob", "grep", "bash", "todo"],
            Tier::Strong,
            Delegation::Librarian,
            "You audit for security problems: authentication, secrets, injection, and \
             dependency risk.\n\
             Describe the class of problem and where it is, not a working exploit. \
             Rank by what an attacker actually gains.",
        ),
        make(
            "perf",
            "profiling, hot paths, benchmarks",
            FULL,
            Tier::Strong,
            Delegation::Librarian,
            "You work on performance. Measure before and after — a change without a \
             measurement is a guess.\n\
             State the workload you measured. An optimisation that helps one shape of \
             input and hurts another is a trade, so say which.",
        ),
        make(
            "general",
            "anything outside the specialists' scope",
            FULL,
            Tier::Balanced,
            Delegation::Librarian,
            "You handle work that fits no specialist. Establish facts with tools before \
             acting, edit surgically, and verify what you changed.",
        ),
        // Not routed to; invoked by the loop.
        make(
            "compactor",
            "condenses a conversation without losing the thread",
            &[],
            Tier::Cheap,
            Delegation::None,
            COMPACTOR_PROMPT,
        ),
    ]
}

/// The router's prompt.
///
/// Every judgement the design defers lands here — which specialist, handoff or
/// delegate, which tier, how to split parallel work. A vague prompt produces a
/// router that picks `general` and `strong` for everything, and the roster is
/// decoration.
const ROUTER_PROMPT: &str = "\
You route work to specialists. You do not implement.

CHOOSING A SPECIALIST
Pick the agent whose description matches the work, not the words. \"The login \
page is broken\" is `fe` if the page renders wrong and `be` if the request \
fails — read enough to tell which, then choose.
When the work is clearly one part of the stack, use the domain agent. Use a \
cross-cutting agent when the work spans domains or the domain does not matter: \
`review` for a PR touching several areas, `docs` for a changelog, `security` \
for an audit.
`general` is for work that fits nothing above. Reaching for it often means the \
roster is missing an agent — say so rather than quietly absorbing the task.

HANDOFF OR DELEGATE
Delegate when the work is a piece of something larger and you will carry on \
afterwards. The specialist starts clean, returns a summary, and its context is \
discarded. This is the cheaper path and the default.
Hand off when the whole request belongs to one specialist and the user will \
keep talking to them. You step out; they finish.
If unsure, delegate. A delegation that turns out to be the whole task costs one \
summary; a handoff that turns out to be a fragment leaves the user talking to \
the wrong specialist.

CHOOSING A TIER
  cheap     mechanical work against a clear specification: rename, format,
            apply a stated pattern, gather files
  balanced  normal implementation: a feature with known shape, a bug with a
            known cause, tests for existing code
  strong    work needing judgement: unclear cause, design decisions,
            unfamiliar code, anything where a wrong answer is expensive
Start one tier lower than feels right. A `balanced` attempt that fails costs \
less than a `strong` attempt that was never needed, and the ladder promotes on \
failure anyway.

SPLITTING PARALLEL WORK
Two delegations may run together when neither writes what the other writes. \
Declare `writes` honestly: too narrow and the specialist is refused mid-task, \
too wide and it blocks work that could have run alongside.
Prefer splitting by area, not by activity. `fe` on the components and `be` on \
the endpoints can run together; \"implement\" and \"test\" on the same files \
cannot.

READING BEFORE ROUTING
Most requests name their kind of work: \"build a portfolio page\" is `fe`, \
\"this query is slow\" is `db`. Route those straight away, without reading \
anything. Read only when the request leaves the specialist genuinely open, and \
then one or two small reads at most. The specialist reads the files it needs \
itself, so do not read for it, and do not paste file contents into a brief.

WRITING THE BRIEF
Say what the user wants and any constraint they stated, in a few lines. Do \
not plan the specialist's steps or invent requirements the user did not give.

WHEN DETAILS ARE OPEN
A request that leaves details open is not a reason to stop and ask. \
\"A simple portfolio\" does not say whose, or in which stack: choose the \
plainest thing that does the job, with placeholder content marked as such, \
say so in the brief, and delegate. A placeholder takes the user seconds to \
change; a list of questions before anything exists costs them a round trip. \
Ask only when the work cannot start without the answer, such as which of two \
existing projects to change. The workspace path is where to work, not \
information about the task: do not read meaning into a folder's name.

AFTER A DELEGATION
Answer the user from the report. Do not re-read the specialist's files to \
check its work unless the report leaves something the user asked about \
unclear.

WHAT YOU MUST NOT DO
Do not do the work. You have read, glob, and grep so you can classify the \
request — not so you can answer it. If you are reading a third file to decide, \
you already have enough to delegate.
Do not chain delegations to build a result yourself. Delegate the task, not \
each step of it; the specialist plans its own steps.
Do not present a specialist's work as your own. Report what came back.
";

/// The compactor's prompt.
///
/// Used both for periodic compaction and for writing a handover when one
/// specialist takes over from another. The job is the same: condense without
/// losing the thread.
const COMPACTOR_PROMPT: &str = "\
You compact a coding-agent conversation so it fits the model's context window \
while remaining usable on the next turn.

Produce ONLY these sections, as tight factual bullets:
  Goal       what the user is trying to achieve
  Decisions  choices already made — libraries, patterns, approaches — so they
             are not relitigated
  Files      which files were read, created, or edited, and their state
  Commands   what was run and what it returned, outcome only
  Open       what is unfinished or blocked

Do not restate every tool call. Do not include file contents or command output \
verbatim: name the file and say what happened to it. Anyone who needs the \
contents can read the file, and a file read before it was edited is now wrong.
Do not apologise for summarising, and do not add commentary.
";

#[cfg(test)]
mod tests {
    use super::*;

    fn roster() -> Vec<AgentDef> {
        builtin_agents()
    }

    fn by_name(name: &str) -> AgentDef {
        roster()
            .into_iter()
            .find(|a| a.name == name)
            .unwrap_or_else(|| panic!("{name} should ship"))
    }

    #[test]
    fn the_router_cannot_do_the_work() {
        let router = by_name("router");
        // Reading is how it classifies; writing would let it absorb the task,
        // and then the roster is never used.
        assert!(router.allows("read"));
        assert!(!router.allows("write"));
        assert!(!router.allows("edit"));
        assert!(!router.allows("bash"));
    }

    #[test]
    fn only_the_router_may_delegate_freely() {
        for agent in roster() {
            match agent.name.as_str() {
                "router" => assert_eq!(agent.delegation, Delegation::Router),
                // Read-only gatherers and the compactor are leaves.
                "librarian" | "compactor" => assert_eq!(agent.delegation, Delegation::None),
                _ => assert_eq!(
                    agent.delegation,
                    Delegation::Librarian,
                    "{} should only reach librarian",
                    agent.name
                ),
            }
        }
    }

    /// Depth is one level plus the librarian exception; nothing else may chain.
    #[test]
    fn delegation_depth_is_bounded() {
        assert!(Delegation::Router.may_call("fe"));
        assert!(Delegation::Librarian.may_call("librarian"));
        assert!(!Delegation::Librarian.may_call("be"));
        assert!(!Delegation::None.may_call("librarian"));
    }

    #[test]
    fn read_only_agents_cannot_mutate() {
        for name in ["librarian", "research", "review", "security"] {
            let agent = by_name(name);
            assert!(!agent.allows("write"), "{name} must not write");
            assert!(!agent.allows("edit"), "{name} must not edit");
        }
    }

    /// `review` and `security` need to run commands — a test suite, an audit
    /// tool — without being able to change what they are inspecting.
    #[test]
    fn inspectors_may_run_commands_but_not_edit() {
        for name in ["review", "security"] {
            let agent = by_name(name);
            assert!(agent.allows("bash"), "{name} needs to run things");
            assert!(!agent.allows("edit"));
        }
    }

    #[test]
    fn every_agent_has_a_description_to_route_on() {
        for agent in roster() {
            assert!(
                !agent.description.trim().is_empty(),
                "{} has nothing for the router to match against",
                agent.name
            );
            assert!(
                !agent.prompt.trim().is_empty(),
                "{} has no prompt",
                agent.name
            );
        }
    }

    #[test]
    fn names_are_unique() {
        let mut names: Vec<String> = roster().into_iter().map(|a| a.name).collect();
        let before = names.len();
        names.sort();
        names.dedup();
        assert_eq!(before, names.len(), "duplicate agent name in the roster");
    }

    #[test]
    fn the_ladder_walks_down_and_stops() {
        assert_eq!(Tier::Strong.lower(), Some(Tier::Balanced));
        assert_eq!(Tier::Balanced.lower(), Some(Tier::Cheap));
        assert_eq!(Tier::Cheap.lower(), None, "cheap is the floor");
    }

    #[test]
    fn tier_parsing_accepts_the_aliases_people_type() {
        assert_eq!(Tier::parse("STRONG"), Some(Tier::Strong));
        assert_eq!(Tier::parse(" fast "), Some(Tier::Cheap));
        assert_eq!(Tier::parse("medium"), Some(Tier::Balanced));
        assert_eq!(Tier::parse("enormous"), None);
    }

    #[test]
    fn a_definition_parses_from_frontmatter() {
        let mut front = BTreeMap::new();
        front.insert("name".into(), "Reviewer".into());
        front.insert("description".into(), "checks things".into());
        front.insert("tools".into(), "read, grep , bash".into());
        front.insert("tier".into(), "strong".into());
        let agent = AgentDef::from_parts(&front, "\n  body text  \n").expect("parses");
        assert_eq!(agent.name, "reviewer", "names are normalised");
        assert_eq!(agent.tools, vec!["read", "grep", "bash"]);
        assert_eq!(agent.tier, Tier::Strong);
        assert_eq!(agent.prompt, "body text");
    }

    /// An agent with no name cannot be addressed, so there is nothing to do
    /// with it; the loader reports it rather than inventing one.
    #[test]
    fn a_nameless_definition_is_rejected() {
        let front = BTreeMap::new();
        assert!(AgentDef::from_parts(&front, "body").is_none());
    }

    #[test]
    fn a_definition_without_a_tier_is_balanced() {
        let mut front = BTreeMap::new();
        front.insert("name".into(), "x".into());
        let agent = AgentDef::from_parts(&front, "b").unwrap();
        assert_eq!(agent.tier, Tier::Balanced);
        assert_eq!(agent.delegation, Delegation::Librarian);
    }
}
