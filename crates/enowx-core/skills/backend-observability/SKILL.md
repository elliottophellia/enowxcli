---
name: backend-observability
description: "Running a backend: configuration read and checked at start, structured logs with request ids, health and readiness checks, graceful shutdown, timeouts, metrics and error tracking when the project has them. Read when starting a service, adding config, or making one easier to run and debug."
---

# Running it

One part of the backend; the whole is in the `backend` skill.

## 1. Configuration

- Read every setting from the environment once, at start, in one module,
  checked with a schema (zod, pydantic-settings, `envconfig`, the framework's
  config): types parsed, required values present, URLs valid. A missing or
  malformed value stops the process at start with a message naming it, not a
  crash on the first request that needs it.
- The rest of the code imports that module; nothing else reads
  `process.env` or `os.environ` (and in Laravel, `env()` is called only in
  `config/`).
- `.env.example` lists every variable with a comment and a safe example;
  defaults only for values that are safe everywhere (a port, a log level),
  never for secrets.

## 2. Logs

- Structured (JSON in production, readable in development) through the
  project's logger (pino, structlog or the standard `logging` with a JSON
  formatter, `slog`, `tracing`, Monolog), never bare `console.log` or
  `print`.
- One line per request at the end: method, route pattern (not the raw URL
  with ids and tokens), status, duration, request id, user id. Plus a line for
  each decision worth finding later (a payment captured, a job failed), not
  for every step.
- A request id on every line of a request: taken from `X-Request-Id` when a
  trusted proxy sets it, generated otherwise, returned in the response, and
  passed on to jobs and outbound calls.
- Levels mean something: `error` needs someone to look, `warn` is unusual but
  handled, `info` is the story of normal work, `debug` is off in production.
- Never passwords, tokens, keys, card numbers or whole personal records
  (`backend-security`).

## 3. Health

- `GET /healthz` (liveness) answers `200` when the process can serve at all,
  without touching dependencies.
- `GET /readyz` (readiness) checks what a request needs (a `SELECT 1` on the
  database, the cache) with a short timeout, and answers `503` while any is
  down, so a load balancer stops sending traffic.
- Neither needs auth, and neither says anything secret.

## 4. Starting and stopping

- On start: config checked, pools created, migrations applied by a separate
  step (a release command, a job), not by every instance racing at boot.
- On `SIGTERM` and `SIGINT`: stop accepting new connections, let requests in
  flight finish (with a limit of about 10 to 25 seconds), stop the workers
  after their current job, close the pools, then exit. Containers and
  platforms send `SIGTERM` on every deploy.
- Timeouts on the server itself (request and header timeouts, body size
  limits) as well as on the database and outbound calls.

## 5. Metrics and error tracking

- When the project has them, use them: errors to its tracker (Sentry and the
  like) with the request id and without personal data; request rate, error
  rate and duration per route; queue depth and job failures.
- Do not add a metrics stack or a tracker on your own initiative; say in the
  report where it would help.

## Avoid

`process.env.X` read all over the code; a missing variable found by the
first user who needs it; `console.log` as the logger; logs with the raw URL,
the body or a token; a health check that queries everything and times out;
a process that drops requests in flight on every deploy; migrations run by
each instance at boot.
