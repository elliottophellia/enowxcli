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
5. **`delegate(agent, task, tier?)`** — runs a branch session, returns a
   summary. Only the router holds handoff/delegate; if every agent could
   delegate, the call chain has no bound.
6. **Tier resolution** and the `[agent.tiers]` table.
7. **`auto_switch`** and the confirmation prompt.

Steps 1–3 are usable without 4–7: agents from files plus handoff is already an
improvement on three fixed roles.

## Open questions

- **Depth limit.** A sub-agent cannot delegate, which bounds the tree at one
  level. Enough, or is two levels needed?
- **Failure.** A sub-agent that fails or is interrupted — does the router see
  the error and retry with another specialist, or does it surface to the user?
- **Cost accounting.** Session usage is per session; a delegation spends
  tokens in a branch. The sidebar should probably show the total, which means
  branch usage has to roll up into the parent.
- **Concurrency.** Two independent sub-agents could run in parallel. Worth it,
  or does it make the transcript unreadable?
