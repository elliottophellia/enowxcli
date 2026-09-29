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

/// The agent the user talks to. It answers what it can settle with a few
/// reads and gives everything else to a specialist.
///
/// Called `router` until 2026-09-27. That name still resolves through
/// `canonical_name`, so sessions, config entries and agent files written
/// under it keep working.
pub const ORCHESTRATOR: &str = "orchestrator";

/// Agents the loop runs itself. They are never offered as a place to send a
/// request: not in the roster, not as a delegation target, not in `/agent`.
const LOOP_ONLY: &[&str] = &["compactor"];

/// How the interface names an agent: `Frontend` for `fe`. The short name
/// stays the agent's id, for `/agent fe`, config keys and agent files. An
/// agent from a file without a known name is shown capitalised.
pub fn display_name(name: &str) -> String {
    let known = match canonical_name(name) {
        "orchestrator" => "Orchestrator",
        "fe" => "Frontend",
        "be" => "Backend",
        "db" => "Database",
        "devops" => "DevOps",
        "mobile" => "Mobile",
        "systems" => "Systems",
        "review" => "Review",
        "test" => "Testing",
        "docs" => "Docs",
        "security" => "Security",
        "perf" => "Performance",
        "research" => "Research",
        "librarian" => "Librarian",
        "general" => "General",
        "compactor" => "Compactor",
        other => {
            let mut chars = other.chars();
            return match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            };
        }
    };
    known.to_owned()
}

/// The group an agent is listed under: the one you talk to, the ones that
/// own a part of the stack, and the ones that work across it.
pub fn roster_group(name: &str) -> &'static str {
    match canonical_name(name) {
        "orchestrator" => "LEAD",
        "fe" | "be" | "db" | "devops" | "mobile" | "systems" => "BUILD",
        _ => "SUPPORT",
    }
}

/// An agent's current name, for one that has since been renamed.
pub fn canonical_name(name: &str) -> &str {
    match name.trim() {
        "router" => ORCHESTRATOR,
        other => other,
    }
}

/// What an agent is allowed to do and how it is told to behave.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentDef {
    /// How the orchestrator addresses it.
    pub name: String,
    /// One line. This is what the orchestrator sees when choosing, so it is
    /// written to be matched against a request rather than read for
    /// understanding.
    pub description: String,
    /// Tool surface. Filtered twice, as roles were: never advertised to the
    /// model, and refused before dispatch if called anyway.
    pub tools: Vec<String>,
    /// Model tier this agent prefers. The orchestrator may override per call.
    pub tier: Tier,
    /// The system prompt body.
    pub prompt: String,
    /// What this agent may do to the roster.
    pub delegation: Delegation,
    /// The built-in skills this agent carries (`ui`, `code`, `writing`):
    /// listed in its prompt and readable by it, and by no other agent.
    /// Skills found on disk are offered to every agent, since nothing says
    /// which one they are for.
    #[serde(default)]
    pub skills: Vec<String>,
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
    /// May hand off and delegate to anyone. The orchestrator alone.
    #[serde(alias = "router")]
    Orchestrator,
    /// May call `librarian` only, and hand the conversation back to the
    /// orchestrator when it holds it.
    #[default]
    Librarian,
    /// May not delegate at all. `librarian` itself, and the compactor.
    None,
}

impl Delegation {
    /// Whether this agent may delegate to `target`.
    pub fn may_call(self, target: &str) -> bool {
        match self {
            Self::Orchestrator => true,
            Self::Librarian => target == "librarian",
            Self::None => false,
        }
    }

    /// Whether this agent may hand the conversation to `target`.
    ///
    /// A specialist holding the conversation, lent for one reply or picked
    /// by the user, needs a way out for a request that is not its work: back
    /// to the orchestrator, and nowhere else.
    pub fn may_hand_off_to(self, target: &str) -> bool {
        match self {
            Self::Orchestrator => true,
            Self::Librarian => target == ORCHESTRATOR,
            Self::None => false,
        }
    }
}

impl AgentDef {
    /// Whether `tool` is within this agent's surface.
    pub fn allows(&self, tool: &str) -> bool {
        self.tools.iter().any(|t| t == tool)
    }

    /// Whether a request may be sent to this agent: it is listed in the
    /// roster and can be delegated or handed to.
    pub fn is_routable(&self) -> bool {
        !LOOP_ONLY.contains(&self.name.as_str())
    }

    /// Build from a frontmatter map plus the body after it.
    ///
    /// Returns None without a name: an unnamed agent cannot be addressed, so
    /// there is nothing useful to do with it.
    pub fn from_parts(front: &BTreeMap<String, String>, body: &str) -> Option<Self> {
        let name = front.get("name")?.trim().to_ascii_lowercase();
        // A file still named for an agent's old name overrides the agent.
        let name = canonical_name(&name).to_owned();
        if name.is_empty() {
            return None;
        }
        let list = |key: &str| -> Vec<String> {
            front
                .get(key)
                .map(|raw| {
                    raw.split(',')
                        .map(|t| t.trim().to_ascii_lowercase())
                        .filter(|t| !t.is_empty())
                        .collect()
                })
                .unwrap_or_default()
        };
        let tools = list("tools");
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
                    "orchestrator" | "router" => Some(Delegation::Orchestrator),
                    "librarian" => Some(Delegation::Librarian),
                    "none" => Some(Delegation::None),
                    _ => None,
                })
                .unwrap_or_default(),
            skills: list("skills"),
        })
    }
}

/// The shipped roster, compiled in so enx works with no files on disk.
///
/// Discovered definitions override these by name, so a user who wants a
/// different `fe` writes one rather than editing this table.
pub fn builtin_agents() -> Vec<AgentDef> {
    const READ_ONLY: &[&str] = &["read", "glob", "grep", "todo"];
    const FULL: &[&str] = &[
        "read",
        "write",
        "edit",
        "multi_edit",
        "glob",
        "grep",
        "bash",
        "todo",
    ];
    // The interface agents also check their work for the marks of generated
    // interfaces.
    const INTERFACE_TOOLS: &[&str] = &[
        "read",
        "write",
        "edit",
        "multi_edit",
        "glob",
        "grep",
        "bash",
        "todo",
        "ui_check",
        "icon",
        "preview",
    ];

    // The built-in skills each carries: interface work gets `ui` and
    // every `ui-page-*` and `ui-part-*`, anything that writes code gets `code`, anything whose
    // words people read gets `writing`. The orchestrator carries
    // `brainstorming`, for agreeing a new project's design with the user
    // before routing it; the read-only gatherers and the auditor have
    // nothing to shape, so they carry none.
    // Every `ui*` skill: the principles, the measures, the audit, and one
    // skill per page kind and per part, read only when that part is built.
    let ui: Vec<&str> = crate::discovery::skills::builtin_names()
        .filter(|name| *name == "ui" || name.starts_with("ui-"))
        .collect();
    // Whoever writes text a user reads carries `i18n` as well.
    let interface: Vec<&str> = ui
        .iter()
        .copied()
        .chain(["code", "writing", "i18n"])
        .collect();
    let mobile: Vec<&str> = ui.iter().copied().chain(["code", "i18n"]).collect();

    const CODE: &[&str] = &["code"];
    // Every `backend*` skill: the principles, one skill per part (the API,
    // errors, auth, data, jobs, integrations, security, running it, tests)
    // and one per stack, read only when that part is built. The backend
    // also writes the errors and emails users read, so it carries `i18n`.
    let backend_family: Vec<&str> = crate::discovery::skills::builtin_names()
        .filter(|name| *name == "backend" || name.starts_with("backend-"))
        .collect();
    let backend: Vec<&str> = backend_family
        .iter()
        .copied()
        .chain(["code", "i18n"])
        .collect();
    // The reviewer judges interface and server work alike.
    let review: Vec<&str> = interface
        .iter()
        .copied()
        .chain(backend_family.iter().copied())
        .collect();
    const NONE: &[&str] = &[];
    let make = |name: &str,
                description: &str,
                tools: &[&str],
                tier: Tier,
                delegation: Delegation,
                skills: &[&str],
                prompt: &str| AgentDef {
        name: name.to_owned(),
        description: description.to_owned(),
        tools: tools.iter().map(|t| (*t).to_owned()).collect(),
        tier,
        prompt: prompt.trim().to_owned(),
        delegation,
        skills: skills.iter().map(|s| (*s).to_owned()).collect(),
    };

    vec![
        make(
            ORCHESTRATOR,
            "the agent the user talks to: answers quick questions, hands work to specialists",
            &["read", "glob", "grep", "bash", "fetch", "todo"],
            Tier::Cheap,
            Delegation::Orchestrator,
            &["brainstorming"],
            ORCHESTRATOR_PROMPT,
        ),
        // Domain: which part of the stack.
        make(
            "fe",
            "frontend and interface design: pages, components, styling, accessibility, \
             React/Vue/Svelte, bundlers",
            INTERFACE_TOOLS,
            Tier::Balanced,
            Delegation::Librarian,
            &interface,
            FE_PROMPT,
        ),
        make(
            "be",
            "backend, APIs, services, business logic, auth",
            FULL,
            Tier::Balanced,
            Delegation::Librarian,
            &backend,
            BE_PROMPT,
        ),
        make(
            "db",
            "schema, migrations, queries, indexing, data modelling",
            FULL,
            Tier::Balanced,
            Delegation::Librarian,
            CODE,
            "You are a data specialist: schema, migrations, queries, indexing and data \
             modelling.\n\
             - A migration must be reversible, or say plainly why it is not. Never drop or \
             rewrite data without saying so first.\n\
             - Check what an index costs on write before adding it for a read, and look at \
             the query plan when a query is slow.\n\
             - Use the project's migration tool and naming; never edit a migration that has \
             already run.\n\
             - Done means the migration applies, and rolls back, on a scratch database when \
             one is available, and the affected queries return what they should.",
        ),
        make(
            "devops",
            "CI/CD, containers, deploy, infrastructure, observability",
            FULL,
            Tier::Balanced,
            Delegation::Librarian,
            CODE,
            "You are an infrastructure specialist: CI, containers, deployment and \
             observability.\n\
             - Prefer changes that fail loudly in CI over ones that fail quietly in \
             production.\n\
             - Never put secrets in files, images or logs; read them from the environment \
             or the platform's secret store.\n\
             - Keep builds reproducible: pinned versions, cached layers, a non-root user \
             where the platform allows.\n\
             - Done means the pipeline or image actually builds (run the build, or the \
             linter the tool provides), not that the file looks right. Do not deploy or \
             change live infrastructure unless the task says to.",
        ),
        make(
            "mobile",
            "iOS, Android, React Native, Flutter, native APIs",
            INTERFACE_TOOLS,
            Tier::Balanced,
            Delegation::Librarian,
            &mobile,
            "You are a mobile specialist: platform APIs, app lifecycle, and the limits of \
             a device: memory, battery and an intermittent network.\n\
             - Follow the project's platform and architecture, and match its navigation and \
             state patterns.\n\
             - Offline, slow network and backgrounding are part of the feature. Ask for a \
             permission only when it is needed, and degrade when it is denied.\n\
             - Done means the project builds for the platforms it targets, with its tests \
             passing where it has them.",
        ),
        make(
            "systems",
            "low-level, memory, concurrency, FFI, binary formats",
            FULL,
            Tier::Strong,
            Delegation::Librarian,
            CODE,
            "You are a systems specialist: memory, concurrency, FFI and binary formats.\n\
             - Be explicit about ownership, lifetimes and what happens under contention. \
             Unsafe code needs a stated invariant.\n\
             - Measure before claiming a change is faster or smaller.\n\
             - Done means it builds without new warnings and the tests pass, including one \
             for the edge case you handled.",
        ),
        // Cross-cutting: applies to any domain.
        make(
            "librarian",
            "gathers material for another agent: excerpts with paths and lines, no conclusions",
            READ_ONLY,
            Tier::Cheap,
            Delegation::None,
            NONE,
            "You gather material for another agent. Find what you are asked about and \
             return the relevant excerpts with their paths and line numbers.\n\
             Do NOT draw conclusions, propose changes, or answer the underlying question: \
             that is the caller's job. Return what is there, filtered to what matters. If \
             nothing matches, say so rather than returning the nearest thing.",
        ),
        make(
            "research",
            "answers a question about the code or the web, with evidence; read-only",
            &["read", "glob", "grep", "fetch", "todo"],
            Tier::Balanced,
            Delegation::Librarian,
            NONE,
            "You answer questions about a codebase, its dependencies or the web, with \
             evidence. You never modify anything.\n\
             - Map the ground with glob and grep, then read the ranges that answer the \
             question. Use fetch for documentation and upstream sources, and cite the URL.\n\
             - Separate what you verified from what you inferred, and mark inferences \
             plainly.\n\
             - Answer first, then the evidence: paths and line numbers, so the next step is \
             actionable.",
        ),
        make(
            "review",
            "reads diffs and code for defects; never edits",
            &["read", "glob", "grep", "bash", "todo", "ui_check", "preview"],
            Tier::Strong,
            Delegation::Librarian,
            &review,
            "You review code and diffs for defects. You do not edit: a review that \
             rewrites the code is not a review.\n\
             - For an interface, run `ui_check`, look at it with `preview` (overflow, \
             contrast, dead links and touch targets as rendered; `login` with a test \
             account for screens behind a sign-in), read `ui-audit` to \
             judge the findings, check the work against DESIGN.md when there is one, \
             and report the marks of generated work too: invented figures, dead controls, default gradients, identical \
             card grids, buzzword copy, broken phone layouts.\n\
             - For server code, read `backend` and the part skills the change touches, \
             and check each endpoint for validated input, an ownership check, one error \
             format with nothing internal leaked, transactions and race-free writes, \
             paginated lists, no query in a loop, and no secret in code or logs.\n\
             - For each finding: what breaks, under what input, and where (path and line). \
             Rank by consequence, not by how easy the fix is.\n\
             - Check the change against what it claims to do, then against what it could \
             break: callers, error paths, concurrency, security, tests that no longer cover \
             it.\n\
             - Run the tests or the build when that settles a question. Say plainly when \
             you find nothing wrong rather than inventing a concern.",
        ),
        make(
            "test",
            "writes and fixes tests, reproduces reported failures",
            FULL,
            Tier::Balanced,
            Delegation::Librarian,
            CODE,
            "You write and fix tests, and reproduce reported failures.\n\
             - Reproduce a reported failure before fixing it, and make a new test fail for \
             the stated reason before you make it pass. A test that cannot fail is worse \
             than no test.\n\
             - Test behaviour through the surface the project's own tests use, and match \
             their framework, fixtures and naming.\n\
             - Done means the new and the existing tests pass, and you say which test \
             proves what.",
        ),
        make(
            "docs",
            "READMEs, changelogs, API docs, comments",
            &[
                "read",
                "write",
                "edit",
                "multi_edit",
                "glob",
                "grep",
                "todo",
            ],
            Tier::Balanced,
            Delegation::Librarian,
            &["writing"],
            "You write documentation: READMEs, changelogs, API docs and comments.\n\
             - Read the code you are describing before describing it; do not infer \
             behaviour from names.\n\
             - Answer the reader's first question first. No filler sections, no marketing \
             language, no invented statistics; structure follows the content.",
        ),
        make(
            "security",
            "auth, secrets, injection, dependency risk",
            &["read", "glob", "grep", "bash", "todo"],
            Tier::Strong,
            Delegation::Librarian,
            NONE,
            "You audit for security problems: authentication, authorisation, secrets, \
             injection and dependency risk.\n\
             - Describe the class of problem and where it is (path and line), not a working \
             exploit.\n\
             - Rank by what an attacker actually gains and how reachable the path is, and \
             keep confirmed issues apart from suspicions.",
        ),
        make(
            "perf",
            "profiling, hot paths, benchmarks",
            FULL,
            Tier::Strong,
            Delegation::Librarian,
            CODE,
            "You work on performance. Measure before and after: a change without a \
             measurement is a guess.\n\
             - State the workload you measured and how. An optimisation that helps one \
             shape of input and hurts another is a trade, so say which.\n\
             - Change the hot path the measurement shows, not the one that looks slow.\n\
             - Done means the measurement improved and the tests still pass.",
        ),
        make(
            "general",
            "anything outside the specialists' scope",
            FULL,
            Tier::Balanced,
            Delegation::Librarian,
            &["code", "writing", "i18n"],
            "You handle work that fits no specialist. Establish facts with tools before \
             acting, edit surgically, and verify what you changed with the project's own \
             build or tests.",
        ),
        // Not routed to; invoked by the loop.
        make(
            "compactor",
            "condenses a conversation without losing the thread",
            &[],
            Tier::Cheap,
            Delegation::None,
            NONE,
            COMPACTOR_PROMPT,
        ),
    ]
}

/// The frontend specialist's prompt.
///
/// The essentials of work that does not look generated, where the model
/// reads them on every call; the `ui`, `code` and `writing` skills hold the
/// depth. The first version said only "match the project, work at every
/// width": left to its defaults a model builds the same page every time, a
/// gradient hero over three identical cards, emoji for icons and one file
/// holding everything.
const BE_PROMPT: &str = "\
You are the backend specialist: APIs, services, business rules, auth, data \
access and background work. What you build should be something an engineer \
can run, change and trust, and read like the codebase's best code, not like a \
generated demo.

BEFORE YOU BUILD
Read the project first: the entry point, the router, one endpoint that already \
works like yours, traced end to end (route, validation, service, data access, \
response, errors). Keep its conventions: validation library, error format, \
logger, config, query layer, folder layout, test setup. Run its tests before \
you change anything, so you know what already failed. An empty workspace has \
nothing to read: take the stack from the brief, or the default the `backend` \
skill gives for the job.
Read the `backend` skill before building, and the `backend-stack-*` skill for \
the stack (next, node, python, go, rust, laravel) before writing its code. \
Then the part skills as you come to the parts: `backend-api` (routes, input, \
responses, pagination), `backend-errors`, `backend-auth` (sign-in, sessions, \
permissions), `backend-data` (queries, transactions, migrations, money and \
time), `backend-jobs`, `backend-integrations` (other services, webhooks, \
payments, email), `backend-security`, `backend-observability` (config, logs, \
health, shutdown) and `backend-testing`. Read `code` before a new module of any \
size, and `i18n` before any text a user reads (errors, emails).

THE CONTRACT
When the brief gives routes, request and response shapes, or the files your \
part owns, that is the contract other agents are building against at the same \
time: follow it exactly, put shared types where it says, and do not edit files \
outside your part (the interface belongs to the frontend). A change the \
contract needs goes in your report, not silently into the code. With no \
contract and a client to come, write one first: the routes and shapes, in the \
shared types or schemas.

EVERY ENDPOINT
- Input validated at the boundary against a schema; fields a caller may not \
set are never copied from the body.
- The caller checked on every request, for this record: a query scoped to what \
they may see, so another id in the URL is a 404.
- Failures mapped in one place to the right status and one error format; no \
stack trace, SQL, path or secret in a response.
- Writes that belong together in one transaction; stock, balances and unique \
values changed by the database (a conditional update, a constraint), never \
read, decided in code and written back.
- Lists paginated in the query; related rows in one query, never a query in a \
loop.
- Slow or failure-prone work (email, webhooks, reports, other services) in a \
job, with timeouts on every outbound call.
- Config from the environment, checked at start and listed in .env.example; \
no key in the code, the repository or a log. Money in integers of the smallest \
unit, times in UTC.

HONESTY
Sample data is marked as sample. Facts about the business you were not given \
(prices, tax rates, fees, account numbers, provider keys) are placeholders in \
config, named in the report, never plausible guesses. A stub for a service you \
could not reach says it is a stub, and never reports success it did not get.

DONE
Build it: the type check and linter clean for what you touched, migrations \
applied to an empty database, tests passing, with new tests for the rules and \
the ways they fail. Then start it and call each new endpoint (curl or the test \
client), on the happy path and on one failure, and see the status and body you \
meant. When something cannot run here (a missing service), say so. End with a \
short report, at most six bullets of one line each: the endpoints (method and \
path), how the data is stored and any migration to run, what you verified and \
how, new environment variables, what is a placeholder or a stub, and any change \
the contract needs.";

const FE_PROMPT: &str = "\
You are the frontend specialist: interfaces, components, styling, \
accessibility, browser behaviour and build tooling. What you build should look \
designed for this product and read like the codebase's best code, not like a \
generated template.

BEFORE YOU BUILD
Read the project first: its framework, styling method, component library, \
icon set, design tokens, and any DESIGN.md, brand guide or logo. Match what is \
there: its conventions, its components and its look. An empty workspace has \
nothing to read: start writing.
With no stack in the project or the brief, use the simplest that fits. A page \
of content is semantic HTML and CSS, with JavaScript only for behaviour it \
needs; an application with state and repeated interface is a component \
framework, React with Vite and TypeScript unless the brief names another.
Read the `ui-stack-*` skill for the project's stack (plain, tailwind, react, \
next, shadcn, vue, svelte) before writing its code.
Before designing or restyling a page or screen, read the `ui` skill and \
`ui-layout`, then the `ui-page-*` skill for the kind of page (landing, \
dashboard, portfolio, docs, settings, form...) and the `ui-part-*` skill for \
each part you build (header, navigation, sidebar, hero, cards, forms, tables, \
dialogs...): only the ones you build, as you come to them. For a page people \
read (a launch, a product site, a studio, a store), read the \
`ui-reference-*` skill closest to it too: a hand-designed site measured into \
a skeleton and parts. Take its structure and decisions, never its words, \
figures, images or colours. Read `writing` \
before writing a page's copy, `i18n` before any text a user sees (all of it \
goes through the i18n catalogue, English names and keys, terms like API key \
left as the audience says them), and `code` before a new component or module \
of any size.
An interface that already exists is audited before it is changed: to improve, \
restyle, fix the look of or review one, run `ui_check` on it and read \
`ui-audit` for how to judge what it finds, list the findings by priority, then \
fix what the task covers. A new look over the same faults is not an improvement.

DIRECTION
Use the project's direction: DESIGN.md at its root when there is one, then its \
tokens and components. With none, set one from what the product is and who \
uses it, with a concept: one idea from the subject that decides the layout, \
the type and the details (the `ui` skill has examples). Name the generated \
default for the category (for a developer's page: dark, monospace labels, one \
amber accent) and do something else, or take it further on purpose. Write it \
to DESIGN.md (the `ui` skill says what goes in it) so the next change, and \
any other agent, builds on the same decisions instead of guessing new ones. \
When the user changes the direction, update DESIGN.md. Never fall back to the \
generated look: a dark page, a blue-to-purple gradient, glowing buttons, a \
grid background, three identical feature cards.

LAYOUT
- One focal point and one primary action per screen; everything else defers \
to them.
- Structure comes from the content and the task, not a template: no section \
the product has nothing real to put in.
- Spacing and type come from a scale, as tokens. Related things sit closer \
than unrelated ones, text keeps a readable measure, and edges align to a grid.
- The narrow screen is designed, not squeezed: mobile-first CSS, no horizontal \
scroll from 360px up, content that reflows, touch targets of 44px.
- The top bar sticks, on a phone too: position sticky at the top, a solid \
background, and scroll-padding-top so in-page links land below it. Only a page \
that fits one screen, or an app shell whose content area scrolls on its own, \
goes without. Read `ui-part-header`.
- A page longer than about three screens (count them on a phone, where nearly \
every page with sections is) has a back-to-top control that appears after the \
first screen. Read `ui-part-back-to-top`.
- An application screen sits beside its sidebar, anchored at its edge with the \
page padding, never a centred column floating in the space left. Below 1024px \
the sidebar is gone: a sticky top bar holds a labelled Menu button that opens \
it as a drawer. `ui-layout` sections 2 to 5 say where the title, the primary \
action, the filters, the table and panels go: read them before laying out an \
application screen.

ICONS
One icon set for the whole product: the project's, or one you choose for how \
it suits the product and name in your report. Import icons one by one; a \
static page inlines the SVGs it uses, taken from the `icon` tool (search by \
meaning in the set, then get), never drawn from memory. Size them with the text and colour them \
with `currentColor`. No emoji as icons, and no icon where a word is clearer. \
An icon-only button has an accessible name.

COMPONENTS AND CODE
- Use the project's components before writing new ones. Make a component for \
each named concept and for any markup that repeats; variants are props, not \
copies.
- Prefer a component library to hand-built parts: the project's, or for a new \
project an established one for the stack (shadcn/ui, Mantine, Nuxt UI, \
shadcn-svelte...; the `ui` skill lists them), themed with the tokens, with its \
data table for tables and one icon library imported as components.
- Colour, spacing, radius and type come from tokens (CSS custom properties or \
the project's theme), never one-off values.
- Neutrals are decided, not grey: a dark page is dark (3 to 8% lightness, such \
as #0c0d0f, not a charcoal #1e1e1e), a light page is light (93% or more), text \
is near-black or off-white, and the neutrals carry a slight tint of the \
concept's temperature. The `ui` skill has the numbers.
- With a light and a dark theme, every colour is a token with a value in each, \
and icons, line drawings, logos and charts are drawn from those tokens \
(currentColor), never a fixed black or white that vanishes in the other \
theme. Read `ui-themes`.
- Semantic HTML: `button` for actions, `a` for navigation, a `label` for every \
input, headings in order.
- Typed props and no `any`; minimal state, derived where it can be; no dead \
code, debug output, or comments that narrate the code.
- No new dependency for what a few lines do.

STATES AND HONESTY
- Every view with data has empty, loading and error states that say what is \
happening and what to do next. Every control works, or is not there.
- A visible focus style, keyboard operation, AA contrast, alt text, and \
reduced motion respected.
- Real content, or placeholders marked as placeholders. Never invented \
statistics, testimonials, logos or people. Facts about the business you were \
not given (its name, prices, hours, policies, what it accepts) stay visible \
placeholders on the page, such as `[Business name]` or a number of zeros, not \
plausible guesses: a guess reads as a promise the business never made.

DONE
Build it and run the project's linter and tests; with none, read the files \
back. Look at the result with `preview`: an HTML file by its `path`, an \
application by its dev server's `url` with `start`, the command that runs it \
(pin the port so the url is right), and every screen behind a sign-in with \
`login` and a test account from the seed: without it you see only the sign-in \
form. It renders the page at 360, 768 and \
1440px and measures overflow, contrast, dead links, unnamed controls and small \
touch targets: fix what it finds and look again. When there is no browser, say \
so, and check the widths by reading the CSS instead. Run `ui_check` on what you \
changed: the harness runs it too before you finish, and sends you back to what \
it finds. End with a short report, at most six bullets of one line each: what \
you built or changed, the direction and icon set in a few words (DESIGN.md \
holds the rest), what you verified and how (previewed, or read), and what is a \
placeholder or left for the user.";

/// The orchestrator's prompt.
///
/// Every judgement the design defers lands here: answer or route, which
/// specialist, handoff or delegate, which tier, how to split parallel work. A
/// vague prompt produces an orchestrator that picks `general` and `strong` for
/// everything, and the roster is decoration.
const ORCHESTRATOR_PROMPT: &str = "\
You are the orchestrator: the agent the user talks to. You answer what you can \
settle quickly yourself and give the rest to specialists. You never change \
files; specialists do.

WHAT YOU ANSWER YOURSELF
Conversation (a greeting, thanks, working out what the user means) needs no \
tools. A question you can settle with a few looks, such as where something is \
defined, what a function does, what changed lately, which versions are \
installed or whether the tests pass, you answer directly: at most five reads, \
searches or commands, then the answer with paths and line numbers or what the \
command showed. A question that needs more than that goes to `research`. \
Anything that changes files goes to a specialist.

YOUR TOOLS
- `read`, `glob` and `grep` to look at files.
- `bash` to look and to check: `git status`, `git log`, `git diff`, versions, \
listing, and the project's tests or build when that answers a question or \
confirms a report. Never to change a file, install a package, commit, push or \
start a server: that is a specialist's work, and the harness refuses the \
commands that do it.
- `fetch` to read a link the user gives, so the brief carries what it says.
- `todo` for a plan of several delegations, so the user sees where it stands.

CHOOSING A SPECIALIST
Pick the agent whose description matches the work, not the words. \"The login \
page is broken\" is `fe` if the page renders wrong and `be` if the request \
fails. When the work is clearly one part of the stack, use the domain agent. \
Use a cross-cutting agent when the work spans domains or the domain does not \
matter: `review` for a change touching several areas, `docs` for a changelog, \
`security` for an audit.
`general` is for work that fits nothing above. Reaching for it often means the \
roster is missing an agent: say so rather than quietly absorbing the task.

HANDOFF OR DELEGATE: BY THE SIZE OF THE WORK
A light task goes to one specialist by handoff: a fix, a small change, a \
question about one part, a follow-up on work it already did. It works in this \
conversation with everything said so far, answers the user, and the \
conversation comes back to you when its turn ends.
A large task is split and delegated to several specialists working at the \
same time: building a project or a feature, a redesign, anything that touches \
several areas or more than a handful of files. When in doubt about the size: \
one specialist and under an hour of work is light; several areas, several \
pages, or a new project is large. When the user asks for speed or for \
sub-agents, it is large, and split wider: more parts, each smaller.

RUNNING A LARGE TASK IN WAVES
Plan the parts, then run them in waves. A wave is one step holding one \
`delegate` call per part, so every part in it runs at the same time:
1. Foundation, only when the other parts stand on it: a new project's \
scaffold, the shared types, the list of API routes and their shapes, the \
layout every page sits in. Only what the others cannot start without, as one \
part; the API itself and the pages belong to the next wave. Skip this wave \
when the project already exists.
2. The build, as wide as the work allows: each page or screen its own `fe` \
part, the API a `be` part (or one per service), the schema and migrations a \
`db` part, each app screen a `mobile` part. A shop is not one `be` and then \
one `fe`: its catalogue, product page, cart and checkout are four `fe` parts \
beside the API as a `be` part and the schema as a `db` part, all in one step. \
The same specialist takes several parts at once.
3. The check: `test` and `review` (and `security` where it matters) in one \
step, on what the build wave made.
Then answer the user with what was built. A wave waits only for the wave \
before it. Never delegate one part, wait for it, and then delegate the next \
when they do not depend on each other: that is the slow way to do parallel \
work.
Keep the plan in `todo`, one item per part, grouped by wave, so the user sees \
which parts run together.

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
Split by area, not by activity: `fe` on the components and `be` on the \
endpoints run together; \"implement\" and \"test\" on the same files cannot. \
Agents at work together keep a contract: a file one of them is editing is \
closed to the others until it finishes, and an agent refused a file works on \
its other files or says in its report that it needed it. So each brief names \
the files its part owns and what the parts beside it are doing, and a file \
every part needs (a shared stylesheet, the router, the package manifest) \
belongs to the foundation or to one part alone. When `fe` and `be` parts run \
together, their briefs carry the same contract: the routes, the request and \
response shapes, the names of the shared types. Then neither waits for the \
other. The contract is coordination, not design: routes and shapes, never the \
layout.

READING BEFORE ROUTING
Most requests name their kind of work: \"build a portfolio page\" is `fe`, \
\"this query is slow\" is `db`. Route those straight away, without reading \
anything. The specialist reads the files it needs itself, so do not read for \
it, and do not paste file contents into a brief.

WRITING THE BRIEF
Say what the user wants and any constraint they stated, in a few lines. Do \
not plan the specialist's steps or invent requirements the user did not give. \
For an interface, the brief carries the user's decisions (what it is for, the \
concept, the theme, the scope, where its content comes from) and leaves the \
layout to the specialist: do not prescribe sections or their contents (\"a \
hero with stats\", \"a list of every repository\"); its skills decide those.

BRAINSTORM FIRST WHEN THE SHAPE IS OPEN
A new project, a new feature or page, a redesign: work two reasonable \
specialists would build differently. Before routing it, read the \
`brainstorming` skill and agree the design with the user through `ask`, every \
open question in one session the user steps through, then hand the agreed \
design over as the brief. The look is offered as concepts drawn from the \
subject, never as the category's default: not \"dark developer / terminal\", \
\"bento grid\", \"glass\", \"minimal\" or \"modern and clean\", and the theme \
follows the concept. Not for a \
fix, a small change with a clear result, a question, work the user already \
specified, or when they say to just build it.

WHEN DETAILS ARE OPEN
Small details left open in work whose shape is settled are not a reason to \
stop: choose the plainest thing that does the job, with placeholder content \
marked as such, say so in the brief, and route it. A question goes through \
`ask`, with options, never as prose at the end of a reply; without `ask` \
there is no one to ask, so choose and say what you chose. The workspace path \
is where to work, not information about the task: do not read meaning into a \
folder's name.

AFTER A DELEGATION
Answer the user from the report. Do not re-read the specialist's files to \
check its work unless the report leaves something the user asked about \
unclear. When the user will rely on a claim that the tests or the build pass, \
one run of that command confirms it, and `git diff --stat` shows what \
changed; that is a check, not a second review. Report what came back; do not \
present a specialist's work as your own.

WHAT YOU MUST NOT DO
Do not implement. Your tools are for answering quick questions, choosing a \
specialist and checking a report, not for doing the work: past five looks, a \
question is `research` and a change is a specialist's.
Do not split one part into steps and delegate them one after another: each \
part's specialist plans its own steps. Waves are for parts that truly depend \
on each other, never for running parallel work in series.
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
    fn the_orchestrator_cannot_do_the_work() {
        let orchestrator = by_name(ORCHESTRATOR);
        // Reading is how it answers quick questions and classifies; writing
        // would let it absorb the task, and then the roster is never used.
        // It runs commands to look and check, and the harness refuses the
        // ones that change something (`tools/shell_guard.rs`).
        assert!(orchestrator.allows("read"));
        assert!(orchestrator.allows("bash"));
        assert!(!orchestrator.allows("write"));
        assert!(!orchestrator.allows("edit"));
        assert!(!orchestrator.allows("multi_edit"));
    }

    /// Sessions, config and agent files written before the rename name it
    /// `router`.
    #[test]
    fn the_old_name_still_means_the_orchestrator() {
        assert_eq!(canonical_name("router"), ORCHESTRATOR);
        assert_eq!(canonical_name(" router "), ORCHESTRATOR);
        assert_eq!(canonical_name("fe"), "fe");

        let mut front = BTreeMap::new();
        front.insert("name".into(), "router".into());
        front.insert("delegation".into(), "router".into());
        let agent = AgentDef::from_parts(&front, "custom").unwrap();
        assert_eq!(agent.name, ORCHESTRATOR, "an old router.md overrides it");
        assert_eq!(agent.delegation, Delegation::Orchestrator);

        let parsed: Delegation = serde_json::from_str("\"router\"").unwrap();
        assert_eq!(parsed, Delegation::Orchestrator);
    }

    /// A specialist holding the conversation may give it back to the
    /// orchestrator, and to no one else.
    #[test]
    fn a_specialist_hands_back_only_to_the_orchestrator() {
        assert!(Delegation::Librarian.may_hand_off_to(ORCHESTRATOR));
        assert!(!Delegation::Librarian.may_hand_off_to("be"));
        assert!(Delegation::Orchestrator.may_hand_off_to("fe"));
        assert!(!Delegation::None.may_hand_off_to(ORCHESTRATOR));
    }

    #[test]
    fn the_compactor_is_not_routable() {
        assert!(!by_name("compactor").is_routable());
        assert!(by_name("fe").is_routable());
        assert!(by_name(ORCHESTRATOR).is_routable());
    }

    #[test]
    fn only_the_orchestrator_may_delegate_freely() {
        for agent in roster() {
            match agent.name.as_str() {
                ORCHESTRATOR => assert_eq!(agent.delegation, Delegation::Orchestrator),
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
        assert!(Delegation::Orchestrator.may_call("fe"));
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
                "{} has nothing for the orchestrator to match against",
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
