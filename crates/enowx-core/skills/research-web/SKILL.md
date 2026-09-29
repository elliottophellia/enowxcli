---
name: research-web
description: "Finding and judging information on the web: official documentation for the right version, changelogs and migration guides, GitHub issues and discussions, specifications and MDN, package registries, forums with dates checked, avoiding outdated answers, reading source when docs are silent, and citing with URLs, versions and dates. Read before researching a library, an API, an error message or a standard online."
---

# Researching on the web

The naive search takes the first result: a 2019 Stack Overflow answer for a
library three majors ago, docs for the wrong version, an error message
paraphrased so nobody can find it again, a blog post quoted as if it were
the specification, links with no dates. This is where each kind of fact
lives, how to tell a current source from a stale one, and how to cite it so
the next person can check. The method for any question is `research`.

## 1. Know the version before you search

- Read it from the lockfile and the runtime files first (`research`,
  section 4). Every page you then read is for that version.
- A fact without a version is a fact about some version. "Next.js caches
  `fetch` by default" was true before Next.js 15 and not after it.

## 2. Official documentation for the right version

- Versioned docs have the version in the path or a switcher on the page:
  `docs.python.org/3.12/`, `docs.djangoproject.com/en/5.1/`,
  `nodejs.org/docs/latest-v22.x/api/`, `docs.rs/<crate>/<version>/`,
  `pkg.go.dev/<module>@<version>`. A page with no version describes the
  latest, which may not be yours.
- Migration guides ("Upgrading from 4 to 5") list the behaviour that
  changed, which is exactly where old answers go wrong.
- Hosted APIs have versions of their own: the one the project pins (a
  header, a dated API version, the SDK's default) decides which reference
  and which changelog entries apply.
- A site rendered by script comes back nearly empty from `fetch`. Try
  `/llms.txt` or `/llms-full.txt` (many documentation sites publish their
  docs as plain text there), the docs' markdown in the repository through
  `raw.githubusercontent.com`, or the README at the tag.

## 3. Changelogs and releases

- `CHANGELOG.md` at a tag:
  `https://raw.githubusercontent.com/<owner>/<repo>/<tag>/CHANGELOG.md`.
- The releases page, `https://github.com/<owner>/<repo>/releases`, and the
  compare view for everything between two versions:
  `https://github.com/<owner>/<repo>/compare/v4.2.0...v4.3.0`.
- Read every entry between the version a source used and yours for the
  behaviour in question: breaking changes, changed defaults, deprecations,
  fixes.

## 4. Issues and discussions

- Search the tracker, closed issues included:
  `https://github.com/<owner>/<repo>/issues?q=is%3Aissue+%22socket+hang+up%22`.
- Weigh by who answered: a maintainer's reply (the Member or Collaborator
  badge) and a linked fix (`Fixes #1234`) outrank the thread's guesses.
- Find the release the fix shipped in and compare it with yours.
- An open issue with many reactions is evidence of a known bug, not of a
  fix, and not of a workaround that works.
- Discussions and RFC threads explain why an API has the shape it has.

## 5. Specifications and the web platform

| Question | Source |
|---|---|
| HTML, DOM, Fetch, URL, Streams | WHATWG living standards: `html.spec.whatwg.org`, `fetch.spec.whatwg.org` |
| CSS | `drafts.csswg.org` (editor's drafts), `w3.org/TR/` (published) |
| Accessibility | WCAG 2.2 (`w3.org/TR/WCAG22/`), ARIA Authoring Practices (`w3.org/WAI/ARIA/apg/`) |
| HTTP and other protocols | RFCs at `rfc-editor.org`; read "Obsoleted by" at the top (RFC 9110 replaced 7231) |
| JavaScript | `tc39.es/ecma262/`; proposals and their stage at `github.com/tc39/proposals` |
| Web APIs in practice | MDN (`developer.mozilla.org`): behaviour, examples, compatibility |
| Browser support | caniuse.com; MDN's tables come from `mdn/browser-compat-data` |

- Baseline, shown on MDN and caniuse: "Newly available" means supported in
  every core browser; "Widely available" means that has held for 30 months.
  Use a feature without a fallback when it is widely available, or when the
  project's `browserslist` covers it.
- The spec says what should happen; MDN and the support data say what
  browsers do. A question about production behaviour needs both.

## 6. Package registries

| Ecosystem | Page | Data as JSON or text |
|---|---|---|
| npm | `npmjs.com/package/<pkg>` | `registry.npmjs.org/<pkg>/latest`, `api.npmjs.org/downloads/point/last-week/<pkg>` |
| PyPI | `pypi.org/project/<pkg>/` | `pypi.org/pypi/<pkg>/json`, `pypi.org/pypi/<pkg>/<version>/json` |
| Rust | `crates.io/crates/<name>`, `docs.rs/<name>`, `lib.rs/crates/<name>` | the crates.io API wants a user agent, so use the pages |
| Go | `pkg.go.dev/<module>` | `proxy.golang.org/<module>/@latest`, `proxy.golang.org/<module>/@v/list` |
| Maven | `central.sonatype.com/artifact/<group>/<artifact>` | `repo1.maven.org/maven2/<group as path>/<artifact>/maven-metadata.xml` |
| RubyGems | `rubygems.org/gems/<name>` | `rubygems.org/api/v1/gems/<name>.json` |
| Packagist | `packagist.org/packages/<vendor>/<pkg>` | `repo.packagist.org/p2/<vendor>/<pkg>.json` |
| NuGet | `nuget.org/packages/<id>` | `api.nuget.org/v3-flatcontainer/<id in lower case>/index.json` |
| Several | `deps.dev` | `api.deps.dev/v3/systems/<npm, pypi, cargo, go, maven or nuget>/packages/<name>` |

From a registry: the latest version and its date, how often releases come,
downloads (inflated by CI and transitive installs, so compare only within a
niche), the repository link (check it points where you expect), the
licence, deprecation notices, and provenance on npm. A scoped npm name is
encoded in deps.dev paths (`%40scope%2Fname`). Choosing between packages is
`research-libraries`.

## 7. Security advisories

- An advisory id: `osv.dev/vulnerability/<id>` (affected and fixed versions
  per ecosystem), `github.com/advisories/<GHSA id>`,
  `nvd.nist.gov/vuln/detail/<CVE id>`.
- Settle three things: whether the installed version is in the affected
  range, which release fixed it, and whether the project calls the
  vulnerable code at all. A score alone decides none of them.
- The project's release notes or security page for the fix and any
  workaround.

## 8. Forums and blogs, dated

- Stack Overflow, Reddit, Medium, dev.to and personal blogs are leads.
  Before using one, check its date, the version it used, the newer answers
  and comments under the accepted one, and whether it links to docs or
  source you can check.
- A source older than the release that changed the behaviour describes the
  past: say so, or drop it.
- Content farms and generated pages reword the docs and get the details
  wrong; go to the docs.

## 9. Read the source when the docs are silent

- The installed copy in the project first (`research-code`).
- Otherwise at the exact version:
  `raw.githubusercontent.com/<owner>/<repo>/<tag>/<path>`; npm files through
  `unpkg.com/<pkg>@<version>/<path>` or
  `cdn.jsdelivr.net/npm/<pkg>@<version>/<path>`; crates through
  `docs.rs/crate/<name>/<version>/source/`.
- Find the file from the repository's tree at the tag
  (`github.com/<owner>/<repo>/tree/<tag>/src`). GitHub's code search needs
  a signed-in browser, so it is out of reach for `fetch`: browse the tree,
  or grep the installed copy. The repository's own tests show the edge
  cases its authors meant to handle.

## 10. Error messages

- Search the message verbatim, in quotes, without what is yours (paths,
  ids, ports, times), with the library's name: `"socket hang up" undici`.
- Find where it is raised: grep the installed package for a fixed part of
  the message and read the condition around it. That is usually faster and
  more certain than any thread.
- Match an issue's version to yours before trusting its fix.

## 11. Judging a source

| Question | Good sign | Bad sign |
|---|---|---|
| Who wrote it | the maintainers, the spec's editors, the vendor | anonymous, a content farm |
| When | after the release you use | before a major that changed the behaviour |
| Which version | stated, and yours | not stated |
| Evidence | code, links to source or docs, steps to reproduce | assertion only |
| Agreement | matches the source code and the official docs | contradicts the code |

## 12. `fetch` in practice

- It reads one URL with a plain GET: no headers, cookies or sign-in, and
  only `http` and `https`. It is not a search engine: go straight to the
  page that holds the fact, built from what you know (the registry, the
  tracker's search URL, the docs path).
- HTML comes back as text, capped at 100 KB of text and 2 MB downloaded,
  with a 30-second timeout; binary files such as PDFs fail. Prefer raw and
  JSON endpoints: less noise, and they fit.
- A page cut at the cap: fetch the section's own page or the raw markdown.
- Pages behind a sign-in, a consent wall or client-side rendering come back
  empty or wrong; APIs that insist on a user agent or a token (the GitHub
  REST API) refuse the request. Take another route to the same fact and say
  which.
- Cite only pages you fetched and read, by the URL you fetched.

## 13. Citing

- Each source: the URL, its title or section, the version it documents, and
  the date published or read ("read 2026-09-29").
- Link to a tag or a commit, not a branch that moves:
  `github.com/<owner>/<repo>/blob/v5.2.0/src/pool.ts#L40-L58`.
- Quote the sentence or lines that carry the claim, briefly.
- State contradictions with both sides: "The v5 docs say X; the source at
  v5.2.0 does Y; for this project the code wins."

## Check it

- Every source's version matches the project's, or the gap is stated.
- Every source is dated, and none predates a release that changed the
  behaviour without the answer saying so.
- Every citation is a URL you fetched, pinned to a tag or version where one
  exists.
- The answer stands on primary sources; forums appear only as leads.

## Avoid

The first search result as the answer; docs for the latest version when the
project runs an older one; paraphrased error messages; Stack Overflow
answers used without their date; blog posts cited as the spec; links to
`main` that will drift; citing a page you did not fetch, or one that came
back empty; generated summaries instead of the docs.
