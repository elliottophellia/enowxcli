---
name: research
description: "Answering a question with evidence: restating it precisely, deciding where the answer can be found, primary sources over secondary ones, checking versions and dates, confirming by running code when cheap, separating what was verified from what was inferred, and writing the answer first with its evidence and confidence. Read before any research question."
---

# Answering a question with evidence

The naive answer comes from memory about a library that has changed since,
cites a 2019 blog post as current, states a guess as fact, or buries the
answer under a wall of links. A researched answer pins down exactly what was
asked, finds it where it is decided (the code, the version in use, the
spec), keeps what was seen apart from what was reasoned, and puts the answer
in its first lines. The methods for the places answers live are in
`research-code`, `research-web` and `research-libraries`.

## 1. Restate the question

- One sentence, precise enough to be checked. "Does the session expire
  after inactivity?" becomes "In this app, is the session cookie's lifetime
  extended by activity, or fixed from sign-in?"
- Add what the answer depends on: the version in use, the runtime, the
  environment (local, CI, production), the configuration, the branch.
- Split compound questions ("why is sign-in slow, and is it safe?") and
  answer each on its own.
- A question you cannot ask back about (a delegated one) is answered for
  its most plausible reading, and the answer names the reading.

## 2. Decide where the answer lives

| Kind of question | Where it is decided | Method |
|---|---|---|
| How does our code do X? | the code, its config and its tests | `research-code` |
| Why is it like this? | commits, pull requests, ADRs, the changelog | `research-code` |
| What does this dependency do? | its installed source, docs for that version, its changelog | `research-code`, `research-web` |
| What does the platform or standard say? | the spec, MDN, browser support data | `research-web` |
| What does this error mean? | the source that raises it, the issue tracker | `research-web` |
| Which library should we use? | registries, repositories, a short trial | `research-libraries` |
| Is X true in our setup? | the config file itself | read it |

Look in the project before the web. The answer about this project is in
this project; a page on the web describes some other version of it.

## 3. Primary sources over secondary ones

| Source | Worth |
|---|---|
| The code that runs (the project, a dependency at its installed version), a command's output | Proof |
| Official docs for that version, the spec, the changelog, a maintainer's answer in an issue | Strong evidence |
| A merged pull request with a test | Strong, for the release it shipped in |
| Blog posts, Stack Overflow, tutorials, forums, generated answers | Leads: follow them to the rows above |
| Your own memory | A hypothesis to check |

Memory is the weakest source for anything that changes: library APIs and
defaults, versions, limits, prices, browser support, command-line flags.
Check each of those before stating it.

## 4. Versions and dates

- The version in use comes from the lockfile (`package-lock.json`,
  `pnpm-lock.yaml`, `yarn.lock`, `uv.lock`, `poetry.lock`, `Cargo.lock`,
  `go.mod`, `Gemfile.lock`, `composer.lock`), not from the manifest's
  range: `^4.2.0` may be 4.9.1 installed.
- Runtimes: `.nvmrc`, `.node-version`, `engines`, `.python-version`,
  `rust-toolchain.toml`, the `go` line in `go.mod`, the Dockerfile's `FROM`.
- Docs to match: a versioned URL, the site's version switcher, source at
  the tag (`/blob/v4.2.1/`) rather than `main`.
- Date every source. One older than a release that changed the behaviour
  describes the old behaviour: read the changelog between its version and
  yours before using it.

## 5. Verify cheaply

- Twenty lines of the implementation settle more than three articles: read
  the function that decides, in the project or in the dependency's installed
  source (`node_modules`, `site-packages`, `vendor`; see `research-code`).
- An existing test that asserts the behaviour, and runs in CI, is evidence.
  Cite it.
- When you can run commands, run the smallest one that decides: a
  `--version`, a one-line `node -e` or `python3 -c`, the project's test for
  that path. When you cannot (the `research` agent has no `bash`), write the
  exact command that would settle it and mark the claim inferred.
- A wide sweep (every caller of a function across thirty files) goes to
  `librarian`, which returns the excerpts and leaves your context for the
  reasoning.

## 6. Traps that make a checked answer wrong

- **Two copies of one dependency.** A lockfile can hold several versions of
  a package, one nested under another package; search it for every entry
  before saying which version runs.
- **The code on disk is not the code deployed.** For "what does production
  do", find the deployed version (the release tag, the image, the deploy
  config) and read that version (with git: `git show v1.4.2:<path>`).
- **Configuration decides.** An environment variable, a feature flag, a
  per-environment config file, or a default overridden in
  `docker-compose.yml`, `next.config.js` or `settings.py` changes the path.
- **One name, several definitions.** A subclass override, a monkey patch, a
  second route with the same path registered later, a module shadowed in a
  monorepo: find the one that wins.
- **Built output is not the source.** `dist/`, `build/` and `.next/` may be
  stale; read the source and say which you read.
- **Caches.** A CDN, a build cache or a memoised value can make what is
  observed differ from what the code says.

## 7. Triangulate what matters

- A claim the answer rests on gets two independent sources: the docs and
  the source, or the source and a test. A blog repeating the docs is not a
  second source.
- When sources disagree, the code at the version in use beats the docs,
  newer official sources beat older ones, and the answer states the
  contradiction.
- Look for evidence against your first guess as well. Stopping at the first
  source that agrees is how confident wrong answers are made.

## 8. Verified, inferred, unknown

- **Verified:** seen directly (a line read, a command's output, docs for the
  right version), with its citation.
- **Inferred:** follows from verified facts by reasoning you show: "the
  middleware never sets the cookie again, so activity cannot extend it."
- **Unknown:** what could not be settled, and what would settle it (a
  command, a log, a person who knows).
- "Is" and "does" only for verified claims; "probably" and "appears" only
  with the reason beside them.
- Confidence: high (a primary source for the right version, or a run),
  medium (inferred from verified facts, or a primary source for a nearby
  version), low (secondary sources only, or sources in conflict).

## 9. The answer

```
The session does not extend with activity: it expires 30 days after sign-in.

Evidence
- src/auth/session.ts:41  maxAge: 60 * 60 * 24 * 30, set once at sign-in
- src/auth/middleware.ts:18-35  reads the session; never sets the cookie again
- tests/auth/session.test.ts:77  asserts expiry on day 30 despite requests on day 29

Verified: the lifetime and where it is set (read). Inferred: activity cannot
extend it, since no path sets the cookie after sign-in.
Unknown: whether the proxy rewrites the cookie; the production response
headers would show it (curl -sI https://<host>/api/me).
Confidence: high.
```

- The answer in one to three sentences, first. Then the evidence: paths with
  lines, URLs with the version they describe and the date, command output
  quoted short. Then caveats and confidence, in a line or two.
- A web source is cited with its URL, what it documents, the version, the
  date read, and the sentence that carries the claim, quoted.
- Every link supports a claim in the answer; a link that supports nothing is
  noise.
- Quote the line that matters, not the page around it.
- **Delegated:** the caller receives only the text from `DONE:` on, and
  anything before it is dropped. Put the answer on the `DONE:` line with the
  evidence indented under it; then `CHANGED: none`, `VERIFIED:` what was read
  or fetched, and `NEXT:` the unknowns and what would settle them.

## 10. Stop, or say it cannot be answered

- Stop when the question is answered with the confidence it needs: a design
  decision needs more than a passing curiosity.
- A normal question takes 10 to 20 tool calls. Past that, answer with what
  you have and say what is left.
- When it cannot be answered: what you checked, what was missing, and the
  next step (a command, access, a person). Never fill the gap with a guess
  dressed as a finding.
- Nothing beyond the question: related facts nobody asked for dilute the
  answer.

## Check it

- The first lines answer the restated question, not a neighbouring one.
- Every factual claim has a citation or is marked inferred.
- Versions cited match the lockfile; each source's date was checked against
  the releases since.
- No unmarked claim from memory about something that changes.
- A secret met along the way is not quoted: name the file and line only.

## Avoid

Answers from memory about libraries, flags or limits; a 2019 post cited as
current; docs for another major version; guesses stated as facts; a wall of
links; stopping at the first source that agrees; a whole page quoted for one
line; padding with related facts; a question sent back when delegated,
instead of an answer to the plausible reading.
