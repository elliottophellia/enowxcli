---
name: docs
description: "Documentation that people can use: knowing the reader and their first question, the four kinds of documentation (tutorials, how-to guides, reference, explanation) and not mixing them, accuracy by reading and running the code, examples that work, structure, maintenance next to the code, and which skill holds each kind. Read before writing or reorganising documentation."
---

# Documentation people can use

The generated version is easy to spot: a README whose first line restates
the project name as a sentence, a feature list of adjectives, install steps
that fail on a clean machine, API pages generated from names with no
example, "simply" in every step, a docs site where a tutorial turns into an
options table halfway down, and screenshots of a screen that changed two
releases ago. This skill is the method behind documentation that holds up:
who reads it, which kind of page they need, how every fact is made true,
and which skill covers each kind in depth. Words and sentences are in
`writing`; this goes further, for documents.

## 1. Start from the reader

Before writing, name the reader, their goal and what they already know, in
one line of your own notes: "A backend developer adding our webhooks to an
existing app; knows HTTP, has never seen our signature scheme." Whatever
does not serve that reader belongs on another page.

| Reader | Arrives asking | The first lines give them |
|---|---|---|
| Evaluator | What is this, is it for me? | What it does, for whom, and when not to use it |
| New user | How do I get it running? | Prerequisites, install, a quick start that works |
| Integrator | How do I call this, what comes back? | The reference entry, with an example and its errors |
| Contributor | How do I build, test and change it? | Setup, the test command, where the code lives |
| Operator | It is broken or needs deploying: what now? | The first action, then the checks |

- Answer the first question in the first lines: on a README the what and
  the install, on a how-to the result and the prerequisites, on a
  reference entry the signature, on a runbook the first action.
- Assume the reader arrived from search, in the middle of a task, on this
  page alone. Each page says what it covers and links what it depends on,
  rather than relying on the page before it.
- One level of reader per page, with what it assumes said at the top ("You
  can run a Docker container"): a tutorial explains its terms, a reference
  page does not.

## 2. Four kinds, one kind per page

Diátaxis (diataxis.fr) sorts documentation by what the reader is doing:

| Kind | The reader is | The page gives | Title |
|---|---|---|---|
| Tutorial | Learning, new | One path to a working result that always succeeds | "Build your first ..." |
| How-to guide | Working, knows the basics | The steps to finish one real task | "Rotate an API key" |
| Reference | Working, looking something up | Complete, exact, consistent, dry facts | The thing's name: "`acme sync`" |
| Explanation | Studying, stepping back | Why it works this way, trade-offs, history | "How retries work" |

- Mixing is the most common fault: a tutorial that stops to list every
  option (link to the reference), a reference entry with a paragraph of
  design history (move it to an explanation page), a how-to that teaches
  the basics (link to the tutorial). When a step wants explaining, write
  one sentence and a link.
- A page that has to be two kinds is two pages that link to each other.
- Not every project needs all four. A small library needs a README with a
  quick start, and reference; add the others when readers start asking the
  questions they answer.
- The other genres map onto these: a README is a landing page
  (`docs-readme`); a changelog is reference over time (`docs-changelog`);
  a runbook is a how-to for an operator, and a design doc or an ADR explains
  a decision (`docs-architecture`); troubleshooting is how-to keyed by
  symptom; an FAQ only collects questions people really asked, each answer
  linking to the page where it belongs.

## 3. Accuracy: read, run, copy

Docs are wrong in the details that matter most: a default, a flag, an
environment variable, a version. Every such fact comes from the code or from
running it, never from a name or from memory.

| Fact | Where it is true |
|---|---|
| Package name, install command | `name` in `package.json`, `[package]` in `Cargo.toml`, `[project]` in `pyproject.toml`, the module path in `go.mod`, the registry page |
| Minimum versions | `engines`, `requires-python`, `rust-version`, the `go` line in `go.mod`, `.nvmrc`, `.tool-versions`, `mise.toml`, the CI matrix |
| Commands and scripts | `scripts` in `package.json`, `Makefile`, `justfile`, `Taskfile.yml`, and above all the CI workflow: what CI runs is what works |
| Environment variables | `.env.example`, and the code that reads them: `process.env`, `import.meta.env`, `os.environ`, `os.getenv`, `std::env::var`, `os.Getenv`, `env()` in Laravel config |
| Options and defaults | The config schema or struct: zod `.default()`, pydantic `Field(default=...)`, `#[serde(default)]` with its `impl Default`, the Go constructor that fills defaults |
| CLI flags | The parser: clap `#[arg]`, argparse `add_argument`, click or typer options, cobra `Flags()`, commander `.option()`; or the binary's `--help` |
| Endpoints and shapes | The router and the request and response schemas; the OpenAPI document when one is generated |
| Error messages | A search for the exact string |
| Supported platforms | The CI matrix and the release targets |

- Never describe behaviour from a name. `retry()` may retry three times
  with backoff, or once and immediately; `--force` may skip one check or
  all of them. Open the function.
- Run what you document when you have `bash`: install in an empty
  directory, paste the quick start, copy the output. Without `bash`, trace
  each command to where it is defined, and list in your report every
  command you could not run, so it can be run before release.
- Copy output, never retype it. Trim it with a line holding only `...`.
- When behaviour differs between versions, say which version the page
  describes ("Since 2.3, ...").
- What you could not verify is marked where the owner will see it,
  `[TODO: confirm the default timeout]`, and listed in your report. A
  plausible guess reads as a fact and gets copied into production.

## 4. Examples that work

- Show first: the example comes before the paragraph about it.
- Minimal and complete: only what the point needs, but with the imports,
  the setup and the call, so it runs when pasted.
- Realistic values: `ada@example.com`, `ord_7Hq2Lx`, `2026-09-29`; not
  `foo`, `string`, `test123` or `0`. Reserved names cannot reach anything
  real: `example.com`, `example.org` and the `.test` domain (RFC 2606); IPs
  from `192.0.2.0/24`, `198.51.100.0/24` and `203.0.113.0/24` (RFC 5737)
  and `2001:db8::/32` (RFC 3849).
- The expected output after the example, in its own block.
- No `$` prompt in blocks readers copy; output in a separate block.
- Placeholders that look like placeholders, `<your-api-key>` or
  `$ACME_API_KEY`, explained under the block. Never a real secret, and no
  fake that matches a real key format (`sk_live_...`, `AKIA...`): secret
  scanners flag it, and readers paste it.
- Tested where the tooling allows: doctests (Rust, Python, Go examples)
  and snippets included from files that CI compiles (`docs-api`,
  `docs-sites`). One example that runs is worth more than three fragments
  that do not.

## 5. Structure

- Headings name the reader's task or question: "Install", "Connect to
  PostgreSQL", "Why is a job retried?". Not "Overview", "Introduction",
  "Miscellaneous" or "Conclusion".
- The first sentence under a heading carries its point, so a reader who
  skims the first line of each section still gets the page.
- Numbered lists for steps (one action each), bullets for facts in no
  order, tables for options (name, type, default, description), code
  blocks with a language tag.
- Short sections. A page past about ten screens is usually two pages.
- One home per fact, linked from everywhere else. Copies drift apart and
  then contradict each other.
- One `h1`, no skipped levels, sentence case. Headings are anchors people
  link to: rename one only with a reason, and fix the links to it.

Where each piece lives:

| The reader needs | Place |
|---|---|
| What it is, install, quick start | `README.md` |
| How to build, test and send a change | `CONTRIBUTING.md` |
| What changed in each version | `CHANGELOG.md` |
| How to report a vulnerability | `SECURITY.md` |
| Guides, concepts, the full reference | `docs/` or a docs site |
| Decisions and their reasons | `docs/adr/` |
| A map of the code for new engineers | `ARCHITECTURE.md` |
| Why one line is the way it is | A comment beside it |
| The public contract | Doc comments and the OpenAPI document |

## 6. Style for documents

The sentence rules are in `writing`. On top of them:

- Second person, present tense, active voice: "The command creates a
  branch", not "A branch will be created".
- Steps in the imperative, one action per step: "Run", "Open", "Set".
- Never "simply", "just", "easy", "easily", "obviously", "of course",
  "clearly", "basically", "straightforward" or "trivial". They tell the
  reader who is stuck that the fault is theirs, and tell everyone else
  nothing.
- Exact names in code format: commands, flags (`--dry-run`), file paths,
  environment variables, functions. Interface labels in bold, exactly as
  the screen shows them: **Settings > API keys**.
- The code's name and the reader's name for a thing can differ: give both
  once ("workspaces, called `tenant` in the API"), then keep to one.
- Name the actor: "the server retries", "enowx stores". Not "we", which
  could mean the company, the maintainers or the code.
- Numbers as digits with units (30 seconds, 5 MB), dates as `2026-09-29`.
  No "new", "recently" or "currently": they age without anyone noticing.
- Plain English for a global audience: no idioms, "for example" rather
  than "e.g.", short sentences that survive machine translation.
- With no house style, settle questions with the Google developer
  documentation style guide or the Microsoft Writing Style Guide, and keep
  to the one you chose.

## 7. Maintenance

- Docs live next to the code and change in the same pull request: a change
  to a behaviour, flag, default or message updates its page, its examples
  and the changelog's Unreleased section.
- Generate what can be generated, so it cannot drift: the API reference
  from the OpenAPI document or doc comments, the CLI reference from the
  parser, the configuration table from its schema. Write the prose around
  it by hand.
- Owners: a CODEOWNERS entry for `docs/` and for the pages a team owns.
- A "Last verified" date and version on pages that go stale: runbooks,
  integration guides, pages with screenshots.
- Delete what is wrong and will not be fixed. A stale page is worse than a
  missing one, because it is trusted.
- In CI: build the docs strictly, check links, run doctests and snippet
  tests, lint the prose (`docs-sites`).
- When a page exists only to explain a confusing behaviour, say in your
  report whether a clearer error message, flag name or default would
  remove the need for it.

## 8. The workflow

1. Name the reader, their goal and the kind of page.
2. Read the existing docs on the area (`glob` for `**/*.md` and
   `docs/**`): follow their structure and terms, and note contradictions to
   fix or report.
3. Read the code the page describes, starting from the entry point the
   reader uses (the command, the endpoint, the exported function), and
   note each fact with the file it came from.
4. Write the examples first, and verify them.
5. Write the page around the examples. Cut every section with nothing
   specific to say.
6. Check it, below.

## 9. Which skill for which job

- `docs-readme`: READMEs for libraries, applications, CLIs and internal
  services.
- `docs-api`: OpenAPI, the concepts an HTTP API shares (authentication,
  errors, pagination, rate limits, versioning, webhooks), SDKs, doc
  comments on a library's public interface, GraphQL.
- `docs-guides`: tutorials, getting-started pages, how-to guides,
  troubleshooting.
- `docs-changelog`: `CHANGELOG.md`, semantic versioning, release notes,
  upgrade guides.
- `docs-comments`: code comments, TODOs, commit messages, pull request
  descriptions.
- `docs-architecture`: design docs, ADRs, diagrams as code, system
  overviews, runbooks, glossaries.
- `docs-sites`: generators, navigation, search, versioning, docs CI.
- `writing` for words and sentences; `i18n` for translated text;
  `ui-page-docs` for the layout of a documentation page.

## Check it

- Every command, flag, variable, default and version on the page traces to
  a file you read or a command you ran; your report lists what was not
  run.
- Every relative link points to a file that exists (`glob` it) and every
  anchor to a heading that exists; every symbol you name exists (`grep`
  it).
- A case-insensitive `grep` for
  `\b(simply|just|easy|easily|obviously|basically)\b` over the files you
  wrote finds nothing that should go.
- Code blocks carry a language tag, copyable commands have no prompt,
  placeholders are explained, and no secret appears anywhere.
- The first lines answer the first question, and each page is one kind.
- The docs build and its checks pass, or your report says they were not
  run.

## Avoid

Describing behaviour guessed from a name; examples that do not run;
"simply" and "just"; an "Overview" that repeats the title; a tutorial that
turns into a reference table; the same fact kept in three places;
screenshots of an old interface; invented numbers, features, users or
benchmarks; docs updated in a later pull request than the code; unverified
claims left unmarked.
