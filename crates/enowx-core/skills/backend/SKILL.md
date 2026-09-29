---
name: backend
description: "Building or changing a backend: APIs, services, business rules, auth, data access, background work. How to read the project, a request's path through the layers and the middleware order, where each piece goes per stack, defaults for a new service per language, one deployable or several, REST, GraphQL or RPC, what every endpoint needs, when it is done, and which backend-* skill covers each part. Read before building or reworking server code."
---

# Backend work

Generated backend code gives itself away: one long handler that reads the
body, queries the database and formats the reply in forty lines; a request
body passed straight to the ORM; `200 OK` with `{ "error": ... }` inside; a
stack trace in the response; every row loaded and filtered in JavaScript; a
key written into the source; three writes with no transaction; nothing for
the request that fails; five services and a message bus for a product with
ten users. This is how to build a backend someone can run, change and
trust, and which skill holds each part in depth.

## 1. Read before you build

- Find the entry point, the router, and one endpoint that already works like
  the one you are adding. Trace a request through it end to end: route,
  validation, handler, service, data access, response, errors.
- Note the conventions and keep them: the validation library, the error
  format, the logger, how config is read, the ORM or query layer, the folder
  layout, the test setup. A second way of doing any of these is a defect.
- Run the tests (and the type check) before changing anything, so you know
  what was already failing.
- Read the `backend-stack-*` skill for the stack: `backend-stack-node`,
  `backend-stack-next`, `backend-stack-python`, `backend-stack-go`,
  `backend-stack-rust`, `backend-stack-laravel`, `backend-stack-java`,
  `backend-stack-dotnet`, `backend-stack-rails`.

## 2. Choosing a stack, only when there is none

The brief or the project decides. With neither, take the plainest stack that
fits the job, and say so in the report:

| Situation | Default |
|---|---|
| The app is Next.js | Its route handlers and server actions (`backend-stack-next`) |
| A JavaScript or TypeScript team, an API on its own | Node with TypeScript and Hono or Fastify (`backend-stack-node`) |
| Python, data work, ML nearby | FastAPI (`backend-stack-python`) |
| A PHP team, or an admin-heavy app | Laravel (`backend-stack-laravel`) |
| A Ruby team, or a product that wants conventions over choices | Rails (`backend-stack-rails`) |
| A Java or Kotlin team, or an enterprise estate | Spring Boot (`backend-stack-java`) |
| A C# team, or a Microsoft estate | ASP.NET Core (`backend-stack-dotnet`) |
| Small, fast, few dependencies | Go with `net/http` (`backend-stack-go`) |
| The project is Rust | axum (`backend-stack-rust`) |

What a new service starts with (the stack skill has the setup):

| Language | HTTP | Data | Validation | Logs | Tests |
|---|---|---|---|---|---|
| TypeScript, Node 24 LTS | Hono or Fastify | Drizzle or Prisma | zod | pino | Vitest |
| Python 3.12 or later, uv | FastAPI | SQLAlchemy 2, Alembic | pydantic v2 | structlog | pytest, httpx |
| Go 1.22 or later | `net/http` or chi | pgx and sqlc | go-playground/validator | `log/slog` | `testing`, `httptest` |
| Rust | axum on tokio | sqlx | serde with validator or garde | tracing | the router called with `oneshot` |
| Java 21 or later | Spring Boot | Spring Data JPA, Flyway | Jakarta Validation | Logback JSON | JUnit 5, Testcontainers |
| C#, .NET 10 | ASP.NET Core minimal APIs | EF Core | FluentValidation or DataAnnotations | Serilog | xUnit, `WebApplicationFactory` |
| PHP | Laravel | Eloquent | Form Requests | the `Log` facade (Monolog) | Pest |
| Ruby | Rails 8 | Active Record | model validations, strong parameters | the Rails logger | Minitest or RSpec |

Data: PostgreSQL for anything with several users; SQLite for a single-user,
local or small app (with the settings in `backend-data`). A query layer that
types the rows (Drizzle, Prisma, SQLAlchemy 2, sqlc, sqlx, Eloquent) and its
migrations, never tables created by hand. Redis or Valkey only when a queue,
a cache or shared rate limits need it, not by habit.

## 3. A request, end to end

Every request passes the same stations. Middleware runs top to bottom on the
way in and bottom to top on the way out:

```text
edge          proxy or load balancer: TLS, a body size cap, timeouts
request id    from X-Request-Id or traceparent, or made here; on every log line
access log    wraps the rest: one line with the final status and duration
error handler wraps the rest: every failure below leaves in one format
headers, CORS security headers on every response; preflights answered here
limits        body size, request deadline, rate limit per IP
authenticate  session cookie or token: who is calling, or anonymous
csrf          cookie auth with an unsafe method: token or Origin check
route         parse and validate input, call one service function
service       may this caller do this to this record; the rules; the transaction
data access   queries scoped to the caller or tenant
response      built from an output schema: status, headers, body
```

- The request id and the error handler come first, so a failure anywhere is
  logged with the id and answered in the format. CORS comes before
  authentication, since a preflight carries no credentials. A rate limit per
  account sits after authentication; one per IP does not need it.
- Middleware answers who is calling; the service answers whether they may
  touch this record, because only the service has loaded it (`backend-auth`).
- Validation belongs to the route, rules to the service, constraints to the
  database: all three, since each catches what the others cannot.
- Nothing below the handler knows about HTTP: the service takes typed input
  and the caller, returns a result or raises a domain error, and the error
  handler maps it (`backend-errors`). The same service then runs from a job,
  a CLI or a test.

## 4. Where each piece goes

- **Modular by feature**: a folder per area (`orders/`, `products/`,
  `auth/`) holding its routes, service, data access, schemas and tests.
  Shared pieces (the database client, the error type, the logger, config) in
  one place such as `lib/` or `core/`.
- **Layers stay apart**:
  - the **route or handler** speaks HTTP: it parses and validates the input,
    calls one service function, and turns the result or the error into a
    response. No SQL, no business rules;
  - the **service** holds the business rules and the transaction: "an order
    can be paid once", "stock never goes below zero";
  - **data access** (a repository, or the query layer used directly in the
    service when the project does that) is the only code that knows tables.
- **Config** comes from the environment, read and validated once at start,
  in one module; the process refuses to start when a required value is
  missing. Every variable is listed in `.env.example`, which is committed;
  `.env` is not.
- File length, names and comments follow the `code` skill.

The same shape in each stack's own layout; the framework's wins where it has
one:

| Stack | Layout |
|---|---|
| Node | `src/orders/{routes,service,repo,schemas}.ts`, `src/lib/{db,errors,logger,config}.ts`, `src/server.ts` |
| Next.js | `app/api/**/route.ts` and server actions stay thin; services and data access in `src/server/` behind `import "server-only"` |
| FastAPI | `app/orders/{router,service,repository,schemas,models}.py`, `app/core/{config,db,errors}.py`, `app/main.py` |
| Go | `cmd/api/main.go` wires everything; `internal/orders/{handler,service,store}.go`; sqlc output in `internal/db` |
| Rust | `src/orders/{mod,routes,service,repo}.rs`, `src/{error,config,main}.rs` |
| Laravel | `routes/api.php`, thin controllers, Form Requests, policies, Actions in `app/Actions` for work beyond one model |
| Rails | the framework's folders; a plain object in `app/services` only for a workflow no single model owns |
| Spring Boot | a package per feature (`orders`) with controller, service, repository and DTOs, not packages per layer |
| ASP.NET Core | `Features/Orders/` with endpoints, handlers and DTOs; the pipeline in `Program.cs` |

## 5. The contract, and the style of API

When the interface is built at the same time, or by someone else, write the
contract before the handlers: the routes, the request and response shapes,
the error codes. Keep it where both sides read it: a shared types module, the
validation schemas, or an OpenAPI document generated from them. A brief that
gives the contract is followed exactly; a change to it goes in the report.
Details in `backend-api`.

| Style | Choose it when |
|---|---|
| REST over JSON (the default) | Public APIs, mobile and web clients, anything cached, logged or called with curl |
| Server actions or tRPC | The only client is the TypeScript frontend in the same repository |
| GraphQL | Many clients need different shapes of one graph of related data (`backend-graphql`) |
| gRPC or Connect | Service to service inside your own system: strict contracts across languages, streaming |
| Webhooks | Telling other systems that something happened (`backend-integrations`) |
| SSE or WebSockets | Pushing live updates to a client (`backend-realtime`) |

One style per audience: two ways into the same data for one client means
two sets of authorisation checks to keep in step.

## 6. What every endpoint has

1. Input validated at the boundary against a schema; unknown fields refused
   or dropped; limits on sizes and lengths (`backend-api`).
2. Who is calling, and whether they may touch this record: the check is in
   the service, on every request (`backend-auth`).
3. Errors mapped to the right status and one error format, nothing internal
   leaked (`backend-errors`).
4. Writes that touch several rows in one transaction; races closed with a
   constraint or a conditional update, not a check then a write
   (`backend-data`).
5. Lists paginated in the query; related rows loaded in one query, never in
   a loop (`backend-data`).
6. Slow or failure-prone work (email, webhooks, reports, calls to other
   services) outside the request (`backend-jobs`, `backend-integrations`);
   live updates pushed to the client as `backend-realtime` says.
7. A log line per request with an id, and no secret or personal data in it
   (`backend-observability`).
8. Tests for the rule it enforces and the ways it fails (`backend-testing`).

Security basics for anything reachable from the internet are in
`backend-security`; text a user reads (errors, emails) goes through i18n
(the `i18n` skill).

## 7. One deployable first

- Start with a modular monolith: one codebase, one deploy (a worker process
  for the queue is still the same deployable), modules by feature, each with
  a small public interface and its own tables. A module calls another
  through that interface, never by reading its tables.
- Enforce the boundaries with a tool, or they erode: dependency-cruiser or
  eslint-plugin-boundaries (TypeScript), import-linter (Python), `internal/`
  packages (Go), Spring Modulith or ArchUnit (Java), Packwerk (Rails),
  Deptrac (PHP), crates in a workspace (Rust).
- Split a module out only for a reason you can name: it scales very
  differently (GPU or CPU-heavy work), it must be isolated (payments in a
  small compliance scope), another team releases it on its own schedule, or
  it needs another runtime. "It might grow" is not a reason.
- A split costs calls that fail and time out (`backend-integrations`), no
  transaction across services (the outbox in `backend-jobs`), versioned
  contracts, tracing across hops (`backend-observability`) and a deploy per
  service (`devops-deploy`). Pay it on purpose.

## 8. Which skill for which part

| Part | Skill |
|---|---|
| Routes, status codes, input, pagination, idempotency, versioning, OpenAPI | `backend-api` |
| Error format, mapping, what a client and a log each see | `backend-errors` |
| Sign-in, sessions, passwords, passkeys, tokens, permissions, tenants | `backend-auth` |
| Queries, transactions, migrations, money and time, pools, replicas | `backend-data` |
| Queues, retries, scheduled work, workflows | `backend-jobs` |
| Other services, webhooks, payments, email, AI APIs | `backend-integrations` |
| Injection, SSRF, headers, CORS, CSRF, rate limits, secrets | `backend-security` |
| Logs, traces, metrics, health checks, shutdown, config at start | `backend-observability` |
| Tests for handlers, rules, jobs and permissions | `backend-testing` |
| HTTP and application caches, invalidation | `backend-caching` |
| Live updates, SSE, WebSockets, streamed AI answers | `backend-realtime` |
| Uploads, object storage, signed URLs | `backend-files` |
| Full-text, fuzzy and semantic search | `backend-search` |
| A GraphQL server | `backend-graphql` |
| Tables, live migrations, query plans, indexes, isolation levels | `database-schema`, `database-migrations`, `database-queries`, `database-indexes`, `database-transactions` |
| One engine's specifics | `database-postgres`, `database-mysql`, `database-sqlite`, `database-mongodb` |
| Images, pipelines, deploys, hosting | `devops-containers`, `devops-ci`, `devops-deploy`, `devops-platforms` |
| A slow endpoint | `performance-backend` |
| Reviewing for security | `security-web`, `security-auth` |
| API reference docs | `docs-api` |

Read the ones for the parts you build, when you come to them.

## 9. When it is done

- It builds; the type check and the linter are clean for what you touched.
- The migrations apply to an empty database; a step that drops or rewrites
  data is named in the report.
- The tests pass, with new ones for the rule, the ways it fails, and the
  caller who may not (`backend-testing`).
- Every new endpoint validates its input, checks the caller against the
  record, answers failures in the project's format, and is in the contract
  (OpenAPI or the shared types).
- Every outbound call has a timeout; slow work is in a job; nothing sends
  mail or charges a card inside a transaction.
- New config is read at start and listed in `.env.example`; no secret in
  the code, a log or the repository.
- The report lists each endpoint with its method, path, one example request
  and response, and anything the caller must know (a new variable, a
  migration to run, a placeholder or a stub).

## Check it

Call each new endpoint, on the happy path and on one failure (bad input, or
no permission), and see the status and body you meant. The test client needs
no server. To call the running app, start it, call it and stop it in one
command, its output going to a file: a server left writing to the command's
output keeps the command from finishing, and one left running outlives your
work. `set -m` puts it in its own process group, so one `kill` stops it and
everything it started:

```sh
set -m
npm run dev > /tmp/api.log 2>&1 &
pid=$!
for i in $(seq 30); do curl -sf localhost:3000/api/health > /dev/null && break; sleep 1; done
curl -s localhost:3000/api/products
curl -s -X POST localhost:3000/api/orders -H 'content-type: application/json' -d '{}'
kill -- -$pid
```

- `curl -si` on the failure shows the status, the problem body and the
  request id; that id in `/tmp/api.log` leads to the error, logged once.
- Another user's record answers `404`; your log lines hold no token or body.

## Avoid

A handler that is the whole feature; a request body written to the database
as it came; `200` for a failure; a stack trace, SQL or a file path in a
response; a key or password in the source or in a log; reading a table to
count or filter it in code; a query inside a loop; a balance or stock updated
with a read then a write; money as a float; times without a time zone; work
that can take seconds done inside the request; a second HTTP client, logger
or validation library beside the project's own; permission checks in the
frontend only; services split before a module boundary has held; an
endpoint nobody has called.
