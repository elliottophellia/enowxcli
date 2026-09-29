---
name: docs-changelog
description: "Changelogs and release notes: the Keep a Changelog format, semantic versioning, grouping changes by Added, Changed, Deprecated, Removed, Fixed and Security, writing for users rather than from commit titles, migration notes for breaking changes, dates, links, and generating drafts from conventional commits then editing them. Read before writing a changelog entry or release notes."
---

# Changelogs for the people who upgrade

The generated changelog is the git log pasted in: "fix stuff", "bump
deps", "Merge branch 'main'", "refactor", forty lines per release with the
one breaking change in the middle, no dates and no way to upgrade. Or
release notes that say "various bug fixes and performance improvements".
The reader wants to know whether to upgrade and what will break. This
skill gives them that: a `CHANGELOG.md` in the Keep a Changelog shape,
version numbers that mean something, entries in the reader's terms, and
tools that draft while a person edits.

## 1. The file

- `CHANGELOG.md` at the root of the repository (one per package in a
  monorepo), following Keep a Changelog 1.1.0: newest version first, an
  Unreleased section on top, each version with its date in ISO 8601,
  compare links at the bottom.
- Categories: Added, Changed, Deprecated, Removed, Fixed, Security; only
  those a version needs. Keep the order the project already uses.
  Whatever it is, a version with a breaking change or a security fix opens
  with one line saying so, so nobody has to read the whole list to find
  out.

```markdown
# Changelog

All notable changes to this project are documented here. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- `acme sync` skips a table without a primary key, with a warning, instead
  of stopping the run. ([#512](https://github.com/acme/acme-sync/pull/512))

## [2.0.0] - 2026-09-29

Two breaking changes: `--tables` takes a list, and `ACME_DB` is gone. See
[Upgrading to 2.0](docs/upgrading.md#to-20).

### Added

- `--dry-run` prints the rows that would be copied, without writing them.
  ([#476](https://github.com/acme/acme-sync/pull/476))

### Changed

- **Breaking:** `--tables` takes a comma-separated list instead of being
  repeated. ([#498](https://github.com/acme/acme-sync/pull/498))

### Removed

- **Breaking:** the `ACME_DB` environment variable, deprecated in 1.6.0.
  Use `SOURCE_URL`. ([#487](https://github.com/acme/acme-sync/pull/487))

### Security

- Connection strings, passwords included, are no longer written to the
  debug log. If you ran 1.x with `--log-level debug` and kept the logs,
  rotate the database password. (GHSA-xxxx-xxxx-xxxx)

[Unreleased]: https://github.com/acme/acme-sync/compare/v2.0.0...HEAD
[2.0.0]: https://github.com/acme/acme-sync/compare/v1.6.0...v2.0.0
```

- On GitHub, `#512` becomes a link in issues, pull requests and release
  notes, but not inside a file in the repository: write the link, or let
  the tool write it.
- A release withdrawn after publishing keeps its section, marked
  `## [1.4.1] - 2026-08-02 [YANKED]`, with the reason.

## 2. Semantic versioning

- `MAJOR.MINOR.PATCH` (SemVer 2.0.0): major for a change that breaks
  users, minor for a backwards-compatible addition or a deprecation, patch
  for a backwards-compatible fix.
- First settle what the public interface is, because "breaking" is
  measured against it:

| Project | Its public interface |
|---|---|
| Library | Exported types and functions and their behaviour, error types, supported runtimes |
| CLI | Commands, flags, output that scripts parse (`--json`), exit codes, the config file |
| HTTP API | Endpoints, fields, status and error codes, authentication |
| Application | Usually none in the semver sense: a date (CalVer, `2026.09.1`) or a build number is fine |

- Breaking: removing or renaming anything public; changing a default, a
  type, a return value, an error or an output format; making an optional
  input required. Dropping a runtime version (Node.js 20, Python 3.9)
  breaks those users: most projects make it a major. If yours makes it a
  minor, say so in the policy and in the entry.
- Before 1.0.0 anything may change. By convention, and by the caret rules
  of npm and Cargo (`^0.4.2` accepts 0.4.x from 0.4.2 on, never 0.5.0), a
  breaking change in 0.x bumps the minor. Release 1.0.0 once people depend
  on it in production.
- Pre-releases (`2.0.0-beta.1`, `2.0.0-rc.1`) sort before `2.0.0`. Either
  give each its own section, or gather their entries into the final
  release's section; pick one and keep to it.
- Deprecate in a minor, remove in a later major. The deprecation entry
  names the replacement and the version it goes away in, and the code
  warns at run time pointing to the same place.

## 3. Entries written for users

- Say what changed in terms the reader sees (a command, an option, an
  endpoint, a screen, a behaviour), why it matters to them, and what they
  must do, if anything.
- Describe the new behaviour in the present tense, starting with the thing
  that changed; the category heading already says added, fixed or removed.
- One entry per change a user can notice. Leave out what they cannot:
  refactors, CI, tests, internal renames, dependency bumps (unless the
  bump fixes a vulnerability, raises a minimum version or changes
  behaviour).
- Never "misc fixes", "various improvements" or "performance
  improvements". A performance change worth listing says where, and by how
  much, measured.
- A link to the pull request or issue on each entry; credit outside
  contributors if the project does.
- Never invent a change. Each entry traces to a merged pull request or
  commit you read. When a commit's effect on users is unclear, read its
  diff; if it is still unclear, ask, or mark the entry for the owner.

| Commit title | Entry |
|---|---|
| `fix(cli): handle empty table list` | Fixed: `acme sync --tables ""` exits with an error instead of copying every table. |
| `feat: add retry` | Added: a failed batch is retried up to 3 times before the run stops; `--retries` changes the count. |
| `chore(deps): bump openssl to 3.0.15` | Left out, unless it fixes a CVE: then under Security, with the id. |
| `refactor: split sync module` | Left out. |

## 4. Breaking changes and upgrade guides

- Each major version gets an upgrade guide (`docs/upgrading.md` or
  `UPGRADING.md`), linked from its changelog section. One section per
  breaking change: how to tell whether you are affected, then before and
  after, in code.

````markdown
### `--tables` takes a list

Affected if a script passes `--tables` more than once.

Before (1.x):

```sh
acme sync --tables orders --tables customers
```

After (2.0):

```sh
acme sync --tables orders,customers
```
````

- The order of the steps when it matters (upgrade the workers before the
  web servers), codemods or migration commands if you ship them, and how
  to roll back.
- A breaking change with no upgrade path is a decision to make before the
  release, not a line to write after it.

## 5. Security entries

- Under Security: what was vulnerable (the component and the condition,
  not an exploit recipe), who is affected (versions and configurations),
  the CVE or GHSA id once assigned, the fixed version, and what to do:
  upgrade, rotate credentials, check logs.
- Follow `SECURITY.md` and the disclosure timeline: an unfixed
  vulnerability is never described in a public changelog, commit or pull
  request before the fix and the advisory are out.

## 6. Release notes and the changelog

- The changelog is complete, per version, for the people upgrading, and
  kept in the repository.
- Release notes (the GitHub release, an announcement, a "What's new" page)
  are for people deciding whether the release matters to them: the two to
  five highlights, a sentence each on why they matter, an example or
  screenshot of the main one, the breaking changes with the upgrade link,
  then a link to the full changelog. The same facts in a friendlier order,
  with no hype.
- GitHub: `gh release create v2.0.0 --title "v2.0.0" --notes-file notes.md`.
  `--generate-notes` drafts from merged pull requests, grouped by the label
  categories in `.github/release.yml`; edit that draft before publishing.
- A product's "What's new" page for end users groups changes by month,
  with screenshots, in the product's own words. It is release notes, not a
  `CHANGELOG.md`.

## 7. Tools draft, a person edits

| Tool | Reads | Produces | Fits |
|---|---|---|---|
| release-please | Conventional commits on the main branch | A release pull request with the version bump and the changelog | Any language on GitHub, one package or a monorepo |
| Changesets | A file per pull request (`npx changeset`): the bump and a summary its author wrote | `npx changeset version` bumps versions and writes `CHANGELOG.md` | JavaScript and TypeScript monorepos |
| git-cliff | Git history, conventional or matched by patterns | A changelog from a template in `cliff.toml` | Any language: `git cliff --unreleased --prepend CHANGELOG.md` |
| release-plz | Conventional commits in a Cargo workspace | A release pull request and crates.io publishing, changelog through git-cliff | Rust |
| semantic-release | Conventional commits | Version, notes and publishing, with no human step | Only where generated notes can ship unedited |

- Conventional Commits 1.0.0: `feat:` bumps the minor, `fix:` the patch,
  and a `!` after the type or a `BREAKING CHANGE:` footer the major.
  `docs`, `refactor`, `test`, `chore`, `ci` and `build` bump nothing and
  are hidden from generated changelogs by default; tools differ on `perf`
  (semantic-release's default makes it a patch).
- A generated draft is a starting point: rewrite the titles for users,
  merge duplicates, drop internals, add the upgrade notes, then release.
- With Changesets the summary is the entry: write it for users when you
  create it.

## 8. Keeping it current

- Every pull request that changes something users notice adds its line
  under Unreleased (or its changeset), and the reviewer checks it.
- At release: rename Unreleased to the version and date, open a new empty
  Unreleased above it, update the compare links, tag.
- One source: when releases are generated, do not also keep a second list
  by hand.

## Check it

- Every entry traces to a pull request or commit in the release range
  (`git log --oneline v1.6.0..HEAD`), and every change users notice in
  that range has an entry. Without `bash`, work from the list of merged
  pull requests you were given, and say so.
- The version matches the entries: any breaking entry means a new major,
  or a new minor in 0.x.
- Compare links name tags that exist; dates are ISO 8601.
- Every breaking change has an upgrade section with before and after;
  every deprecation names its replacement and its removal version.
- No commit jargon (scopes, `chore`, bare hashes) and no "misc" entries.

## Avoid

The git log as a changelog; "bug fixes and improvements"; the breaking
change hidden in the middle of the list; a major version with no upgrade
guide; entries for refactors and CI; `#123` references that do not link;
describing a vulnerability before its fix is out; invented changes and
unmeasured performance claims; generated notes published without a person
reading them.
