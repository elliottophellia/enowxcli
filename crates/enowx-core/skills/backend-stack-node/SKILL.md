---
name: backend-stack-node
description: "A Node.js API in TypeScript: Hono, Fastify or Express, validation with zod, an error handler, config at start, Drizzle or Prisma, pino logging, graceful shutdown, tests with Vitest. Read when building or changing a Node.js backend."
---

# Node.js with TypeScript

The backend rules are in the `backend` skill; these are Node's own.

## 1. Setup

- TypeScript with `strict` on, ES modules, the Node version in
  `package.json` `engines` and `.nvmrc`. The project's package manager
  (pnpm, npm, yarn, bun), one lock file.
- Framework: the project's; for a new API, Hono (small, typed, runs
  anywhere) or Fastify (schemas, plugins, speed). Express only where the
  project already uses it, with its async errors handled (Express 5, or a
  wrapper on 4).
- Development with `tsx watch src/index.ts`; production runs the compiled
  output (`tsc` or `tsup` to `dist/`), not `ts-node`.

## 2. Layout

```
src/
  index.ts          start: config, app, server, shutdown
  app.ts            the app and its routes, exported for tests
  config.ts         env parsed with zod
  lib/              db, logger, errors
  orders/
    routes.ts       HTTP: parse, call the service, respond
    service.ts      rules and transactions
    repository.ts   queries
    schemas.ts      zod schemas and the types inferred from them
    orders.test.ts
```

## 3. Validation and types

- zod schemas at the edge (`@hono/zod-validator`, Fastify's type provider
  for zod, or `schema.parse` in the handler), with `z.infer` for the types;
  the same schemas shared with a TypeScript client when there is one.
- `z.coerce` for query strings and path parameters, `.strict()` or copying
  fields to refuse extra keys (`backend-api`).

## 4. Errors

- An `AppError` class with a `code` and a status, thrown from services; one
  error handler (`app.onError` in Hono, `setErrorHandler` in Fastify, the
  final middleware in Express) that maps it, zod errors and the unexpected
  into the format in `backend-errors`.
- No `try/catch` around each handler.

## 5. Data and logging

- Drizzle (with `drizzle-kit` migrations) or Prisma (with `prisma migrate`),
  one client from `lib/db.ts`; `backend-data` for the rest.
- pino as the logger (Fastify's built-in one, or `hono-pino`), with a
  request id and a child logger per request; `pino-pretty` only in
  development.

## 6. Shutdown

```ts
const server = serve({ fetch: app.fetch, port: config.PORT });
for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => {
    server.close(async () => {
      await db.close();
      process.exit(0);
    });
    setTimeout(() => process.exit(1), 20_000).unref();
  });
}
```

And `process.on("unhandledRejection")` logs and exits, so a bug is not left
running in a broken state.

## 7. Tests

Vitest; the app exported from `app.ts` and called without a port
(`app.request("/orders")` in Hono, `app.inject()` in Fastify, supertest for
Express); a real database per `backend-testing`; MSW for other services.

## 8. Check

`tsc --noEmit`, the linter (ESLint or Biome), the tests, then start it and
call the endpoints with `curl`.

## Avoid

`any` for request bodies; a `try/catch` in every handler; `process.env` read
outside `config.ts`; `console.log` as the logger; `ts-node` in production;
Express 4 async handlers without error handling; a floating promise for work
that must finish.
