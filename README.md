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
providers stay connected side by side, each with its own key, the way opencode
keeps them.

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
`/status` `/skills` `/mcp` `/compact` `/quit`

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

Three files under `~/.enx` (or `ENX_HOME`), split the way opencode splits them:

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
Node, Python, Go, Rust, Laravel), `code`, `writing`, `i18n` and
`brainstorming`, each carried by the agents
whose work needs it and read only when the work does. A project or user skill of the same
name replaces one. `/skills` and `/mcp` open popups to toggle or add entries;
`/compact` folds older turns into a summary; auto-compact fires when the
context window nears its cap.

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
