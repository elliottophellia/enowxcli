---
name: backend
description: "Building or changing a backend: APIs, services, business rules, auth, data access, background work. How to read the project, where each piece goes, what every endpoint needs, and which backend-* skill covers each part. Read before building or reworking server code."
---

# Backend work

Generated backend code gives itself away: one long handler that reads the
body, queries the database and formats the reply in forty lines; a request
body passed straight to the ORM; `200 OK` with `{ "error": ... }` inside; a
stack trace in the response; every row loaded and filtered in JavaScript; a
key written into the source; three writes with no transaction; nothing for
the request that fails. This is how to build a backend someone can run,
change and trust.

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
  `backend-stack-rust`, `backend-stack-laravel`.

## 2. Choosing a stack, only when there is none

The brief or the project decides. With neither, take the plainest stack that
fits the job, and say so in the report:

| Situation | Default |
|---|---|
| The app is Next.js | Its route handlers and server actions (`backend-stack-next`) |
| A JavaScript or TypeScript team, an API on its own | Node with TypeScript and Hono or Fastify (`backend-stack-node`) |
| Python, data work, ML nearby | FastAPI (`backend-stack-python`) |
| A PHP team, or an admin-heavy app | Laravel (`backend-stack-laravel`) |
| Small, fast, few dependencies | Go with `net/http` (`backend-stack-go`) |
| The project is Rust | axum (`backend-stack-rust`) |

Data: PostgreSQL for anything with several users; SQLite for a single-user,
local or small app (with the settings in `backend-data`). A query layer that
types the rows (Drizzle, Prisma, SQLAlchemy 2, sqlc, sqlx, Eloquent) and its
migrations, never tables created by hand.

## 3. Where each piece goes

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

## 4. The contract first

When the interface is built at the same time, or by someone else, write the
contract before the handlers: the routes, the request and response shapes,
the error codes. Keep it where both sides read it: a shared types module, the
validation schemas, or an OpenAPI document generated from them. A brief that
gives the contract is followed exactly; a change to it goes in the report.
Details in `backend-api`.

## 5. What every endpoint has

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
   services) outside the request (`backend-jobs`, `backend-integrations`).
7. A log line per request with an id, and no secret or personal data in it
   (`backend-observability`).
8. Tests for the rule it enforces and the ways it fails (`backend-testing`).

Security basics for anything reachable from the internet are in
`backend-security`; text a user reads (errors, emails) goes through i18n
(the `i18n` skill).

## 6. Which skill for which part

| Part | Skill |
|---|---|
| Routes, methods, status codes, input, pagination, versioning | `backend-api` |
| Error format, mapping, what a client and a log each see | `backend-errors` |
| Sign-in, sessions, passwords, tokens, permissions | `backend-auth` |
| Queries, transactions, migrations, money and time, SQLite | `backend-data` |
| Queues, retries, scheduled work | `backend-jobs` |
| Calling other services, webhooks, payments, email | `backend-integrations` |
| Injection, uploads, SSRF, headers, rate limits, secrets | `backend-security` |
| Logs, health checks, shutdown, config at start | `backend-observability` |
| Tests for handlers and rules | `backend-testing` |

Read the ones for the parts you build, when you come to them.

## 7. Before you call it done

- It builds, the type check and the linter are clean for what you touched,
  the migrations apply to an empty database, and the tests pass.
- You called each new endpoint, on the happy path and on one failure (bad
  input, or no permission), and saw the status and body you meant. The test
  client needs no server. To call the running app, start it, call it and
  stop it in one command, its output going to a file: a server left writing
  to the command's output keeps the command from finishing, and one left
  running outlives your work. `set -m` puts it in its own process group, so
  one `kill` stops it and everything it started:
  ```sh
  set -m
  npm run dev > /tmp/api.log 2>&1 &
  pid=$!
  for i in $(seq 30); do curl -sf localhost:3000/api/health > /dev/null && break; sleep 1; done
  curl -s localhost:3000/api/products
  curl -s -X POST localhost:3000/api/orders -H 'content-type: application/json' -d '{}'
  kill -- -$pid
  ```
- The report lists each endpoint with its method, path, one example request
  and response, and anything the caller must know (a new variable, a
  migration to run).

## Avoid

A handler that is the whole feature; a request body written to the database
as it came; `200` for a failure; a stack trace, SQL or a file path in a
response; a key or password in the source or in a log; reading a table to
count or filter it in code; a query inside a loop; a balance or stock updated
with a read then a write; money as a float; times without a time zone; work
that can take seconds done inside the request; a second HTTP client, logger
or validation library beside the project's own; an endpoint nobody has
called.
