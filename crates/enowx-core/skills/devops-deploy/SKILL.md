---
name: devops-deploy
description: "Deploying without drama: environments and their parity, deploy on merge or on tag, health and readiness checks, rolling, blue-green and canary releases, graceful shutdown, database migrations in the right order, feature flags, smoke tests after a deploy, and a one-step rollback. Read before setting up or changing how software is deployed."
---

# Deploying

The generated deploy is `ssh`, `git pull`, `npm install`, `pm2 restart` and
hope; or a platform that switches traffic to the new container the moment it
starts, before it can serve, while every instance runs the migrations at boot
and drops a column the old version still reads. Nobody knows which commit is
live, and rollback means reverting on `main` and waiting for CI. This is how
releases reach users gradually, prove they are healthy, and come back in one
step. One part of devops; the whole is in the `devops` skill.

## 1. Environments

| Environment | Purpose | Data | Deploys |
|---|---|---|---|
| Development | Laptops, Compose | Seeded, fake | Any time |
| Preview | One per pull request | Seeded, or a database branch | Each push to the PR; removed on close |
| Staging | The release candidate, shaped like production | Synthetic or anonymised, never a raw production copy | On merge to `main` |
| Production | Users | Real | After staging passes, automatically or with approval |

- Parity: the same image (by digest), runtime versions, variable names and
  kinds of backing service (Postgres in staging too, not SQLite). Only sizes,
  secrets and domains differ.
- Configuration per environment comes from the platform's variables and
  secret store, never from the image. Values a bundler inlines at build time
  (`NEXT_PUBLIC_*`, `VITE_*`) are public and force a build per environment:
  keep them few, or read them at runtime.
- Build once, promote: the digest tested in staging is the one production
  runs. Production lives in its own account or project with its own
  credentials.

## 2. When a deploy happens

- Continuous deployment by default: `main` deploys when CI is green, in small
  changes, often.
- Release trains (tags such as `v1.8.0`) for libraries, versioned public
  APIs, regulated products and teams that ship on a schedule.
- A human approval for production, where the organisation needs one,
  through the CI environment's protection rules (required reviewers, allowed
  branches), not a chat message.
- One deploy at a time per environment, queued, never cancelled halfway
  (`devops-ci`).
- Deploy when people are around to watch: working hours, not Friday evening
  or the night before a launch; freezes around big events are announced.
- Every release records the git SHA, the image digest, the trigger and the
  time. The app reports its version (`GET /version`, or a header) from a
  build argument, and the image carries `org.opencontainers.image.revision`.

## 3. Health: liveness, readiness, startup

| Check | Question | Fails when | Effect |
|---|---|---|---|
| Liveness (`/healthz`) | Is the process working at all? | Deadlock, stuck event loop | Restart |
| Readiness (`/readyz`) | Can it take traffic now? | Database unreachable, warming up, draining | Taken out of the load balancer, not restarted |
| Startup | Has it finished starting? | A slow boot | Liveness and readiness wait for it |

- The endpoints themselves are in `backend-observability`. Liveness never
  touches dependencies: one database blip would restart every instance.
- Rollouts wait on readiness: a new instance gets traffic only once ready,
  and an old one leaves only after that.
- Wire them into the platform: Kubernetes probes (`devops-kubernetes`), Fly
  `[[http_service.checks]]`, Render `healthCheckPath`, Railway
  `healthcheckPath`, a Coolify health check or Dockerfile `HEALTHCHECK`, ALB
  target group checks for ECS, Cloud Run startup and liveness probes.
- Numbers: every 5 to 10s, 2 to 3s timeout, 2 to 3 failures to mark
  unhealthy; a startup allowance of twice the slowest real boot.

## 4. Release strategies

| Strategy | How | Needs | Choose when |
|---|---|---|---|
| Rolling | Replace instances a few at a time (`maxSurge: 25%`, `maxUnavailable: 0`) | Readiness; old and new side by side | The default |
| Blue-green | Start a full new set, switch all traffic, keep the old set warm | Double capacity for a while | You want an instant switch back (Fly `bluegreen`, ECS, Argo Rollouts) |
| Canary | 1 to 5% of traffic, watch, then 25, 50, 100% | Traffic splitting, metrics per version | High traffic where a bad release is expensive (Argo Rollouts, Flagger, Cloud Run traffic split, Fly `canary`) |
| Recreate | Stop the old, start the new | Accepting seconds of downtime | One instance holding a volume or a lock |

- Everything but recreate runs two versions at once, so APIs, message
  formats and schemas stay backward compatible for the rollout and for a
  rollback.
- A single writer on a volume (SQLite, an embedded store, a process that
  locks its data directory) cannot overlap with its successor. Fly allows only
  rolling or immediate with volumes, a Render disk disables zero-downtime
  deploys, and Coolify starts the new container before stopping the old one
  unless "Consistent container name" is on. Accept stop-then-start, or move
  the state into a database.
- Canary analysis compares the canary with the baseline for 10 to 30 minutes
  per step (error rate, p95 latency, saturation) and aborts on a threshold
  such as twice the baseline error rate.

## 5. Graceful shutdown

On every deploy, each old instance:

1. is marked not ready and removed from the load balancer;
2. receives `SIGTERM` (in Kubernetes at the same time as step 1, so a
   `preStop` sleep of 5 to 10s lets routing catch up first);
3. stops accepting connections, finishes requests in flight, lets workers
   finish or release their current job, closes pools and exits 0
   (`backend-observability`);
4. is killed with `SIGKILL` when the grace period ends.

| Platform | Grace after `SIGTERM` | Setting |
|---|---|---|
| Docker, Compose | 10s | `docker stop -t`, `stop_grace_period` |
| Kubernetes | 30s | `terminationGracePeriodSeconds` |
| ECS | 30s (Fargate up to 120s) | `stopTimeout` |
| Cloud Run | 10s | fixed |
| Render | 30s (up to 300s) | `maxShutdownDelaySeconds` |
| Railway | configurable | `drainingSeconds` |
| Fly.io | 5s, and the signal is `SIGINT` | `kill_timeout`, `kill_signal = "SIGTERM"` |

- The app's own drain deadline sits below the platform's (for example 20s
  under Kubernetes' 30s).
- Load balancers drain too: an AWS target group's deregistration delay is 300s
  by default; set it to 30 to 60s to match.
- WebSockets and streams: the server closes them at shutdown with
  going-away (code 1001) and clients reconnect with backoff and jitter, to a
  new instance.

## 6. Database migrations

- Once per deploy, from one place, before the new code takes traffic: the
  platform's release step (Fly `release_command`, Render and Railway
  `preDeployCommand`, Coolify's pre-deployment command, Heroku `release:`), a
  Kubernetes Job or chart hook, or a CI step. Never from every instance at
  boot: they race, and a failing migration becomes a crash loop. If boot is
  the only place, take a lock first (`pg_advisory_lock`).
- Additive changes (a table, a nullable column, an index built
  `CONCURRENTLY`) ship before the code that uses them; the old code ignores
  them.
- Breaking changes go through expand and contract over several releases: add
  the new shape, write both, backfill in batches, read the new, stop writing
  the old, drop it later (`database-migrations`).
- A time limit on each migration (`SET lock_timeout = '5s'`, a
  `statement_timeout`), so a blocked `ALTER TABLE` fails fast instead of
  queueing every query behind it.
- A failed migration fails the deploy: the new version never starts, the old
  one keeps serving.
- Before anything destructive, a fresh backup or confirmed point-in-time
  recovery. Down migrations rarely work on real data; plan to roll forward.

## 7. Feature flags

- Deploying ships code, releasing turns it on. Merge behind a flag, enable it
  for staff, then 5%, then everyone.
- Kill switches for risky paths (a new payment provider, an expensive
  report): off in seconds, without a deploy.
- OpenFeature in the code, with a provider behind it (Unleash, Flagsmith,
  GrowthBook, LaunchDarkly, PostHog, ConfigCat) or a settings table in a
  small app. Anything that guards access is evaluated on the server.
- Each flag has an owner and a removal date; both paths are tested until it
  goes.

## 8. After the deploy

```sh
#!/usr/bin/env bash
# smoke.sh: run against the new release; any failure fails the deploy job
set -euo pipefail
base="${BASE_URL:?}"
curl -fsS --retry 10 --retry-delay 5 --retry-all-errors "$base/healthz" > /dev/null
curl -fsS "$base/readyz" > /dev/null
test "$(curl -fsS "$base/version")" = "${GIT_SHA:?}"
curl -fsS "$base/" > /dev/null
curl -fsS "$base/api/products?limit=1" > /dev/null
```

- Watch for 15 to 30 minutes: error rate, p95 latency, saturation, and new
  issues in the error tracker for this release, with deploy markers on the
  dashboards (`devops-observability`).
- Automatic rollback where the platform offers it: the ECS deployment
  circuit breaker with `rollback: true`, Argo Rollouts analysis, Coolify
  keeping the old container when the new one fails its check. A plain
  Kubernetes rollout only stalls (old pods keep serving) and reports failure
  after `progressDeadlineSeconds`; `kubectl rollout status` exits non-zero so
  the pipeline can undo it.

## 9. Rollback

- The previous release stays deployable: images tagged with the git SHA,
  never overwritten, the last 10 kept in the registry (and on the host, for
  self-hosted platforms).
- One step: revert the GitOps commit (or `kubectl rollout undo
  deployment/api` when nothing reconciles), `vercel rollback`,
  `heroku rollback`, the dashboard on Render or Coolify, `fly deploy --image`
  with the previous image, the previous task definition revision on ECS.
- Data is the hard part. Code rollback is safe only while the schema still
  suits the old code, which is why expand and contract exists. After a
  destructive migration, rollback means a restore and lost writes; say so
  before the deploy, not after.
- Roll back first, debug second: when errors climb right after a release,
  revert, then investigate.
- Practise it in staging every quarter, and time it.

## 10. Telling people

- A changelog generated from merged PRs (release-please, Changesets), plus
  human-written notes for changes users will notice.
- Deploy notifications in the team channel: SHA, author, link to the diff.
- During an incident, the status page says what is affected and when the
  next update comes, and keeps that promise.

## 11. The agent's part

Prepare and validate deploy configuration; deploy to preview or staging only
when asked; never trigger a production deploy, rollback or migration unless
the task says so. The report states what will deploy, what gates it, which
migration runs and whether it is backward compatible, and how to roll back.

## Check it

- Config validated with the platform's tool (`fly config validate`,
  `docker compose config -q`, `kubectl apply --dry-run=server`,
  `helm template`).
- In staging, deploy while a load loop runs (`oha -z 2m <url>/healthz`, or
  `k6`): a rolling deploy produces no errors.
- Ship a build that fails readiness to staging once: the rollout stops and
  the old version keeps serving.
- Run the rollback in staging and time it; `curl <url>/version` shows the
  SHA you expect after each step.
- Run the migration against a production-sized copy first, and note its
  duration and the locks it takes.

## Avoid

`git pull` on servers; traffic before readiness; liveness checks that call
the database; migrations run by every instance at boot; a destructive schema
change in the same release as the code that stops using the column; mutable
tags that make the previous release unknown; rebuilding for production;
cancelled deploys; no `preStop` sleep in Kubernetes; Fly's 5-second `SIGINT`
default left in place; "revert and wait for CI" as the rollback plan; flags
that never get removed; triggering production deploys on your own initiative.
