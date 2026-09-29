---
name: security-supply-chain
description: "Dependency and supply-chain risk: lockfiles and pinning, vulnerability audits per ecosystem and whether the vulnerable code is reachable, typosquatting and install scripts, abandoned packages, licences, SBOMs, signed artefacts and provenance, CI pipelines as an attack surface, and an update policy. Read before auditing dependencies or a build pipeline."
---

# Dependencies and the build pipeline

The naive version pastes `npm audit` output (143 vulnerabilities, sorted by
CVSS), recommends `npm audit fix --force`, and misses what matters: the
`pull_request_target` workflow that hands a write token to any fork, the
action pinned to a tag someone can move, the new package with a
`postinstall` script and a name one letter off. This skill covers the
lockfile discipline, audits triaged by reachability, the packages that are
risky before any CVE exists, licences, SBOMs and provenance, CI as an attack
surface, and an update policy. The build-side rules are also in
`devops-security`; choosing a new package in `code` section 6.

## 1. Lockfiles and pinning

The lockfile is committed and the install in CI refuses to change it:

| Ecosystem | Lockfile | Install that honours it |
|---|---|---|
| npm | `package-lock.json` | `npm ci` |
| pnpm | `pnpm-lock.yaml` | `pnpm install --frozen-lockfile` (the default in CI) |
| Yarn 2 and later | `yarn.lock` | `yarn install --immutable` |
| Bun | `bun.lock` (text since 1.2) | `bun install --frozen-lockfile` |
| pip | `requirements.txt` with hashes | `pip install --require-hashes -r requirements.txt` |
| uv | `uv.lock` | `uv sync --locked` |
| Poetry | `poetry.lock` | `poetry install`, checked by `poetry check --lock` |
| Cargo | `Cargo.lock` (libraries too, by current guidance) | `cargo build --locked` |
| Go | `go.sum` | `go mod verify`; builds do not change `go.mod` by default |
| Bundler | `Gemfile.lock` | `bundle config set frozen true` |
| Composer | `composer.lock` | `composer install`, never `update` in CI |
| .NET | `packages.lock.json` | `dotnet restore --locked-mode` |

- Pin what is not a package too: GitHub Actions by full commit SHA with the
  version in a comment, base images by digest
  (`FROM node:22-slim@sha256:...`), tools fetched in CI or a Dockerfile by
  version and checksum. `curl ... | sh` of a moving URL is a finding.
- **Dependency confusion**: an internal package name that the public
  registry could also serve. Scope internal npm packages to the private
  registry (`@company:registry=` in `.npmrc`); pip's `--extra-index-url`
  lets a public package with a higher version win (use one index that
  proxies both, or uv, which takes a package from the first index that has
  it); check repository order in Maven and Gradle.

```sh
rg -n 'extra-index-url|--index-url|registry=|always-auth' --hidden -g '!node_modules' .
rg -n 'curl[^|]*\|\s*(ba|z)?sh|wget[^|]*\|\s*(ba|z)?sh' --hidden .
rg -n -i '^FROM\s+[^@\s]+(\s+as\s+\S+)?\s*$' -g '*Dockerfile*' .
```

## 2. Audits per ecosystem

```sh
npm audit --omit=dev                       # pnpm audit --prod; yarn npm audit --all --recursive
npm audit signatures                       # registry signatures and provenance
pip-audit -r requirements.txt --no-deps    # a fully pinned file, or plain pip-audit inside the venv
pip-audit --no-deps -r <(uv export --no-hashes --no-emit-project)
cargo audit                                # cargo deny check advisories bans licenses sources
govulncheck ./...                          # only vulnerabilities in code that is called
bundle audit check --update
composer audit
dotnet list package --vulnerable --include-transitive
osv-scanner scan source -r .               # every lockfile it knows (v1: osv-scanner -r .)
trivy fs --scanners vuln .
```

Each advisory is triaged, not counted:

1. **In production or only in development?** A ReDoS in a test runner or a
   bundler plugin is informational.
2. **Which path pulls it in?** `npm explain pkg`, `pnpm why pkg`,
   `yarn why pkg`, `cargo tree -i pkg`, `go mod why -m module`,
   `uv tree --invert --package pkg`, `pipdeptree -r -p pkg`.
3. **Is the vulnerable function called, with input an attacker controls?**
   The advisory (GHSA, OSV) names the function or the condition; read the
   call sites. govulncheck answers this for Go.
4. **Is it exploited in the wild?** On CISA's Known Exploited
   Vulnerabilities list: fix now. EPSS gives the probability of exploitation.
5. **What fixes it?** A direct upgrade, or an override for a transitive one
   (npm `overrides`, `pnpm.overrides`, Yarn `resolutions`, uv
   `override-dependencies`, Cargo `[patch]`, `go get module@version`), or
   removing the package.

Rate by the answers, not the CVSS: a reachable code-execution flaw in an
internet-facing path is critical; the same library with the function unused
is low, with an upgrade suggested. `npm audit fix --force` installs breaking
majors: never a recommendation without review.

## 3. Risky packages, before any CVE

- **Look-alike names**: one letter off, a swapped scope, a `-js` or `-py`
  suffix, and names invented by AI assistants that attackers then register
  (slopsquatting). Confirm every new package on its registry page and
  repository before adding or recommending it.
- **Takeovers**: event-stream (2018), ua-parser-js (2021), the xz-utils
  backdoor (2024), the chalk and debug takeover and the Shai-Hulud worm on
  npm (2025). Signs: a new maintainer, a burst of releases after years of
  quiet, a new install script, published code that differs from the
  repository.
- **Code that runs at install**: `preinstall`, `install`, `postinstall` and
  `prepare` scripts; `setup.py` in source distributions; Cargo `build.rs`
  and procedural macros. pnpm 10 and Bun run dependencies' scripts only for
  an allow-list (`onlyBuiltDependencies`, `trustedDependencies`); npm needs
  `ignore-scripts=true`.
- **A minimum release age** (pnpm and Renovate `minimumReleaseAge`, uv
  `--exclude-newer`) holds new versions back a few days, which is when most
  malicious releases are found and pulled.
- **Signals**: obfuscated code, network or environment access at install,
  binaries in the tarball, a repository link that does not match. Socket,
  OpenSSF Scorecard (`scorecard --repo=github.com/org/name`) and deps.dev
  show them.
- A package for a ten-line task is another maintainer to trust.

```sh
npm query ':attr(scripts, [postinstall])' | rg '"name"'
rg -l --no-ignore --hidden -g 'package.json' '"(preinstall|install|postinstall)"\s*:' node_modules
```

## 4. Abandoned packages

Last release years ago with open security issues, an archived repository,
one maintainer who has gone quiet, a deprecation notice
(`npm view pkg deprecated time.modified`). Informational or low on its own
(CWE-1104), higher when a known flaw will never be fixed; the finding names
a maintained replacement or the platform feature that makes it unnecessary.

## 5. Licences

- List them: `pnpm licenses list`, `pip-licenses`,
  `cargo deny check licenses`, `go-licenses report ./...`,
  `trivy fs --scanners license .`; ScanCode for a deep scan.
- Flag for the owner: GPL in distributed proprietary software; AGPL, whose
  obligations start at network use; SSPL, BUSL and the Elastic License,
  which restrict offering the software as a service; no licence at all (no
  permission); a licence that changed between versions (HashiCorp's BUSL in
  2023, Redis in 2024 and again in 2025).
- SPDX identifiers in manifests; a missing one is a note. This is not legal
  advice: the finding names the package, the licence and the concern.

## 6. SBOMs, signatures and provenance

- **SBOM** with each release: `syft dir:. -o cyclonedx-json`,
  `syft name:tag -o spdx-json`, or
  `docker buildx build --sbom=true --provenance=mode=max`. Scan it later as
  new advisories appear (`grype sbom:./sbom.json`).
- **Signatures**: Sigstore cosign, keyless from CI
  (`cosign sign image@sha256:...`), verified against the expected identity,
  not just "is signed":

```sh
cosign verify ghcr.io/org/app@sha256:<digest> \
  --certificate-identity=https://github.com/org/app/.github/workflows/release.yml@refs/heads/main \
  --certificate-oidc-issuer=https://token.actions.githubusercontent.com
```

- **Provenance**: SLSA build levels (L1 provenance exists, L2 signed by a
  hosted build platform, L3 a hardened build); GitHub's
  `actions/attest-build-provenance`, checked with
  `gh attestation verify file --repo org/app`; npm trusted publishing and
  `--provenance`, PyPI trusted publishing, so no long-lived registry token
  sits in CI.
- Downloads checked against a checksum or signature from the release, not a
  checksum served next to the file alone.

## 7. CI as an attack surface

```sh
rg -n 'pull_request_target|workflow_run|permissions:\s*write-all|secrets:\s*inherit|self-hosted|GITHUB_ENV|GITHUB_OUTPUT' .github/workflows
rg -n '\$\{\{\s*github\.(event\.(issue|pull_request|comment|review|head_commit|commits|pages)|head_ref)' .github/workflows
rg -n 'uses:\s*[^#\s]+@(v?\d+(\.\d+){0,2}|main|master|develop|latest)\s*($|#)' .github/workflows
```

- **Pwn requests**: `pull_request_target` or `workflow_run` that checks out
  and runs the pull request's code with secrets or a write token. Critical
  on a public repository.
- **Script injection**: titles, bodies, branch names, commit messages and
  author names interpolated with `${{ }}` inside `run:` are pasted into the
  script before the shell starts. Pass them through `env:` and quote
  `"$TITLE"`.
- **Token scope**: no `permissions:` (older repositories default to read
  and write) or `write-all`. Set `contents: read` at the top, widen per job.
- **Mutable references**: actions by tag or branch can be moved
  (tj-actions/changed-files, 2025); pin a full SHA. `secrets: inherit` hands
  every secret to a reusable workflow.
- **Runners and caches**: self-hosted runners on a public repository run
  anyone's pull request; caches written by untrusted triggers and restored
  in release jobs (the Ultralytics release, 2024); artefacts from pull
  request runs trusted by `workflow_run`; untrusted text written to
  `$GITHUB_ENV` sets variables for later steps.
- **Cloud trust**: an OIDC role that trusts
  `token.actions.githubusercontent.com` without a `sub` condition naming
  the repository and branch or environment can be assumed from other
  repositories (`security-infra`).
- GitLab and others: protected variables only on protected branches, the
  `CI_JOB_TOKEN` allow-list, no secrets in pipelines for forks.
- Tools: `zizmor .github/workflows`, `actionlint`, and Scorecard's
  Dangerous-Workflow, Token-Permissions and Pinned-Dependencies checks.

## 8. Update policy

- Renovate or Dependabot, with non-major updates grouped weekly, security
  updates at once, patch and minor merged automatically when tests pass,
  majors on a monthly cadence with the changelog read, lockfile maintenance
  weekly, and a minimum release age for everything but security fixes:

```json
{
  "extends": ["config:recommended"],
  "minimumReleaseAge": "3 days",
  "lockFileMaintenance": { "enabled": true },
  "vulnerabilityAlerts": { "minimumReleaseAge": null },
  "packageRules": [
    { "matchUpdateTypes": ["minor", "patch"], "groupName": "non-major", "automerge": true }
  ]
}
```

- Runtimes and base images count: an end-of-life runtime gets no security
  fixes (Node 20 since April 2026, Python 3.9 since October 2025; check
  endoflife.date for the rest).
- A named owner for the policy and a time to fix per severity that the team
  meets.

## Check it

- The lockfile exists for every manifest, is committed, and every install in
  CI and in the Dockerfiles uses the frozen form from the table.
- Every advisory from the audits has a triage line: production or dev, path,
  reachable or not, fix.
- `.github/workflows` has no pwn request, no interpolated event text in
  `run:`, top-level `permissions:`, and actions pinned by SHA.
- New or suspicious packages were checked on the registry and their
  repository, not judged by name.

## Avoid

Pasting audit output as findings; CVSS as the rating;
`npm audit fix --force`; recommending a package you have not seen on its
registry; a missing or ignored lockfile; `--extra-index-url` for internal
packages; actions by tag; `pull_request_target` with a checkout of the pull
request; `${{ github.event... }}` inside `run:`; install scripts allowed by
default; auto-merging brand-new releases the day they appear.
