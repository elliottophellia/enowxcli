# enowx-cli

A Rust coding agent with a terminal interface.

```
enx                          # open the terminal interface (default)
enx auth login deepseek      # store a provider's API key (typed, not echoed)
enx auth list                # which providers are connected, and how
enx config get model.active  # the model in use, as provider/model
enx config set model.default deepseek/deepseek-flash   # pin the model to start on
enx config path
```

enowx-cli opens on a home screen: the enowX wordmark with the composer under
it. Until a provider and a model are set, the line under the composer says
which one is missing. Open `/provider`, choose a provider, then enter only its
API key. Built-in providers: **enxapi**, OpenAI, OpenRouter, Groq, and DeepSeek;
"Add a custom provider" takes any OpenAI-compatible endpoint. Any number of
providers stay connected side by side, each with its own key.

## What is inside

| Piece | Notes |
|---|---|
| Agent loop, tools, sessions | Streaming model calls, paired tool results, JSONL sessions |
| Terminal interface | Framed layout, thought/tool cards, paged right sidebar, theme picker |
| Roles | Three shipped: Orchestrator, Writer, Researcher |

The binary opens the terminal interface, with five selectable palettes.

## Roles

| Role | Tools | Purpose |
|---|---|---|
| Orchestrator | read, write, edit, glob, grep, bash, fetch, todo | Owns a task end to end, then verifies it |
| Writer | read, write, edit, glob, grep, todo | Documentation and prose. No shell |
| Researcher | read, glob, grep, fetch, todo | Read-only investigation |

Role filtering runs twice: unavailable tools are never advertised to the model,
and a call that arrives anyway is refused before dispatch.

## Terminal commands

`/help` `/new` `/sessions` `/resume <id>` `/role` `/model` `/provider`
`/reasoning` `/tools` `/theme` `/sidebar` `/tab 1..5` `/clear` `/stop`
`/retry` `/status` `/skills` `/mcp` `/compact` `/quit`

When the provider is down (502, 503, 429, a dropped connection), each call is
retried for about three and a half minutes. A turn that still fails continues
from where it stopped by itself, up to three times a minute apart; `/retry`
continues at once and `Esc` cancels the wait.

Keys: `Enter` sends, `Ctrl+Enter` inserts a newline, `/` opens the palette,
`Ctrl+R` toggles reasoning, `Ctrl+O` toggles tool output, `PgUp`/`PgDn` scroll
the chat, and `Ctrl+C` clears the composer, opens the quit prompt when the
composer is empty, or interrupts a running turn.

`F1`–`F5` (or `Alt+1`–`Alt+5`) selects a telemetry tab without consuming typed
digits. `Ctrl+B` shows/hides the sidebar; on narrow terminals it opens over the
transcript while leaving the composer accessible.

Themes are changed only through `/theme`: arrow keys preview, `Enter` saves,
`Esc` cancels. Palettes: Obsidian Ice, Neo Acid, Chrome Void, OLED Stealth, and
Classic Amber. `NO_COLOR` is respected; unset it to see palette colours.

Mouse: drag over the transcript copies text to the clipboard on release, click
on a rendered file path opens it with the OS default application, and the
scroll wheel scrolls the transcript, popup selectors, or the composer field
depending on where the pointer is.

`/provider` lists every provider with whether it is connected and whether the
model in use is on it. Enter on a built-in provider asks for its key; a custom
one opens its name, base URL, key and model-list URL. `d` removes a
provider's stored key, and a second `d` removes a custom provider.

`/model` lists the models of every connected provider in one place, the
favourites and recent picks first. Type to search, `Enter` to use a model,
`Ctrl+F` to mark a favourite, `F2` to add a model by hand, `F5` to ask the
providers for their lists again. A model is always named with its provider,
`deepseek/deepseek-flash`, and a pick is remembered in `~/.enx/model.json`
rather than written to `config.toml`. `/model <provider/model>` does the same
from the composer.

## Configuration

Three files under `~/.enx` (or `ENX_HOME`), each with one job:

| File | Holds |
|---|---|
| `config.toml` | Custom providers, a pinned start model, and every other setting. No keys |
| `auth.json` | One API key per provider, readable by the user alone. `enx auth login/logout` edits it |
| `model.json` | The recent and favourite models picked in `/model` |

At start the model in use is the first that can run of: `ENX_MODEL`, a pinned
`model.default`, then the recent picks. A provider's key also comes from its
own environment variable (`DEEPSEEK_API_KEY`, `OPENAI_API_KEY`,
`OPENROUTER_API_KEY`, `GROQ_API_KEY`). `ENX_BASE_URL`, `ENX_API_KEY` and
`ENX_MODEL` describe an endpoint for one process, for a container, and are
never written. A configuration from before this layout is moved on first load,
with the old file kept as `config.toml.before-providers.bak`.

| Key | Meaning |
|---|---|
| `model.default` | Model to start on, as `provider/model`; empty uses the latest pick |
| `model.active` | Read only: the model in use |
| `provider.<id>.base_url` | A custom provider's OpenAI-compatible endpoint |
| `provider.<id>.models_url` | Where its model list is read |
| `provider.<id>.name` | Its name in the interface |
| `provider.<id>.models.<model>.context_window` | A window for one model, over what the provider or catalogue says |
| `agent.models.<agent>` | A model of its own for one agent, on any connected provider |
| `agent.max_steps` | Hard cap on model calls per turn |
| `agent.workspace` | Directory the file and shell tools are rooted in |
| `agent.shell_timeout_secs` | Kill a shell command after this long |
| `agent.lsp` | Check written files with the project's language servers (default on) |
| `agent.preview` | Let agents look at pages in headless Chrome (default on; `/preview` toggles it) |
| `agent.background_delegation` | The orchestrator's delegations run on after its turn and their reports wake it (default on) |
| `agent.auto_compact_at` | Fraction of the context window that triggers auto-compact |
| `agent.compact_keep_last` | Turns kept verbatim during compact |
| `ui.theme` | `obsidian_ice`, `neo_acid`, `chrome_void`, `oled_stealth`, or `classic` |
| `ui.currency` | Display currency for cost readouts (USD, IDR, JPY, …) |
| `ui.currency_rate` | Multiplier applied to USD prices for the display currency |

Sessions are stored as JSONL under `~/.enx/sessions`, one file per session,
including tool calls and results for replay. Interrupted calls without a
recorded result are marked unavailable rather than executed again
automatically. A session records the workspace it was created in and refuses to
resume against a different one.

Skills, MCP servers, and per-project agent instructions are discovered from
`.agents/`, `.enx/`, `.claude/`, `.cursor/`, `.gemini/`, and the standard
`~/.config` locations. Skills ship inside enx for interface work (`ui`,
`ui-layout`, `ui-audit`, one `ui-page-*` per kind of page and one `ui-part-*`
per part), for server work (`backend`, one `backend-*` per part such as the
API, auth, data, jobs and tests, and one `backend-stack-*` per stack: Next.js,
Node, Python, Go, Rust, Laravel, Java, .NET, Rails, plus caching, real-time,
files, search and GraphQL), for motion (`motion*`), the engineering behind an
interface (`frontend*`: state, data, forms, accessibility, performance,
testing, SEO, security, errors), databases (`database*`), infrastructure
(`devops*`), mobile apps (`mobile*`), low-level work (`systems*`), tests
(`testing*`), documentation (`docs*`), security audits (`security*`),
performance (`performance*`), reviewing (`review*`), research (`research*`),
gathering (`librarian`) and running large tasks (`orchestration`), plus
`code`, `writing`, `i18n` and `brainstorm` (agreeing a design, then the
plan documents the user chooses: PRD, DESIGN, ARCHITECTURE, ERD, API, PLAN,
written by the orchestrator with `plan_write`), each carried by the agents
whose work needs it and read only when the work does. A project or user skill of the same
name replaces one. A skill installed in the project or `~/` goes to every
agent until the orchestrator binds it, with `skill_bind`, to the agents whose
work it serves; bindings are kept in `~/.enx/skill-bindings.json`, and the
Skills tab shows who has each one. `/skills` and `/mcp` open popups to toggle
or add entries; `/compact` folds older turns into a summary; auto-compact fires
when the context window nears its cap.

After `write`, `edit` and `multi_edit`, the file goes to its language server
(rust-analyzer with clippy, typescript-language-server, pyright and ruff,
gopls) and the errors and warnings come back with the result, so an agent
fixes a type error before it builds anything on top of it. An edit waits only
for the server's first answer; the `diagnostics` tool waits for the full
check. A server that is not installed is named once, with its install
command. `agent.lsp = false` turns this off.

Every change an agent makes goes through three checks before it lands:

- **Rules.** Markdown rules (a pattern, the files it applies to, and what to
  do instead) are checked on the code the change adds. `block` refuses the
  change and returns the rule; `remind` lets it through with the rule
  attached. enx ships rules for secrets in code, `any`, empty catches, index
  keys, `Box::leak`, deprecated Go and Python APIs, `transition: all` and
  removed focus outlines; add or override them in `~/.enx/rules/` or the
  project's `.enx/rules/` (`severity: off` turns one off). A line with
  `enx-allow: <rule>` passes a blocking rule.
- **Syntax.** Rust, TypeScript, TSX, JavaScript, Python, Go, JSON and CSS
  files are parsed before and after the change. When a change breaks a file
  that parsed, a quick model call mends the changed region; when that fails,
  an edit is refused with the error's line.
- **Language server**, as above.

`read` shows each line with an anchor (`12#a3f:text`), and `edit_lines`
changes whole lines by those anchors, so an agent never has to repeat the
old text exactly, and an edit against a stale read is caught. `lsp` asks
the language server for a definition, references, a symbol's type, a file's
symbols, or a rename across every file.

Calls in one step that only look (`read`, `glob`, `grep`, `fetch`,
`diagnostics`, `ui_check`, and at most one `bash` beside them) run at the
same time; writes and everything else keep their order. A picture that is
already in the conversation is not sent to the model again.

The orchestrator does not wait on its specialists: a wave it delegates runs
in the background, its turn ends, and the status bar says how many agents
are working. When the whole wave has finished, their reports start its next
turn on their own. Tests and review run only when the user chose them at
the start.

`preview` shares one headless Chrome across every look, each in a throwaway
browser context, two at a time; the browser closes after 90 seconds without
a look and never outlives enx. Looking again at a page with no file changed
returns the last result instead of opening a browser.

File tools reject paths and symlinks outside the workspace. `bash` runs with
the user's OS permissions and is **not a sandbox**; use only with trusted tasks
and providers.

## Build

```sh
cargo build --release
cargo test --workspace
```

Install the binary (cargo emits it as `enx`, per `[[bin]]` in
`crates/enowx-cli/Cargo.toml`):

```sh
which -a enx                                     # expect no output before installing
install -m 755 target/release/enx ~/.local/bin/enx
```

## Live reload while developing

```sh
enx dev                     # rebuild and relaunch the interface on every source change
enx dev --session <id>      # pin one conversation across reloads
enx tui --session <id>      # resume a session directly
```

Run `enx dev` from this checkout. It watches `crates/` and `Cargo.toml`,
keeps the interface in the foreground, and on each save rebuilds and relaunches
it while resuming the newest session in this workspace.

## Licence

MIT. See `LICENSE`.
