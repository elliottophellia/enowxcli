---
name: devops-ci
description: "Continuous integration that is fast and trustworthy: GitHub Actions structure, jobs for lint, typecheck, test and build, caching, matrices, concurrency, least-privilege permissions, pinned actions, secrets and OIDC, artifacts, required checks, preview deployments and release workflows. Read before writing or changing a CI pipeline."
---

# Continuous integration

The generated pipeline is one job that runs `npm install`, lint, tests and the
build in a row on every push, with no cache, on `ubuntu-latest`, with actions
pinned to `@v3` or `@master`, default token permissions, a cloud access key in
a repository secret, and a flaky test retried until it passes. It takes twenty
minutes and proves little. This is CI that answers in under ten minutes and can
be trusted. One part of devops; the whole is in the `devops` skill.

## 1. Structure

- One workflow per purpose: `ci.yml` (pull requests, `main`, the merge queue),
  `release.yml` (tags or the release PR), `deploy.yml` (after CI on `main`, or
  on a release), `preview.yml` (per PR, with cleanup); scheduled work apart.
- Independent jobs in parallel (lint, typecheck, test, build); `needs:` only
  where a job consumes another's output; no `continue-on-error` on a check.
- A pinned runner image (`ubuntu-24.04`): `ubuntu-latest` jumps to a new Ubuntu
  on GitHub's schedule. `timeout-minutes` on every job (10 to 20; the default
  is 360).
- Shared setup in a local composite action or a reusable workflow
  (`on: workflow_call`), and the same scripts developers run (`pnpm lint`,
  `make test`), so a red check reproduces on a laptop.

## 2. A pull request pipeline

```yaml
# .github/actions/setup/action.yml
name: setup
description: Install pnpm (version from packageManager), Node and locked dependencies
runs:
  using: composite
  steps:
    - uses: pnpm/action-setup@ea17c68df8912ef543352723c149a84f56e3d413 # v6.1.0
    - uses: actions/setup-node@820762786026740c76f36085b0efc47a31fe5020 # v7.0.0
      with:
        node-version-file: .nvmrc
        cache: pnpm
    - run: pnpm install --frozen-lockfile
      shell: bash
```

```yaml
# .github/workflows/ci.yml
name: ci
on:
  pull_request:
  push:
    branches: [main]
  merge_group:
permissions:
  contents: read
concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: ${{ github.event_name == 'pull_request' }}
jobs:
  check:
    strategy:
      matrix: { task: [lint, typecheck, build] }
    runs-on: ubuntu-24.04
    timeout-minutes: 10
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
        with: { persist-credentials: false }
      - uses: ./.github/actions/setup
      - run: pnpm run "$TASK"
        env:
          TASK: ${{ matrix.task }}
  test:
    runs-on: ubuntu-24.04
    timeout-minutes: 15
    services:
      postgres:
        image: postgres:18
        env: { POSTGRES_PASSWORD: postgres }
        ports: ["5432:5432"]
        options: >-
          --health-cmd "pg_isready -U postgres"
          --health-interval 5s --health-timeout 5s --health-retries 10
    env:
      DATABASE_URL: postgres://postgres:postgres@localhost:5432/postgres
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
        with: { persist-credentials: false }
      - uses: ./.github/actions/setup
      - run: pnpm test
      - uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a # v7.0.1
        if: ${{ !cancelled() }}
        with: { name: test-reports, path: reports/, retention-days: 14 }
  ci-ok:
    if: always()
    needs: [check, test]
    runs-on: ubuntu-24.04
    steps:
      - if: contains(needs.*.result, 'failure') || contains(needs.*.result, 'cancelled')
        run: exit 1
```

The service runs the production major version with a throwaway password. Since
July 2026 `uses: $/.github/actions/setup` reaches the repository at the running
commit without a checkout; zizmor suggests it, actionlint 1.7.12 rejects it.

## 3. Speed: under ten minutes

- Dependencies: `setup-node` with `cache: pnpm` (or `npm`, `yarn`),
  `astral-sh/setup-uv` with `enable-cache: true`, `setup-go` (caches by
  default), `Swatinem/rust-cache`; otherwise `actions/cache` keyed on the
  lockfile (`hashFiles('**/pnpm-lock.yaml')`). Cache the package manager's
  store, not `node_modules`.
- Images: `docker/build-push-action` with `cache-from: type=gha` and
  `cache-to: type=gha,mode=max`, or a registry cache
  (`type=registry,ref=ghcr.io/acme/app:buildcache,mode=max`) for large ones;
  layer order does the rest (`devops-containers`).
- Only what changed: `turbo run lint test build --affected` or
  `nx affected -t lint test build`, with remote caching when the team has it.
- Shard slow suites over a matrix (`vitest --shard=1/4`, `jest --shard=1/4`,
  `playwright test --shard=1/4`) and merge the reports.
- The cache holds 10 GB per repository by default and evicts entries unused
  for 7 days. The slowest job is the pipeline's time: read step timings
  before optimising.

## 4. Security

- `permissions: contents: read` at the top of every workflow; each job adds
  only what it needs (`id-token: write` for OIDC, `packages: write` for GHCR,
  `pull-requests: write` to comment).
- Every action pinned to a full commit SHA with its version as a comment.
  Tags move: in March 2025 the `tj-actions/changed-files` tags, and in March
  2026 76 of 77 `aquasecurity/trivy-action` tags, were rewritten to code that
  stole CI secrets; SHA pins were untouched. `pinact run` rewrites tags to
  SHAs, Renovate (`helpers:pinGitHubActionDigests`, in `config:best-practices`)
  keeps them current, and an organisation can require SHA pins by policy.
- `pull_request_target` and `workflow_run` run with secrets and a write token:
  never check out or run a pull request's code in them. `actions/checkout` v7
  refuses fork PR code there unless `allow-unsafe-pr-checkout` is set; leave
  it unset. Test contributions with plain `pull_request`.
- Script injection: a PR title, body, branch name, commit message or comment
  inside `${{ }}` in `run:` executes what a stranger typed. Pass it through
  `env:` and quote the variable, as `TASK` is passed above.
- `persist-credentials: false` on checkout unless a later step pushes, so the
  token is not left in `.git/config` for every later step and artefact.
- Secrets in GitHub environments (`production`, `staging`) with required
  reviewers and branch rules, referenced only by the job that needs them,
  never echoed (masking misses base64, JSON and URL-encoded forms). Fork PRs
  get no secrets and a read-only token; design around it.
- Cloud access through OIDC, never stored keys. Deploys queue, never cancel
  (a cancelled deploy leaves half a release):

```yaml
  deploy:
    runs-on: ubuntu-24.04
    environment: production
    permissions: { contents: read, id-token: write }
    concurrency: { group: deploy-production, cancel-in-progress: false }
    steps:
      - uses: aws-actions/configure-aws-credentials@e1253824e5c10ff9df46874f81ed3ec929e19cfd # v6.3.0
        with:
          role-to-assume: arn:aws:iam::123456789012:role/github-deploy-production
          aws-region: eu-west-1
```

  The role's trust policy pins `token.actions.githubusercontent.com:aud` to
  `sts.amazonaws.com` and `:sub` to
  `repo:acme/shop:environment:production` (`StringEquals`), so nothing else
  can assume it. Google Cloud: `google-github-actions/auth` with a workload
  identity provider; Azure: `azure/login` with a federated credential.
- Self-hosted runners only for private repositories, ephemeral (one job
  each); on a public repository any pull request runs code on them.

## 5. Concurrency, matrices, artefacts

- One `concurrency` group per workflow and ref: a pull request cancels its
  superseded runs, `main` runs finish, deploys queue (section 4).
- Matrices only for what the project supports: a library tests its runtime
  range (`engines`, `requires-python`), an application the one version it
  runs. `include` and `exclude` rather than a full cross product;
  `fail-fast: false` when every combination's result matters.
- JUnit XML uploaded with `if: ${{ !cancelled() }}` and shown with a reporter
  action or `$GITHUB_STEP_SUMMARY`; traces, screenshots and coverage kept 7 to
  14 days (`retention-days`; the default is 90). Coverage thresholds live in
  the test runner's config.
- What a later job deploys is built once (an image pushed by digest, or an
  uploaded artefact), never rebuilt.

## 6. Required checks and branch protection

- A ruleset on `main`: pull request required, CI required, no force pushes,
  squash or linear history, code owner review where `CODEOWNERS` exists.
- Require the aggregate job (`ci-ok`), not each matrix leg: a renamed leg
  cannot silently drop a required check. `on: pull_request: paths:` skips the
  whole workflow and leaves a required check pending; filter inside jobs
  (`dorny/paths-filter`) and let the aggregate job decide.
- A merge queue (the `merge_group` trigger) when `main` moves fast: each PR is
  tested on top of the ones ahead of it.
- Flaky tests are quarantined (skipped with a linked issue and an owner, or in
  a non-blocking job) and fixed; blanket retries hide real races.

## 7. Releases and previews

- Versions from tags (`v1.4.2`, semantic versioning): release-please opens a
  release PR from Conventional Commits, Changesets suits monorepos of
  packages, semantic-release is fully automatic, GoReleaser builds Go
  binaries.
- Build once, push by digest, attest it (`packages: write`,
  `id-token: write`, `attestations: write`), after `docker/login-action` and
  `docker/metadata-action`:

```yaml
      - id: build
        uses: docker/build-push-action@c3c9e263c25d99ce0380d002d59b67737d91b0dc # v7.4.0
        with:
          context: .
          push: true
          tags: ${{ steps.meta.outputs.tags }}
          labels: ${{ steps.meta.outputs.labels }}
          cache-from: type=gha
          cache-to: type=gha,mode=max
      - uses: actions/attest-build-provenance@4d101475d8b20a2381f78447822ac1eab6504dd8 # v4.2.2
        with:
          subject-name: ghcr.io/acme/shop
          subject-digest: ${{ steps.build.outputs.digest }}
          push-to-registry: true
```

  Image names are lowercase. Consumers check with
  `gh attestation verify oci://ghcr.io/acme/shop@sha256:<digest> --owner acme`.
- npm and PyPI packages publish through trusted publishing (OIDC), not
  stored tokens; npm revoked classic tokens in December 2025.
- Preview deployments per PR (native on Vercel, Netlify, Cloudflare; Coolify
  through its GitHub App, Render, Railway): their own URL, seeded data or a
  database branch (Neon, Supabase), never production data or secrets,
  removed when the PR closes (`types: [closed]`), behind the platform's auth
  when the app is not public.

## 8. GitLab CI and others

The same ideas: `rules:` for triggers, `needs:` for a graph, `cache:key:files`,
`interruptible: true`, protected variables, `id_tokens:` for OIDC, JUnit reports.

```yaml
workflow:
  rules:
    - if: $CI_PIPELINE_SOURCE == "merge_request_event"
    - if: $CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH
default:
  image: node:24-slim
  interruptible: true
  cache: { key: { files: [package-lock.json] }, paths: [.npm/] }
  before_script: [npm ci --cache .npm --prefer-offline]
test:
  script: [npm test]
  artifacts: { when: always, reports: { junit: reports/junit.xml } }
```

## Check it

- `actionlint` (or `docker run --rm -v "$PWD:/repo" -w /repo rhysd/actionlint:1.7.12`)
  and `uvx zizmor .github/workflows`: nothing new reported.
- `pinact run --check`: every `uses:` pinned to a SHA.
- Push a branch and watch it (`gh run watch`, `gh run view --log-failed`).
  Break one test on purpose once and see `ci-ok` block the merge.
- `gh run view <run-id>`: the slowest job under ten minutes, the second run
  faster than the first (the cache hits).
- `act` only for a quick local pass: it does not reproduce runners, caches or OIDC.

## Avoid

`npm install` instead of a frozen install; no cache, or `node_modules` cached
across lockfile changes; `ubuntu-latest` and actions on tags or branches;
missing `permissions`; `pull_request_target` running a contributor's code;
event text pasted into `run:`; cloud keys in repository secrets instead of
OIDC; secrets echoed to prove they are set; cancelled deploys; a required
check per matrix leg; retries that hide flaky tests; rebuilding for
production instead of promoting the tested digest; a pipeline called done
that never ran.
