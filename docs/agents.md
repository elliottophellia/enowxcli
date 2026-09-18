# Agents

Design for replacing the three hardcoded roles with a router and a roster of
specialists. Nothing here is implemented yet; this records the decisions and
the reasoning so the work can be picked up without re-deriving them.

> Not to be confused with `/AGENTS.md` at the repo root, which is an
> instruction file for agents working *on this repository*. This document
> describes a feature *of* enx.

## The shape

One **router** decides who works; **specialists** do the work. The router has
no editing tools of its own — if it could do the job it would, and the roster
would go unused. It reads enough to classify the request, then hands over.

A specialist is one definition used two ways:

```
HANDOFF                              SUB-AGENT
router ──hands over──▶ fe            router ──calls──▶ fe   (branch session)
                       │                              │ briefing only
      same session,    │ carries on                   │ works
      full history     │ to the end                   │
                       ▼                              ▼
              user now talks to fe      router ◀──summary── done

                                       router keeps leading
```

| | Handoff | Sub-agent |
|---|---|---|
| Context in | Full history, same session | Briefing only, clean start |
| On finish | Carries on as normal | Returns a summary |
| Session | One, the agent changes | Branches, then rejoins |

Handoff suits a request that is one specialist's job end to end. Sub-agent
suits a piece of work inside a larger task — and it is the cheaper of the two,
see [Cost](#cost).

## Why this is cheaper, not more expensive

The worry with a large roster is token cost. Measured, it is not the problem:

| Roster | Cost per turn |
|---|---|
| 13 agents, one-line descriptions | ~170 tokens |
| 40 agents, verbose descriptions | ~2,100 tokens |

Against the fixed overhead of a turn — router prompt, specialist prompt, tool
schemas — the whole roster is about **two file reads**. Descriptions are cheap.

What actually fills a window is accumulated tool output, and that is where the
sub-agent mode wins. For a task touching frontend and backend:

| | Tokens held at the end |
|---|---|
| One agent doing everything | ~47,500 |
| Router with two sub-agents | ~3,200 in the router |

Each specialist peaks in its own branch and is then discarded. The router
carries roughly **15× less**. More specialists, used as sub-agents, means less
context held — not more.

Four rules keep it that way:

1. **Descriptions are written to be chosen from, not read.** One line, domain
   keywords. `- fe: frontend, React, CSS, components, accessibility` is 11
   tokens and enough to route on.
2. **A specialist's prompt loads only when it is used.** The router needs to
   know a specialist exists and what for, not how it thinks.
3. **Tool schemas follow the active agent.** The router needs read/glob/grep
   plus handoff/delegate; a specialist's ten-tool schema never reaches it.
4. **Prefer sub-agent for substantial work.** Handoff is comfortable but its
   context accumulates; delegation discards it.

## The roster

Two groups. The split is by what the work *is about* (domain) versus what kind
of work it *is* (cross-cutting), because those are the two questions a router
can actually answer from a request.

### Domain — which part of the stack

| Agent | For |
|---|---|
| `fe` | Frontend: React/Vue/Svelte, CSS, components, accessibility, bundlers |
| `be` | Backend: APIs, services, business logic, auth |
| `db` | Schema, migrations, queries, indexing, data modelling |
| `devops` | CI/CD, containers, deploy, infrastructure, observability |
| `mobile` | iOS/Android, React Native, Flutter, native APIs |
| `systems` | Low-level: memory, concurrency, FFI, binary formats |

### Cross-cutting — applies to any domain

| Agent | For |
|---|---|
| `librarian` | Gathers material: reads widely, returns excerpts with paths and line numbers. Does not conclude. |
| `research` | Answers questions: maps the ground and reports findings with conclusions. |
| `review` | Reads diffs and code, reports defects, never edits |
| `test` | Writes and fixes tests, reproduces reported failures |
| `docs` | READMEs, changelogs, API docs, comments |
| `security` | Auth, secrets, injection, dependency risk |
| `perf` | Profiling, hot paths, benchmarks |
| `general` | Fallback for anything outside the above |

Plus one that is never routed to: `compactor`, invoked by the loop rather than
chosen by the router. See [Compaction](#compaction).

`librarian` and `research` are both read-only and are easy to confuse. The
difference is the output: librarian returns **raw material, filtered** —
quotes, paths, line numbers — while research returns **an answer**. Librarian
is the natural sub-agent: read twenty files, hand back the ten relevant
excerpts, discard the rest.

### Why not one axis

An earlier draft used domain only (8 agents). Checked against fifteen real
requests, eight had no home: review, tests, docs, dependency upgrades,
security audits, changelogs. They are genuinely cross-domain — "review this
PR" is neither frontend nor backend.

A three-axis catalogue (domain × activity × concern, 17 agents) covered
everything but made routing ambiguous: "fix the bug in the API" is equally
`debug` and `be`, and the router has no basis to choose.

Two groups with a stated tie-break avoids both:

> **When a request is clearly one domain, use the domain agent. Use a
> cross-cutting agent when the work spans domains or the domain does not
> matter.**

So "review the API changes" → `be` with a review instruction. "Review this PR"
(touching several areas) → `review`.

## Agent definitions

Agents are files, not Rust. This is the load-bearing decision: a roster that
needs a recompile to extend will not be extended. Skills and MCP servers are
already discovered from disk; agents follow the same path.

```
.agents/agents/fe.md          # project
~/.enx/agents/fe.md           # user
```

```markdown
---
name: fe
description: frontend, React, CSS, components, accessibility
tools: read, write, edit, glob, grep, bash, todo
tier: balanced          # optional; default is the active model
---

You are a frontend specialist. …
```

| Field | Meaning |
|---|---|
| `name` | How the router addresses it |
| `description` | One line. This is what the router sees; write it to be chosen from |
| `tools` | Tool surface. Filtered twice, as roles are today: never advertised, and refused if called anyway |
| `tier` | Optional model tier, see below |

Project definitions override user ones by name, matching how skills already
resolve.

## Model tiers

A specialist should not always run on the same model as the router. The router
mostly classifies; a specialist may be doing the hard part.

Agents name a **tier**, not a model, so a definition survives changing
provider:

```toml
[agent.tiers]
cheap    = "cbc/deepseek-v4.1-flash"
balanced = "cbc/claude-sonnet-4.5"
strong   = "cbc/claude-opus-5"
```

Resolution order, first match wins:

1. User override in config (`[agent.models] fe = "…"`)
2. Tier chosen by the router for this particular delegation
3. Tier declared in the agent file
4. The active model

The router picking a tier per call is the point: the same specialist can run
cheap for a small fix and strong for a redesign. Guidance for that judgement
belongs in the router's prompt and is still to be written — without it the
router will reach for `strong` every time.

Unmapped tiers fall back to the active model rather than failing; a missing
tier table must not make delegation stop working.

## Control

```toml
[agent]
auto_switch = true    # switch without asking
```

- `true` — the router hands over or delegates on its own.
- `false` — a prompt appears first: *"hand over to fe?"*

Either way the user can force a switch (`/agent fe`), and forcing overrides
whatever the router had decided.

## What has to change in the code

Ordered so each step is usable on its own.

1. **`Role` (enum) → `Agent` (data).** Everything else depends on it. Load
   from `.agents/agents/*.md` through the existing discovery path. The three
   current roles become shipped definitions, so nothing regresses.
2. **`session.role` → `session.agent`, plus a switch history.** The transcript
   has to show a change of agent; otherwise the answer silently changes voice
   and the user cannot tell why.
3. **`handoff(agent, reason)`** — same session, agent changes, history stays.
4. **Branch sessions** — a sub-agent needs its own saved session so its
   transcript is inspectable, without entering the router's context.
5. **`delegate(agent, task, tier?, writes?, reads?)`** — runs a branch
   session, returns a summary. Only the router holds handoff/delegate, except
   that a specialist may call `librarian`. Branch usage rolls up into the
   parent, or the cost readout is wrong by an order of magnitude.
6. **Tier resolution** and the `[agent.tiers]` table.
7. **`auto_switch`** and the confirmation prompt.
8. **Parallel delegation** with write contracts, serialising the overlaps.
   Worth deferring until sequential delegation is proven — it multiplies the
   ways a run can go wrong.
9. **Compaction rewrite** — tool calls in the summary input, and the
   `compactor` agent. Independent of 1–7 and worth doing early: it is the
   largest single saving, and the current summariser is losing information
   today.

Steps 1–3 are usable without 4–7: agents from files plus handoff is already an
improvement on three fixed roles.

## Failure and fallback

A model call fails in four ways, and today all of them take the same path: ten
retries against the same model, then give up.

| Kind | Example | Retry helps? | Another model helps? |
|---|---|---|---|
| Transport | connection reset, timeout | Yes | No |
| Capacity | 429, 503, overloaded | Yes | Yes |
| Capability | context overflow, no tool support | **No** | **Yes** |
| Permanent | 401, unknown model | No | No |

Retrying capacity failures is close to free — the server rejects before
processing, so a refused request is generally not billed; what is spent is
time. The exception is a stream that dies *after* the response started: that
work was done and charged, so it is retried twice rather than ten times.

Capability failures are the ones retrying cannot fix. Those need a different
model, which is what the tier ladder is for.

### The ladder

1. Retry the chosen model, per the table above.
2. Exhausted — try another model in the same tier, if the config lists one.
3. Still failing — drop one tier (`strong` → `balanced` → `cheap`) and say so
   in the transcript: *"strong unavailable, continuing with balanced"*.
4. Bottom of the ladder — fail, naming every model tried.

The notice at step 3 is not optional. A cheaper model may not be up to the
task, and a silent downgrade produces worse work with no indication why.

## Compaction

Compaction is the largest lever on token cost — larger than the agent
architecture. Measured over a thirty-turn conversation:

| | Cumulative input |
|---|---|
| No compaction | 1,395,000 |
| Compact aggressively | 363,000 (26%) |
| + sub-agent architecture | 279,000 (20%) |

The reason is that every turn resends the whole conversation, so cost grows
with the square of the length. Nothing else moves that number as much.

### What it does today

1. Keep the last `keep_last` turns verbatim (default 4), fold the rest.
2. Flatten the folded turns to `USER: …` / `ASSISTANT: …` text.
3. Ask the model for a Goal / Decisions / Files / Open note.
4. Replace the folded turns with one `[COMPACTED SUMMARY]` message.

The session file on disk is untouched — only what is sent to the model
shrinks, so a compacted session still resumes with its full history.

### The problem

The summariser only sees user and assistant prose. Tool calls and their
results are dropped by a catch-all arm before summarisation, and an assistant
turn whose content is empty because it carries `tool_calls` is dropped with
them.

For a working session — twelve reads and six commands — that leaves the
summariser looking at about **7% of what happened**. It is then asked for
"Files: which files were created, edited, or read", information that was
discarded before it arrived. That is why compaction feels like it loses the
thread: not that it folds too much, but that it folds away the part that
carried the work.

### The fix

Include tool calls, summarised rather than verbatim: the tool, its argument,
and a one-line outcome.

```
USER: fix auth
ASSISTANT: let me look
  → read src/auth.rs (240 lines)
  → edit src/auth.rs (+12/-4)
  → bash cargo test → exit 0, 41 passed
ASSISTANT: done, the token check was inverted
```

This costs about **360 tokens** per session for the tool lines, and closes the
89% gap. Full file contents and command output stay out — the point is what
was touched and what came back, not the bytes.

### Compaction as an agent

The summariser becomes a `compactor` agent rather than a hardcoded prompt. It
is a natural fit for the roster: its own prompt, its own tier — `cheap`, since
the job is condensing rather than reasoning — and improvable by editing a file
instead of recompiling.

It is not routed to like the others; the loop invokes it directly, on the
threshold or on `/compact`.

### Trigger

Percentage of the window, as today, plus the existing manual `/compact`.

Worth knowing: a percentage trigger scales with the window, so on a 1M-context
model `auto_compact_at = 0.85` fires at 850,000 tokens — in practice never,
and the conversation pays the full quadratic cost. A large window is for
holding one large file, not a reason to drag 800k of history through every
turn. Users on large-context models should set this far lower.

## Delegation rules

### Depth

One level: only the router delegates. A specialist that could delegate turns a
task into a tree — three levels at a fanout of three is forty agents and
~600,000 tokens, which no task justifies.

One exception: **a specialist may call `librarian`.** It is read-only, so it
cannot corrupt anything, and it exists precisely to keep a specialist's
context clean — read twenty files, hand back ten excerpts. Forbidding it would
push that reading into the specialist's own window, which is the cost the
architecture is trying to avoid.

So: router → specialist → librarian, and no other second hop.

### When a sub-agent fails

Five ways it ends badly, and they are not equivalent:

| Failure | State afterwards |
|---|---|
| Wrong specialist chosen | Nothing changed — safe |
| Model exhausted every tier | Nothing changed — safe |
| Specialist declines the task | Nothing changed — safe |
| **Ran out of steps** | **Partial: files already changed** |
| **Interrupted** | **Partial: files may be half-written** |

The first three are simply "pick differently". The last two are the dangerous
ones: work is half done, and a router that treats them as "try again" will
have a second specialist build on a state it knows nothing about.

So a partial failure is retried **once**, with the same specialist, and the
briefing carries what already changed:

```
Previous attempt ran out of steps. Already changed:
  src/auth.rs   edited (+12/-4)
  tests/auth.rs created
Continue from there; do not redo work that is done.
```

Once. If it fails again the router stops and reports to the user, naming the
files left in a partial state. Retrying further only stacks up more half-done
edits, and each attempt makes the state harder to reason about.

### Cost

A branch session keeps its own usage, so without rolling it up the sidebar
reports the router's spend alone: 3,000 tokens shown against 87,000 actually
spent, off by **29×**. That is not an incomplete number, it is a wrong one.

Branch usage rolls up into the parent. The total is what the sidebar shows;
a per-agent breakdown is available for anyone who wants to see where it went.

### Parallelism

Sub-agents may run in parallel, but each one declares a **contract** up front:

```
delegate(agent="fe", task="…", writes=["src/ui/**"], reads=["src/api/types.rs"])
```

- **`writes`** — paths or globs it intends to change.
- **`reads`** — paths it expects to read. Overlapping reads are fine.

Overlapping *writes* are the only conflict. Those delegations are serialised —
the second waits for the first — while everything else still runs together:

```
fe    → src/ui/**        ┐
be    → src/api/**       ├ in parallel
docs  → README.md        ┘

test  → src/ui/**        ← overlaps fe, runs after it
```

Nothing is refused. A router that had its delegation rejected would have to
re-plan, and it has no better information than the scheduler does.

Declaring intent beats locking: a conflict is known before any work starts,
rather than discovered halfway through when one agent has already written.

## Open questions

- **Contract enforcement.** A specialist declares `writes` but the tool layer
  does not yet know about it. Is the declaration advisory, or should a write
  outside it be refused the way an out-of-workspace path already is?
- **Router prompt.** The tier guidance and the domain/cross-cutting tie-break
  both live in the router's prompt, and neither is written. Without them the
  router will reach for `strong` every time and pick a domain agent for
  everything.
- **Handoff and cost.** Handoff keeps one session, so its cost is already
  counted. But a session that has changed agent three times has three prompts'
  worth of history in it — is that worth compacting on switch?
