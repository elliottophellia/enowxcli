---
name: backend-observability
description: "Running a backend: configuration read and checked at start, structured logs with request and trace ids, log levels, OpenTelemetry traces and metrics per stack, RED metrics, health and readiness checks, graceful shutdown with timings, timeouts, feature flags, and error tracking with Sentry. Read when starting a service, adding config, logs, metrics or tracing, or making one easier to run and debug."
---

# Running it

The generated service reads `process.env` in twenty files, logs with
`console.log` and the raw URL, answers `/health` by querying every
dependency, drops requests in flight on every deploy, and gives no way to
tell one request's lines from another's. One part of the backend; the whole
is in the `backend` skill, and the pipeline behind it (collector, dashboards,
SLOs, alerts) is in `devops-observability`. This is what the service itself
does so it can be configured, found, debugged and replaced safely.

## 1. Configuration

- Read every setting from the environment once, at start, in one module,
  checked with a schema: types parsed, required values present, URLs valid.
  A missing or malformed value stops the process at start with a message
  naming it (the name, never the value), not a crash on the first request
  that needs it.
- The rest of the code imports that module; nothing else reads
  `process.env` or `os.environ` (and in Laravel, `env()` is called only in
  `config/`).
- `.env.example` lists every variable with a comment and a safe example;
  defaults only for values that are safe everywhere (a port, a log level),
  never for secrets.
- Secrets typed so they cannot be printed: pydantic `SecretStr`, Rust
  `secrecy::SecretString`, a Go type with a `LogValue` method returning
  `[redacted]`. Config is read-only after start; changing it is a restart,
  and a switch that must change at runtime is a feature flag (section 9).

| Stack | Checked at start with |
|---|---|
| Node, Next.js | zod (`z.object(...).safeParse(process.env)`), or `@t3-oss/env-core` / `env-nextjs` |
| Python | pydantic-settings (`BaseSettings`) |
| Go | `caarlos0/env` or `sethvargo/go-envconfig` (`kelseyhightower/envconfig` works but is unmaintained since 2019); koanf when files and env are layered |
| Rust | figment or the `config` crate into a serde struct |
| Laravel | `config/*.php`, `php artisan config:cache` in production |
| Java, .NET | `@ConfigurationProperties` with `@Validated` (`backend-stack-java`); options with `ValidateOnStart()` (`backend-stack-dotnet`) |

```ts
const parsed = Env.safeParse(process.env); // Env: a z.object of every variable
if (!parsed.success) {
  const names = parsed.error.issues.map((i) => i.path.join("."));
  console.error(`Invalid or missing environment variables: ${names.join(", ")}`);
  process.exit(1);
}
export const config = parsed.data;
```

## 2. Logs

- Structured (JSON in production, readable in development) through the
  project's logger, never bare `console.log` or `print`, to stdout for the
  platform to collect (no log files or rotation code in a container).

| Stack | Logger |
|---|---|
| Node | pino (Fastify's built-in one; `pino-pretty` only in development) |
| Python | structlog, or `logging` with a JSON formatter |
| Go | `log/slog` with `slog.NewJSONHandler` |
| Rust | `tracing` with `tracing-subscriber`'s JSON output |
| Laravel | Monolog: `LOG_CHANNEL=stderr`, `LOG_STDERR_FORMATTER=Monolog\Formatter\JsonFormatter` |
| Java, .NET | Logback JSON via Boot's structured logging; Serilog or the JSON console logger |

- One line per request at the end: method, route pattern (not the raw URL
  with ids and tokens), status, duration, request id, user id. Plus a line
  for each decision worth finding later (a payment captured, a job failed),
  not for every step.
- Levels mean something: `error` needs someone to look, `warn` is unusual
  but handled, `info` is the story of normal work, `debug` is off in
  production and switchable without a redeploy (`LOG_LEVEL`). A client's
  `4xx` is `info` or `warn`; an error is logged once, where it is handled
  (`backend-errors`).
- Never passwords, tokens, keys, card numbers, cookies, `Authorization`
  headers or whole personal records (`backend-security`). Redact in the
  logger (pino `redact`, a structlog processor, slog `ReplaceAttr`), and keep
  exception locals out: structlog's default `dict_tracebacks` prints them;
  use `ExceptionRenderer(ExceptionDictTransformer(show_locals=False))`.
- An audit trail (who changed which record, when, from what to what) is
  data: a table written in the same transaction, not log lines that expire.

## 3. Request ids and trace context

- A request id on every line of a request: taken from `X-Request-Id` when a
  trusted proxy sets it, generated otherwise, returned in the response, and
  passed on to jobs and outbound calls.
- With tracing, W3C trace context is the correlation:
  `traceparent: 00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01`
  (version, 32-hex trace id, 16-hex parent span id, flags). Every log line
  carries `trace_id` and `span_id` through the logger's OpenTelemetry
  integration; `X-Request-Id` stays for people quoting it. Continue trace
  context from your own services; at the public edge, start a fresh trace.
- Carry the ids in the request's context, not in arguments: Node
  `AsyncLocalStorage` with a pino `mixin`, `structlog.contextvars`, Go
  `context.Context`, Rust `tracing` spans, Laravel `Context::add()` (which
  lands in every log record and travels into queued jobs by itself).
- Across a queue, the producer puts the request id and trace context in the
  job's metadata and the worker restores them before it logs.

## 4. Traces with OpenTelemetry

Add a telemetry vendor only when the project has one or the task asks; say
in the report where it would help. When it is there, configure it by the
standard `OTEL_*` variables (`devops-observability`), not values in code,
and start the SDK before any instrumented module loads, or nothing is patched.

| Stack | Setup |
|---|---|
| Node | `@opentelemetry/sdk-node` with `@opentelemetry/auto-instrumentations-node`, loaded by `node --import ./dist/instrumentation.js dist/index.js`; route names from `@hono/otel` or `@fastify/otel` |
| Next.js | `instrumentation.ts` exporting `register()` that calls `registerOTel("app")` from `@vercel/otel` |
| Python | `opentelemetry-distro`, `opentelemetry-bootstrap -a install`, run under `opentelemetry-instrument uvicorn app.main:app`; under gunicorn, start the SDK in the `post_fork` hook |
| Go | `otelhttp.NewHandler(mux, "api")`, `otelpgx.NewTracer()` on the pool config, a `TracerProvider` with a batcher, `propagation.TraceContext{}` |
| Rust | `tracing` spans exported through `tracing-opentelemetry` and `opentelemetry-otlp`, versions that match each other |
| Laravel | OpenTelemetry's PHP SDK with its Laravel auto-instrumentation, or the vendor's agent |

```ts
// src/instrumentation.ts
import { register } from "node:module";
import { NodeSDK } from "@opentelemetry/sdk-node";
import { getNodeAutoInstrumentations } from "@opentelemetry/auto-instrumentations-node";

// ES modules need the loader hook, or pg, ioredis and the rest are not patched.
register("@opentelemetry/instrumentation/hook.mjs", import.meta.url);

export const sdk = new NodeSDK({ // exporters come from the OTEL_* variables
  instrumentations: [
    getNodeAutoInstrumentations({ "@opentelemetry/instrumentation-fs": { enabled: false } }),
  ],
});
sdk.start();
```

- Auto-instrumentation covers inbound and outbound HTTP, database drivers,
  Redis and most queues. Add a manual span only around work worth timing on
  its own (pricing a basket, rendering a PDF); attributes are ids and
  counts, never personal data or secrets; span names stay low-cardinality
  (`POST /orders/{id}/refunds`).
- Exporters batch: call `shutdown()` on the way out (section 7) or the last
  seconds of spans are lost.

## 5. Metrics

- RED per route: rate, errors, duration as a histogram
  (`http.server.request.duration` in seconds, with `http.request.method`,
  `http.route` and `http.response.status_code`). HTTP auto-instrumentation
  or the framework's metrics usually give these for free.
- USE for what the service holds: pool connections in use, idle and
  waiting, and acquire time; queue depth and the age of the oldest job;
  Node's event loop delay (`perf_hooks.monitorEventLoopDelay`), Go's
  goroutines, GC pauses; duration and errors per outbound dependency.
- A few business counters that show breakage (orders placed, payments
  failed, sign-ups). Labels stay low-cardinality: the route template, the
  method, the status; never user ids, raw paths, emails or error messages.
- Pushed over OTLP or scraped from `/metrics` on an internal port, as the
  platform does it; never on the public listener. Dashboards, SLOs and
  alerts: `devops-observability`. Profiling: `performance-profiling`.

## 6. Health

| Endpoint | Answers | Checks | When it fails |
|---|---|---|---|
| `GET /healthz` (liveness) | `200` while the process can serve at all | nothing outside the process | the platform restarts the instance |
| `GET /readyz` (readiness) | `200`, or `503` while any need is down | what a request needs: `SELECT 1` on the database, the cache, with a short timeout | traffic stops going to this instance |
| startup (optional) | `200` once warmed up | caches loaded, migrations present | liveness waits instead of killing a slow start |

- Neither needs auth, and neither says anything secret: at most
  `{"status":"ok","checks":{"db":"ok"}}` and the release. Checks finish inside
  the probe's timeout (Kubernetes defaults: 1 second, every 10 seconds, 3
  failures), a few hundred milliseconds per dependency.
- Liveness never touches the database: a database blip would restart every
  instance at once. Readiness never checks a third-party API: its outage
  would take every instance out of rotation; that failure is a `503` per
  request (`backend-integrations`).
- Use the framework's own where it has one: Laravel's `/up`, Spring
  Actuator's probes, ASP.NET Core `MapHealthChecks`.

## 7. Starting and stopping

- On start: config checked, logger, telemetry, pools created and checked,
  routes, then listen. Migrations are applied by a separate step (a release
  command, a job), not by every instance racing at boot.
- The process is PID 1 or receives signals from it: `CMD ["node",
  "dist/index.js"]` in exec form, not `npm start` or a shell-form command,
  which may not pass `SIGTERM` on; or `tini` (`docker run --init`).

On `SIGTERM` (sent on every deploy) and `SIGINT`:

1. Readiness turns `503`, and the process keeps serving for 5 to 10 seconds
   while the load balancer notices (or a Kubernetes `preStop` sleep).
2. Stop accepting new connections and close idle keep-alive ones; let
   requests in flight finish, with a limit of about 10 to 25 seconds.
3. Stop the workers after their current job; a longer job must be safe to
   run again (`backend-jobs`).
4. Flush telemetry and error tracking (`sdk.shutdown()`,
   `Sentry.close(2000)`).
5. Close the pools, then exit `0`; exit non-zero if the limit was hit.

The whole sequence fits inside the platform's grace period: 30 seconds by
default in Kubernetes (`terminationGracePeriodSeconds`), 10 in `docker
stop`, 10 on Cloud Run.

- Timeouts on the server itself (request and header timeouts, body size
  limits) as well as on the database and outbound calls. The idle keep-alive
  timeout is longer than the load balancer's (65 seconds behind an AWS ALB's
  60), or it reuses a closed connection and reports a `502`.

## 8. Error tracking

- When the project has a tracker, use it: `5xx` and crashes (never a
  client's `4xx`) with the request id and without personal data.
- Sentry (or GlitchTip, API-compatible): initialised before the app code
  (Node: in the file loaded by `--import`), with `release` set to the
  deployed git SHA, `environment`, `sendDefaultPii: false`
  (`send_default_pii=False`), a `beforeSend` that scrubs, users by id only,
  source maps uploaded at build, and `tracesSampleRate` 0.05 to 0.2 when
  OpenTelemetry already traces.
- Per stack: `@sentry/node` (`setupExpressErrorHandler`,
  `setupFastifyErrorHandler`), `@sentry/nextjs`, `sentry-sdk` (FastAPI and
  Django integrations turn on by themselves), `sentry-go`, the `sentry` crate
  (keep the guard from `sentry::init` alive for all of `main`),
  `sentry/sentry-laravel`.
- Sentry's SDK and your own OpenTelemetry SDK must not both register a
  global tracer provider: follow the installed SDK's guide for the pair.
  Flush on shutdown, or the crash that ended the process is never reported.

## 9. Feature flags

- Code talks to OpenFeature, a vendor-neutral API; the provider (flagd,
  LaunchDarkly, Unleash, Flagsmith, GrowthBook, ConfigCat, PostHog) is
  chosen in one place.

```ts
import { OpenFeature } from "@openfeature/server-sdk";

await OpenFeature.setProviderAndWait(provider); // at start, in one module
const flags = OpenFeature.getClient();

// The default (false) is what runs when the flag service is unreachable.
export const useNewCheckout = (userId: string) =>
  flags.getBooleanValue("new-checkout", false, { targetingKey: userId });
```

- Evaluated on the server with a context (user id, tenant), the safe old
  behaviour as the default, the evaluated variant recorded on the request's
  span or log line.
- Each flag has an owner and a removal date and goes within weeks of full
  rollout; a kill switch in front of each risky dependency; never a flag in
  place of an authorisation check. Environment variables only for coarse
  switches changed with a restart.

## Check it

- Start with a required variable missing: the process exits non-zero at
  once and names it.
- `curl -si localhost:8080/readyz` answers `200`; stop the database and it
  answers `503` within the check timeout while `/healthz` stays `200`.
- One request writes exactly one request line with the route template,
  status, duration and request id, and the same id is in the response
  header. Search the logs for a test password, token and email: nothing.
- `kill -TERM` the process during a slow request: it completes, the log
  shows the shutdown steps, the exit code is `0`, inside the grace period.
- With tracing on, one request is one trace with its database spans inside,
  and its log lines carry the trace id.

## Avoid

`process.env.X` read all over the code; a missing variable found by the
first user who needs it; `console.log` as the logger; logs with the raw URL,
the body, a token or exception locals; a health check that queries everything
and times out; liveness that checks the database; a process that drops
requests in flight on every deploy; `npm start` as PID 1; migrations run by
each instance at boot; user ids as metric labels; the tracing SDK started
after the app's imports; 100% trace sampling in production; a flag nobody
removes.
