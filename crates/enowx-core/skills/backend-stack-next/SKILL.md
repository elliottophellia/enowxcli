---
name: backend-stack-next
description: "The server side of a Next.js app (App Router): route handlers versus server actions, a data access layer, validation, auth checks in every action, the database client, runtime, caching and revalidation, environment variables. Read when building the backend of a Next.js project."
---

# Next.js on the server

The backend rules are in the `backend` skill; pages and components are the
frontend's. These are Next's own server rules.

## 1. Route handlers or server actions

- **Server actions** (`"use server"`) for mutations the app's own forms and
  buttons make: create, update, delete. They are public HTTP endpoints all the
  same: validate the input and check the user in every one.
- **Route handlers** (`app/api/.../route.ts`) for everything called from
  outside the app's own components: webhooks, mobile or third-party clients,
  file downloads, anything needing a specific status, header or method.
- **Reads** for a page happen in server components, through the data access
  layer, not by the page fetching its own `/api` route.

## 2. A data access layer

- All database and business code lives in a server-only layer
  (`src/server/` or `lib/` with a folder per feature), and each module starts
  with `import "server-only"` (the `server-only` package) so it can never be
  bundled for the browser.
- Server components, actions and route handlers call its functions
  (`getOrders(user)`, `payOrder(user, id)`); they do not hold queries
  themselves.
- The functions take the current user and check permissions themselves
  (`backend-auth`), so every caller is covered. Middleware (`middleware.ts`)
  can redirect signed-out users, but it is not authorisation.
- Return plain data objects (DTOs) with only the fields the caller may see;
  never pass a whole database row to a client component, where it is
  serialised into the page.

## 3. Validation and results

- Parse input with zod (or the project's library) at the top of every action
  and handler: `const parsed = schema.safeParse(Object.fromEntries(formData))`.
- An action returns a typed result the form can show
  (`{ ok: false, errors: { quantity: "too_many" } }` or `{ ok: true, id }`)
  instead of throwing for expected failures; unexpected ones throw and reach
  `error.tsx`.
- A route handler answers with `NextResponse.json(body, { status })` and the
  error format in `backend-errors`.
- After a mutation, `revalidatePath` or `revalidateTag` for what changed, then
  `redirect` if the flow moves on.

## 4. The database client

- One client module (`src/server/db.ts`). With hot reload in development,
  keep it on `globalThis` so every reload does not open a new pool:
  ```ts
  const globalForDb = globalThis as unknown as { db?: Db };
  export const db = globalForDb.db ?? createDb();
  if (process.env.NODE_ENV !== "production") globalForDb.db = db;
  ```
- Drizzle or Prisma, with their migrations (`backend-data`). SQLite through
  `better-sqlite3` or libSQL needs the Node.js runtime and a writable disk:
  fine on a server or container, not on serverless platforms, which need a
  hosted database (Turso, Neon, Supabase).
- Routes that use the database or Node APIs run on the Node.js runtime (the
  default); `export const runtime = "edge"` only for code that needs neither.

## 5. Caching

- Know what your Next.js version caches by default before relying on it,
  and say what is dynamic: pages that read the user or change often use
  `cookies()`, `headers()` or `export const dynamic = "force-dynamic"`, or tag
  their data and revalidate it after writes.
- Route handlers that return user data are never cached.

## 6. Environment variables

- Server secrets without a prefix, read only in server code through the
  config module (`backend-observability`). `NEXT_PUBLIC_` variables are
  compiled into the browser bundle: never a key.

## 7. Check

`npm run build` type-checks and lints; `npm run lint` alone for lint. Call
route handlers with `curl` against `npm run dev`, and test actions and the
data layer with Vitest against a test database (`backend-testing`).

## Avoid

`"use server"` functions with no validation or user check; queries written in
page components; a whole row passed to a client component; the auth check
only in middleware; a Prisma or Drizzle client created per request or per
reload; SQLite on a serverless deploy; a secret in a `NEXT_PUBLIC_` variable.
