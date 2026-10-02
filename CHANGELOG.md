# Changelog

All notable changes to enx. Dates are YYYY-MM-DD.

## Unreleased

Everything below is committed but not yet released; it ships in the next tag
after v0.1.0.

### Agents and delegation

- **New `maestro` agent**: an all-rounder in the LEAD group. It has the
  orchestrator's reach (delegate and hand off to anyone) but carries every
  tool, so it can change files, run commands and check interfaces itself
  rather than only handing work out. For a job that mixes doing and
  delegating. The orchestrator stays the default; pick it with
  `/agent maestro`.
- **A delegated agent always comes back with a report.** The four fields
  (DONE, CHANGED, VERIFIED, NEXT) are now read however the model dresses them
  (markdown, changed case); every ending without a report is nudged; after two
  nudges the model is asked once more with no tools, so all it can do is write
  the report; and if it still will not, the caller gets the four fields built
  from what the branch actually did.
- **A delegation at work is never run twice.** Resuming a branch that is still
  running is refused, and the orchestrator is told up front what it left
  running, so a "continue" from the user can no longer start the same work a
  second time.
- **A finished delegation is cleaned up.** Once a delegation reports
  successfully its report stays in the conversation and its transcript (and any
  delegation under it) is deleted instead of filling the disk. One that failed
  or stopped short is kept, to be resumed.
- **Reviewers are firm, tidy and honest.** `review` and `perf` open with one
  verdict on a fixed scale (Good, Good with fixes, Needs work, Bad) that
  follows from the findings, call bad work bad and good work good with the
  reason, and treat inconsistency as a finding rather than a nit to drop.
- **Give an agent its own model from the roster.** In `/agent`, `m` opens the
  model list for the selected agent and `d` puts it back on the default.

### Interface

- **Queue messages typed while a turn runs.** Enter during a turn puts the
  message in a queue above the composer, with a `[send now]` button; queued
  messages go one at a time as each turn ends. `Ctrl+Enter` sends now (idle it
  is still a newline; `Alt+Enter` is one at any time), `Up` in an empty
  composer edits the last queued message.
- **`/handoff` to a fresh session.** Folds the conversation into a summary in a
  new, light session held by the same agent; asks first whether to keep the old
  session's history or delete it.
- **The SESSION card shows what the session costs the machine**: the memory and
  CPU of enx and the processes it started (MCP servers, language servers, the
  preview browser), and the history on disk.
- **A sub-agent's transcript is read only.** Viewing a delegation shows no
  composer; nothing is typed or sent from it.
- **Ctrl shortcuts instead of function keys**, since not every terminal passes
  F-keys through: `Ctrl+T` next sidebar tab, `Ctrl+G`/`Ctrl+X` the log's filter
  and detail, and in `/model` `Ctrl+N` add, `Ctrl+E` edit, `Ctrl+R` refresh.

### Models

- **Model names are matched however the upstream spells them.** `claude-opus-4.7`,
  `claude-opus-4.7-1m`, `anthropic/claude-opus-4.7:thinking`,
  `us.anthropic.claude-opus-4-7` and the like now resolve to the right
  catalogue entry, so context window, prices, vision and thinking efforts are
  correct. The maker's listing wins over a reseller's.
- **Edit a model's properties** (context window, thinking effort, vision,
  prices) from the model list.

### Built-in MCP servers

- **`coolify`, `dokploy` and `vps`, served by enx itself** (no Node or Python).
  A server is offered to agents only once installed: `enx mcp install coolify`,
  `enx mcp install dokploy`, or `enx vps add` for an SSH host. Tokens and
  passwords go to `auth.json`; secret fields and environment values are redacted
  from tool output; a VPS host key is pinned on first connection.

### Skills

- **New `ui-layout-grid` skill**: CSS Grid and Flexbox mechanics for tidy
  layouts.
- **New `canvas` agent and skills**: standalone single-file HTML pages, tools,
  visualisers and games.
- **UI skills enforce layout mechanics**: every layout is flexbox or grid,
  cards in a group share one size and row height, controls in a row share a
  height, and dropdowns are custom because the native select renders
  differently on every OS (with a keyboard- and ARIA-complete custom select in
  `ui-part-choices`).

### Tools

- **The `todo` tool keeps nested steps** instead of silently dropping them: a
  step may be an object with sub-steps, flattened with indentation.
- **The `ask` tool reads a call however the model dresses its fields** (a
  question named `prompt`/`text`/`q`/`title`, options named `choices`/`answers`,
  a bare string, a single question not wrapped in a list).

### Fixes

- **Backspace works everywhere.** Terminals that send Backspace as `^H` (many
  Linux terminals and SSH sessions) now erase instead of doing nothing.
- **Windows builds.** `nix` is used only on Unix, and the bash tool and preview
  browser fall back to the `sh` from Git for Windows, or PowerShell.
- **The session resource card counts only enx's real descendants**, not
  unrelated processes caught by macOS recycling a pid.
- **`/resume` lists conversations only**, never a delegation's transcript.

### Project

- **Install from [enowx.ai](https://enowx.ai)** (`curl -fsSL https://enowx.ai/install.sh | sh`,
  or `irm https://enowx.ai/install.ps1 | iex` on Windows). The installer adds
  enx to your `PATH` and the site counts installs.
- **Licensed under Apache-2.0** (was MIT).

## v0.1.0 (2026-10-02)

First public build (early release). Prebuilt binaries for macOS, Linux
(static musl) and Windows on x86_64 and aarch64, built and published by a
GitHub release workflow. The agent loop, the terminal interface, roles, the
specialist roster and the authorized-assessment security team, built-in
skills, and smart context-window detection.
