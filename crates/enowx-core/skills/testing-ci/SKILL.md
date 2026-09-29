---
name: testing-ci
description: "Running tests in CI: ordering for fast feedback, parallel workers and sharding, caching dependencies, services for integration tests, test reports and annotations, coverage uploads, detecting and tracking flaky tests, test selection for large repositories, timeouts, and required checks. Read before setting up or speeding up tests in a pipeline."
---

# Tests in CI

The default pipeline is one job that installs everything and runs lint,
unit, integration and end-to-end tests in a row for 35 minutes, with no
cache, retries set to 3 so it goes green, a coverage badge nobody reads, and
failures that show as "exit code 1" at the end of a 10,000-line log. People
push, wait, rerun. This skill: fast feedback first, parallel jobs and
shards with isolated resources, caches, services with health checks,
reports that point at the failing line, coverage that means something,
flake tracking, selection for large repositories, timeouts, and required
checks that can neither be bypassed nor deadlock. Principles in `testing`.

## 1. Order for fast feedback

- First, in under 5 minutes: formatting, lint, type check, unit tests.
- Then, in parallel jobs: integration tests with their services, end-to-end
  shards, the build; the whole pull request pipeline in 10 to 15 minutes.
- `needs:` only for a real dependency; independent jobs start together.
  Runs superseded by a newer push are cancelled.
- CI calls the commands developers run (`pnpm test`, `make test`), so a
  failure reproduces with one command.

## 2. A workflow to start from (GitHub Actions)

```yaml
name: test
on:
  pull_request:
  push:
    branches: [main]
  merge_group:

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: ${{ github.event_name == 'pull_request' }}

permissions:
  contents: read

jobs:
  unit:
    runs-on: ubuntu-latest
    timeout-minutes: 10
    steps:
      - uses: actions/checkout@v7
      - uses: pnpm/action-setup@v6
      - uses: actions/setup-node@v7
        with:
          node-version-file: .nvmrc
          cache: pnpm
      - run: pnpm install --frozen-lockfile
      - run: pnpm lint && pnpm typecheck
      - run: pnpm vitest run --reporter=default --reporter=github-actions # failures annotated on the PR

  integration:
    runs-on: ubuntu-latest
    timeout-minutes: 15
    services:
      postgres:
        image: postgres:17-alpine
        env:
          POSTGRES_PASSWORD: postgres
          POSTGRES_DB: app_test
        ports: ["5432:5432"]
        options: --health-cmd "pg_isready -U postgres" --health-interval 5s --health-retries 10
    env:
      DATABASE_URL: postgres://postgres:postgres@localhost:5432/app_test
    steps:
      # checkout, pnpm, setup-node and install as in `unit`
      - run: pnpm db:migrate && pnpm test:integration

  e2e:
    runs-on: ubuntu-latest
    timeout-minutes: 20
    strategy:
      fail-fast: false
      matrix:
        shard: [1, 2, 3, 4]
    steps:
      # checkout, pnpm, setup-node and install as in `unit`
      - run: pnpm exec playwright install --with-deps chromium
      - run: pnpm exec playwright test --shard=${{ matrix.shard }}/${{ strategy.job-total }}
      - uses: actions/upload-artifact@v7
        if: ${{ !cancelled() }}
        with:
          name: blob-report-${{ matrix.shard }}
          path: blob-report
          retention-days: 7

  e2e-report:
    if: ${{ !cancelled() }}
    needs: [e2e]
    runs-on: ubuntu-latest
    steps:
      # checkout, pnpm, setup-node and install as in `unit`
      - uses: actions/download-artifact@v8
        with:
          path: all-blob-reports
          pattern: blob-report-*
          merge-multiple: true
      - run: pnpm exec playwright merge-reports --reporter html ./all-blob-reports
      - uses: actions/upload-artifact@v7
        with:
          name: playwright-report
          path: playwright-report
          retention-days: 14

  ci-ok:
    if: ${{ always() }}
    needs: [unit, integration, e2e]
    runs-on: ubuntu-latest
    steps:
      - if: ${{ contains(needs.*.result, 'failure') || contains(needs.*.result, 'cancelled') }}
        run: exit 1
```

Playwright uses the `blob` reporter on CI (`testing-e2e`). Actions at their
current major versions, pinned by commit SHA where policy asks.

## 3. Parallel workers and shards

- Inside a job the runners already parallelise: Vitest and Jest workers,
  `pytest -n auto` (pytest-xdist), `go test` packages, nextest, Gradle
  `maxParallelForks`, Playwright workers.
- Across machines once a suite passes about 10 minutes: Playwright, Vitest
  and Jest `--shard=1/4` (Vitest with `--reporter=blob`, then
  `--merge-reports`), pytest-split (`--splits 4 --group 1`, timings from
  `--store-durations`), nextest `--partition hash:1/4`, GitLab `parallel: 4`
  (`CI_NODE_INDEX`, `CI_NODE_TOTAL`), `circleci tests split --split-by=timings`.
- Split by timing data when durations vary: counting files leaves one shard
  with all the slow ones, and the slowest shard is the pipeline's time.
- Each worker and shard gets its own database (`app_test_1`), ports and key
  prefixes; a database shared by parallel workers is the usual source of
  CI-only failures (`testing-flaky`).

## 4. Caching

- Dependencies through the setup actions, keyed on the lockfiles:
  `actions/setup-node` with `cache: pnpm`, `actions/setup-python` with
  `cache: pip`, `astral-sh/setup-uv` with `enable-cache: true`,
  `actions/setup-go` (on by default), `actions/setup-java` with
  `cache: gradle`, `Swatinem/rust-cache@v2` for Cargo.
- Build and test caches where the tool has them: Go reports unchanged
  packages as `(cached)` (`-count=1` turns that off), the Gradle build
  cache, Nx and Turborepo remote caches, Bazel's remote cache.
- Do not cache Playwright browsers: restoring takes about as long as
  downloading (Playwright's own advice). Install the one browser needed, or
  use the `mcr.microsoft.com/playwright:v<version>-noble` image.

## 5. Services for integration tests

- Job services with health checks, as above, so tests start only when the
  database accepts connections; migrations run before the tests.
- Or Testcontainers in the job. On GitHub-hosted runners, service containers
  and Docker need an Ubuntu runner. Ryuk stays on; no container reuse.
- Image versions shared with production and the local compose file.

## 6. Reports that say what failed

- JUnit XML from every runner: Vitest `--reporter=junit`, `jest-junit`,
  `pytest --junitxml=reports/junit.xml`,
  `gotestsum --junitfile reports/junit.xml -- -race ./...`, nextest's
  `[profile.ci.junit]` (written to `target/nextest/ci/junit.xml`), Gradle
  and Maven by default (`build/test-results/`, `target/surefire-reports/`),
  `dotnet test --logger trx`, `phpunit --log-junit`,
  `rspec --format RspecJunitFormatter` (rspec_junit_formatter).
- Surfaced on the pull request as annotations: `dorny/test-reporter` or
  `mikepenz/action-junit-report`, or the runner's own GitHub reporter
  (Vitest `github-actions`, Playwright `github`). GitLab reads
  `artifacts:reports:junit`.
- Artefacts (JUnit files, the Playwright report and traces, screenshots,
  logs) uploaded with `if: ${{ !cancelled() }}`, kept 7 to 14 days.
- Quiet on success, detailed on failure (`pytest -q --tb=short`,
  `gotestsum --format testname`); counts in `$GITHUB_STEP_SUMMARY`.

## 7. Coverage

- Collected as lcov, Cobertura or JaCoCo and uploaded where the team reads
  it (Codecov, Coveralls, a pull request comment); shards merged first
  (Vitest's `--merge-reports` includes coverage; `coverage combine` in Python).
- Gate on the changed lines, not the total:
  `diff-cover coverage.xml --compare-branch=origin/main --fail-under=80`, or
  the service's patch status (diffs need `fetch-depth: 0` on checkout).
- Never lower a threshold to pass; never fail a pull request for a global
  dip caused by deleting tested code.

## 8. Flaky tests in CI

- Retry once, report passes on retry as flaky, and track them (each
  runner's switch is in `testing-flaky`).
- Follow flake rates over time: the CI's test analytics (Datadog Test
  Optimization, Buildkite Test Engine, Trunk Flaky Tests, CircleCI Test
  Insights, Develocity) or a job that keeps the JUnit history.

```toml
# .config/nextest.toml, run with: cargo nextest run --profile ci --partition hash:1/2
[profile.ci]
retries = 1
fail-fast = false
slow-timeout = { period = "60s", terminate-after = 3 }

[profile.ci.junit]
path = "junit.xml"
```

## 9. Selecting tests in large repositories

- Affected projects only on pull requests: `nx affected -t test --base=origin/main`,
  `turbo run test --affected`, Bazel with a remote cache, Pants
  `--changed-since=origin/main --changed-dependents=transitive test`.
- Affected files: `vitest run --changed origin/main`,
  `jest --changedSince=origin/main`, `pytest --testmon`,
  `playwright test --only-changed=origin/main`.
- Selection misses configuration, shared fixtures and dynamic imports: the
  full suite still runs on `main`, in the merge queue and nightly.

## 10. Timeouts

- Every job gets `timeout-minutes`, about 2 to 3 times its usual duration:
  GitHub's default is 360 minutes, and a hung test burns all of it.
- Every test has a limit: Vitest and Jest 5 s, Playwright 30 s per test and
  5 s per `expect`, Go 10 minutes per binary (`-timeout`), pytest none until
  pytest-timeout (`--timeout=60` dumps stacks), JUnit
  `junit.jupiter.execution.timeout.default`, nextest `slow-timeout`,
  `dotnet test --blame-hang-timeout 10m` (dumps the hung process).

## 11. Secrets and permissions

- Tests need no production secrets: services are faked at the edge; a
  sandbox key belongs only to the dedicated job that needs it.
- Forks get no secrets on `pull_request`; never switch to
  `pull_request_target` to run the pull request's code with secrets.
- `permissions: contents: read` at the top, widened per job (`checks: write`
  for a reporter). No environment dumps in logs; no `.env` files or
  Playwright storage state in artefacts.

## 12. Required checks

- Require the aggregate `ci-ok` job in branch protection or a ruleset
  instead of each job: matrix jobs create one check per value (`e2e (1)`),
  and renaming a job silently changes a required check's name.
- A workflow skipped by `paths` or `branches` filters leaves its required
  checks pending and blocks the merge; a job skipped by an `if:` reports
  success. For required workflows, detect changes inside the workflow and
  skip jobs with `if:`, not with trigger filters.
- A merge queue needs `merge_group` in the triggers, or checks never report.

## 13. Nightly and slow suites

- `on: schedule: - cron: "0 2 * * *"` (UTC): the full browser matrix, long
  property and fuzz runs, mutation testing, load tests, a monorepo's full
  suite.
- A nightly failure goes to an owner (an issue, an alert), not a red badge.
  GitHub disables schedules in public repositories after 60 idle days.

## Check it

- On a branch, add a failing unit test: an annotation points at it within
  minutes and the required check blocks the merge. Remove it.
- Job durations compared: the slowest shard against the others.
- The second run restores caches; the end-to-end report artefact opens,
  with traces.
- Branch protection requires `ci-ok`; `merge_group` is in the triggers if a
  queue is used; `actionlint` passes on the workflow files.

## Avoid

One serial job for everything; jobs without timeouts; retries raised to
hide flakes; one database shared by parallel shards; cached Playwright
browsers; path filters on required workflows; required checks named after
matrix values; a global coverage number as the gate; secrets in test jobs;
`pull_request_target` running pull request code; failures only in raw logs;
CI running tests differently from developers.
