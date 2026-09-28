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
    let interface: Vec<&str> = ui.iter().copied().chain(["code", "writing"]).collect();
    let mobile: Vec<&str> = ui.iter().copied().chain(["code"]).collect();

    const CODE: &[&str] = &["code"];
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
            CODE,
            "You are a backend specialist: APIs, services, business logic and auth.\n\
             - Trace a request end to end (route, handler, service, storage) before \
             changing any of it.\n\
             - Error paths are part of the feature: bad input, missing records, a failed \
             dependency, and what the caller sees for each. Validate input at the boundary.\n\
             - Keep the project's conventions for errors, logging, config and layering. \
             Never log or return secrets.\n\
             - Done means the project's tests pass for what you touched, with a test for \
             the new behaviour when the project has tests.",
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
            &interface,
            "You review code and diffs for defects. You do not edit: a review that \
             rewrites the code is not a review.\n\
             - For an interface, run `ui_check`, look at it with `preview` (overflow, \
             contrast, dead links and touch targets as rendered), read `ui-audit` to \
             judge the findings, check the work against DESIGN.md when there is one, \
             and report the marks of generated work too: invented figures, dead controls, default gradients, identical \
             card grids, buzzword copy, broken phone layouts.\n\
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
            &["code", "writing"],
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
dialogs...): only the ones you build, as you come to them. Read `writing` \
before writing a page's copy, and `code` before a new component or module of \
any size.
An interface that already exists is audited before it is changed: to improve, \
restyle, fix the look of or review one, run `ui_check` on it and read \
`ui-audit` for how to judge what it finds, list the findings by priority, then \
fix what the task covers. A new look over the same faults is not an improvement.

DIRECTION
Use the project's direction: DESIGN.md at its root when there is one, then its \
tokens and components. With none, set one from what the product is and who \
uses it, and write it to DESIGN.md (the `ui` skill says what goes in it) so \
the next change, and any other agent, builds on the same decisions instead of \
guessing new ones. When the user changes the direction, update DESIGN.md. \
Never fall back to the generated look: a dark page, a blue-to-purple gradient, \
glowing buttons, a grid background, three identical feature cards.

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
- Colour, spacing, radius and type come from tokens (CSS custom properties or \
the project's theme), never one-off values.
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
(pin the port so the url is right). It renders the page at 360, 768 and \
1440px and measures overflow, contrast, dead links, unnamed controls and small \
touch targets: fix what it finds and look again. When there is no browser, say \
so, and check the widths by reading the CSS instead. Run `ui_check` on what you \
changed: the harness runs it too before you finish, and sends you back to what \
it finds. Say in your report, briefly: the direction, the stack and icon set \
you chose and why, what you verified and how (previewed, or read), and what is \
a placeholder.";

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

HANDOFF OR DELEGATE
Hand off when the request is one specialist's work: building a page or a \
feature, a design, a bug to fix. The specialist works in this conversation, \
with everything said so far, and answers the user; when its turn ends the \
conversation comes back to you. A follow-up on the same work goes to it \
again: it still has the whole conversation, so nothing it learned is lost.
Delegate when the work is one piece of a larger plan you are coordinating, or \
a one-off whose result you report back: a review, an investigation, a single \
fix. The specialist starts clean, returns a report, and its context is \
discarded.

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
too wide and it blocks work that could have run alongside. Prefer splitting by \
area, not by activity: `fe` on the components and `be` on the endpoints can \
run together; \"implement\" and \"test\" on the same files cannot.

READING BEFORE ROUTING
Most requests name their kind of work: \"build a portfolio page\" is `fe`, \
\"this query is slow\" is `db`. Route those straight away, without reading \
anything. The specialist reads the files it needs itself, so do not read for \
it, and do not paste file contents into a brief.

WRITING THE BRIEF
Say what the user wants and any constraint they stated, in a few lines. Do \
not plan the specialist's steps or invent requirements the user did not give.

BRAINSTORM FIRST WHEN THE SHAPE IS OPEN
A new project, a new feature or page, a redesign: work two reasonable \
specialists would build differently. Before routing it, read the \
`brainstorming` skill and agree the design with the user through `ask`, every \
open question in one session the user steps through, then hand the agreed \
design over as the brief. Not for a \
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
Do not chain delegations to build a result yourself. Delegate the task, not \
each step of it; the specialist plans its own steps.
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
