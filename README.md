# enowxcli

A Rust coding agent with a terminal interface.

Website: [enowx.ai](https://enowx.ai)

```
enx                          # open the terminal interface (default)
enx auth login deepseek      # store a provider's API key (typed, not echoed)
enx auth list                # which providers are connected, and how
enx config get model.active  # the model in use, as provider/model
enx config set model.default deepseek/deepseek-flash   # pin the model to start on
enx config path
```

## Install

**Early release.** v0.1.0 is the first public build; expect bugs and please
[report them](https://github.com/enowdev/enowxcli/issues).

Prebuilt binaries are published on the [releases page](https://github.com/enowdev/enowxcli/releases)
for macOS, Linux and Windows, on Intel/AMD (x86_64) and ARM (aarch64). The
installers are served from [enowx.ai](https://enowx.ai) and download the
binary from those releases. The binary is `enx`.

### macOS (Apple Silicon and Intel)

```sh
curl -fsSL https://enowx.ai/install.sh | sh
```

The script picks the right build, checks its SHA-256, installs to
`~/.local/bin/enx`, signs it ad hoc so Gatekeeper lets it run, and adds
`~/.local/bin` to `PATH` in your shell's rc file (`.zshrc`, `.bash_profile`,
fish's `config.fish`, or `.profile`). Open a new terminal and run `enx`.

### Linux (x86_64 and ARM64)

The same script. The Linux builds are static (musl), so they run on any
distribution, including Alpine, without extra libraries.

```sh
curl -fsSL https://enowx.ai/install.sh | sh
```

### Windows (x64 and ARM64)

In PowerShell:

```powershell
irm https://enowx.ai/install.ps1 | iex
```

It installs to `%LOCALAPPDATA%\Programs\enx\enx.exe` and adds that folder to
your user `PATH`; open a new terminal afterwards. The `bash` tool runs commands
with the `sh` from [Git for Windows](https://git-scm.com/download/win) when it
is installed, and with PowerShell otherwise. Windows Terminal renders the
interface best.

### Options and manual install

Both scripts read `ENX_VERSION` (a release tag such as `v0.1.0`; default the
latest) and `ENX_INSTALL_DIR`. `ENX_NO_MODIFY_PATH=1` stops `install.sh` from
touching your shell config:

```sh
curl -fsSL https://enowx.ai/install.sh | ENX_VERSION=v0.1.0 ENX_INSTALL_DIR=/usr/local/bin sh
```

To install by hand, download the archive for your platform from the releases
page, check it against its `.sha256` file, and put `enx` (`enx.exe`) on your
`PATH`:

| Platform | Archive |
|---|---|
| macOS, Apple Silicon | `enx-aarch64-apple-darwin.tar.gz` |
| macOS, Intel | `enx-x86_64-apple-darwin.tar.gz` |
| Linux, x86_64 | `enx-x86_64-unknown-linux-musl.tar.gz` |
| Linux, ARM64 | `enx-aarch64-unknown-linux-musl.tar.gz` |
| Windows, x64 | `enx-x86_64-pc-windows-msvc.zip` |
| Windows, ARM64 | `enx-aarch64-pc-windows-msvc.zip` |

### From source

With a Rust toolchain:

```sh
cargo install --git https://github.com/enowdev/enowxcli enowx-cli
```

Or build it yourself (see [Build](#build)). Check the install with
`enx --version`.

enowxcli opens on a home screen: the wordmark, `enow` in pixel letters with the
mark (an X of lit cells, its centre in orange) as the X, and the composer under
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
| Specialists | A roster the orchestrator delegates to, including an authorized-assessment security team |

The binary opens the terminal interface, with five selectable palettes.

## Roles

| Role | Tools | Purpose |
|---|---|---|
| Orchestrator | read, write, edit, glob, grep, bash, fetch, todo | Owns a task end to end, then verifies it |
| Writer | read, write, edit, glob, grep, todo | Documentation and prose. No shell |
| Researcher | read, glob, grep, fetch, todo | Read-only investigation |

Role filtering runs twice: unavailable tools are never advertised to the model,
and a call that arrives anyway is refused before dispatch.

## Specialists

The orchestrator hands work to a roster of specialists, grouped in the sidebar:

- **BUILD**: `fe` (frontend/interface), `motion`, `canvas` (standalone
  single-file HTML pages and tools), `be` (backend), `db`,
  `devops`, `mobile`, `systems`.
- **SECURITY**: `security`, the lead of an authorized security assessment, and
  its team.
- **SUPPORT**: `research`, `review`, `test`, `docs`, `perf`, `librarian`,
  `general`.

### The security assessment team

`security` is the single security role a user selects. It leads an authorized
penetration test: it confirms the scope and written authorization first, then
delegates recon and each testing area to a specialist, consolidates the
findings, and finds the chains an individual surface cannot see. Its thirteen
specialists are reachable only through it (a `Lead` delegation, so an ordinary
request never lands one directly):

| Agent | Tests |
|---|---|
| `sec-recon` | Maps the target: hosts, services, versions, stack, input surface |
| `sec-osint` | Passive intelligence, no active touch on the target |
| `sec-webapp` | Web application against the OWASP categories |
| `sec-api` | REST/GraphQL/gRPC: authorization, injection, mass assignment, tokens |
| `sec-cloud` | AWS/Azure/GCP/Kubernetes posture |
| `sec-internal` | Authorized internal hosts, post-foothold |
| `sec-mobile` | Android/iOS applications and their backend |
| `sec-intercept` | Proxy-driven request tampering |
| `sec-reverse` | Binary and firmware analysis |
| `sec-threat-model` | Attack surface and trust boundaries |
| `sec-ir` | Incident triage and response |
| `sec-vibecoder` | Scores how likely a site was AI/boilerplate generated, and reports the gaps it left |
| `sec-report` | The write-up |

Active testing of a domain requires verifiable authorization, not a claim in
the chat. `authorize_target` checks that a connected Cloudflare account controls
the domain's DNS zone, which proves control of the domain, and records the
domain, its subdomains and the addresses it resolves to as the scope the team
may test. A domain the account does not control is refused. Connect the account with `enx auth login cloudflare`: it asks which scope
(minimal `Zone:Read`, medium, or full, all read-only), opens the Cloudflare
token page with that template pre-filled, verifies the token you paste reads
zones, and saves it. For the longest-lived token leave its validity
as no expiry (the default, and longer than any end date). The token is read from `auth.json` or `CLOUDFLARE_API_TOKEN`.

Every role tests only authorized targets, reads over writes, proves a finding
with a benign payload (never a destructive one), and never prints a real
secret. Each confirmed issue is recorded with the `report_finding` tool, which
shapes it the same way every time: severity, location, reproduction, impact and
fix. The distinct `security` audit remains as the `security` skill family, which
reads code rather than testing a running target. This is for assessing systems
you are authorized to test, such as your own project before release.

## Terminal commands

`/help` `/new` `/resume` `/agent` `/model` `/effort` `/provider` `/attach`
`/theme` `/typesafe` `/skills` `/mcp` `/compact` `/sidebar` `/reasoning`
`/tools` `/preview` `/status` `/clear` `/stop` `/retry` `/quit`

`/effort` chooses how hard the model thinks, from the levels models.dev lists
for it (`/effort high` picks one directly). The level shows beside the model,
under the composer and in the status bar, or `effort default` while the model
runs on its provider's default.

The transcript hangs each step's tool calls on a `├─ / └─` rail under the
request. Calls that only look around (read, grep, glob, fetch, skill reads)
fold into one `explored` row, and skill bindings into one `bound` row; click a
row to open it. While a turn runs, a line under the composer shows the mark's
middle row turning, what the turn is doing, and how long it has taken.

When the provider is down (502, 503, 429, a dropped connection), each call is
retried for about three and a half minutes. A reply the provider breaks off
mid-stream, or a stream that comes back empty, is asked for again up to three
times; one that still breaks tells the agent to send less at once (one file
per step, long files in parts), and the turn carries on. A turn that still fails continues
from where it stopped by itself, up to three times a minute apart; `/retry`
continues at once and `Esc` cancels the wait.

Keys: `Enter` sends, `Ctrl+Enter` inserts a newline, `/` opens the palette,
`Ctrl+R` toggles reasoning, `Ctrl+O` toggles tool output, `PgUp`/`PgDn` scroll
the chat, and `Ctrl+C` clears the composer, opens the quit prompt when the
composer is empty, or interrupts a running turn.

`Ctrl+T` steps to the next sidebar tab and `Alt+1`–`Alt+4` picks one without
consuming typed digits; `Ctrl+G` steps the log's filter and `Ctrl+X` toggles its
detail. Shortcuts are `Ctrl` combinations rather than function keys, which not
every terminal or OS passes through. `Ctrl+B` shows/hides the sidebar; on narrow terminals it opens over the
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
`Ctrl+F` to mark a favourite, `Ctrl+N` to add a model by hand, `Ctrl+E` to edit
a model's context window, thinking effort, vision and prices, `Ctrl+R` to ask
the providers for their lists again. A model is always named with its provider,
`deepseek/deepseek-flash`, and a pick is remembered in `~/.enx/model.json`
rather than written to `config.toml`. `/model <provider/model>` does the same
from the composer.

## Configuration

Three files under `~/.enx` (or `ENX_HOME`), each with one job:

| File | Holds |
|---|---|
| `config.toml` | Custom providers, a pinned start model, and every other setting. No keys |
| `auth.json` | One API key per provider, readable by the user alone. `enx auth login/logout` edits it |
| Cloudflare token | In `auth.json` under `cloudflare` (or `CLOUDFLARE_API_TOKEN`); proves domain control for a security assessment |
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
authorized assessment (`pentest*`), performance (`performance*`), reviewing (`review*`), research (`research*`),
gathering (`librarian`) and running large tasks (`orchestration`), plus
`code`, `writing`, `i18n` and `brainstorm` (agreeing a design, then the
plan documents the user chooses: PRD, DESIGN, ARCHITECTURE, ERD, API, PLAN,
written by the orchestrator with `plan_write`), each carried by the agents
whose work needs it and read only when the work does. A project or user skill of the same
name replaces one. A skill installed in the project or `~/` goes to every
agent until the orchestrator binds it, with `skill_bind` (every binding in one
call), to the agents whose work it serves; bindings are kept in `~/.enx/skill-bindings.json`, and the
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

### Releasing

Pushing a `v*` tag runs `.github/workflows/release.yml`: it builds `enx` for
the six platforms above and publishes the archives and their checksums as a
GitHub release. Running the workflow by hand builds without publishing.

```sh
git tag v0.1.0 && git push origin v0.1.0
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

Apache License 2.0. See `LICENSE` and `NOTICE`.
