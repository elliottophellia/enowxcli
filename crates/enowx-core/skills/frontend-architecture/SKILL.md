---
name: frontend-architecture
description: "Structuring a frontend codebase: folders by feature, layers of components, routing and layouts, where data fetching lives, code splitting, environment configuration, a typed API client, imports and dependencies. Read before starting a frontend project or adding a feature area."
---

# Structuring a frontend codebase

The generated codebase sorts files by kind: forty components in one
`components/` folder, every hook in `hooks/`, `fetch` written inline in
whichever component needed data first, an `index.ts` in every folder
re-exporting the rest, `../../../../lib` imports, and a secret in a
`VITE_` variable because that is how the value reached the browser. It
works for the demo and fights every change after. This is a structure in
which each feature lives in one place, data comes through one typed
client, and configuration is checked before it ships. The whole frontend
is in `frontend`; code style in `code`.

## 1. Folders by feature

Group by what the product does, not by the kind of file. A feature folder
holds everything that changes together:

```text
src/
  app/                  # the shell: providers, router, root layout, root error boundary
  routes/               # thin route files importing from features (or Next's app/, SvelteKit's src/routes/)
  features/
    orders/
      api/              # orders.queries.ts, orders.mutations.ts, orders.schemas.ts
      components/       # OrderTable.tsx, OrderStatusBadge.tsx, RefundDialog.tsx
      hooks/            # useOrderFilters.ts
      routes/           # OrdersPage.tsx, OrderDetailPage.tsx
  components/
    ui/                 # library primitives: Button, Dialog, Input (where shadcn/ui puts them)
    layout/             # AppShell, PageHeader
  lib/                  # api client, env, query client, formatters, i18n setup
  styles/               # tokens.css, globals.css
  test/                 # renderWithProviders, MSW handlers, fixtures
```

- A feature imports from `components`, `lib` and itself, never from
  another feature's files; what two features share moves to `components`
  or `lib`. Enforce it with `import/no-restricted-paths` zones or
  `eslint-plugin-boundaries`. Tests sit beside the file they test, or
  where the project already keeps them.
- Next.js: routes stay in `app/` (route groups such as `(marketing)` and
  `(app)` for different frames, private `_components` folders for parts of
  one route); features sit beside it in `src/features/`.
- Folders by kind suit a tiny app (one or two screens, under about 15
  components); move to features when a second area appears.
- Files past about 300 lines split along a concept: a sub-component, a
  hook, the schema, the column definitions (`code`).

## 2. Component layers

| Layer | Lives in | Knows | Never |
|---|---|---|---|
| Primitives | `components/ui` | props, variants, native attributes, `ref` | data, domain words, fetching |
| Product components | `features/*/components` | one concept of the domain (`OrderStatusBadge`, `InvoiceTotals`), data passed in as props | fetching, routing |
| Route components | `features/*/routes`, or the router's files | params, which queries run, the four states, the arrangement of the screen | details a primitive owns |

- Props down, events up: a product component receives the order and calls
  `onRefund(order.id)`; the route decides what refunding does. Plain data
  in means it renders in a test or a story without a server.
- Composition over configuration: `children` and slots rather than a
  boolean per variation (`<Card><CardHeader>`, not `showHeader`).
- Primitives pass native props and the `ref` through, so a `Button` stays
  a button to forms and assistive technology (`ui-stack-react`).

## 3. Routing and layouts

- File-based where the framework has it (Next, SvelteKit, Nuxt, TanStack
  Router with its plugin, Expo Router), a route config elsewhere (React
  Router's `routes.ts`, Vue Router, Angular): keep the project's. The shell
  is a layout route rendering its children, so it never remounts.
- A not-found route (Next `not-found.tsx` with `notFound()`, a `*` route in
  React Router, SvelteKit `+error.svelte` with `error(404)`, Nuxt
  `error.vue` with `createError({ statusCode: 404 })`) and an error
  boundary per section (`frontend-errors`).
- Each route is its own chunk: automatic in Next, SvelteKit, Nuxt and
  TanStack Router (`autoCodeSplitting`); `lazy` in React Router;
  `React.lazy` with `Suspense` elsewhere.
- Guarded routes redirect to sign-in with where the user was going, and
  sign-in sends them back after checking it is a path on this site
  (`frontend-security`). Check in the loader or `beforeLoad`, before the
  screen renders, never in an effect that flashes the page first. The
  server still checks every request (`backend-auth`).

```ts
// TanStack Router: every route under /_app needs a signed-in user
export const Route = createFileRoute("/_app")({
  beforeLoad: ({ context, location }) => {
    if (!context.auth.user) {
      throw redirect({ to: "/sign-in", search: { returnTo: location.href } });
    }
  },
});
```

## 4. Where data fetching lives

- Each feature owns its queries and mutations in `api/`: a key factory,
  `queryOptions`, hooks (`useOrders(filters)`, `useRefundOrder()`). Route
  loaders start requests (`ensureQueryData`, the framework's `load`) with
  the same definitions the components read, so they share one cache entry.
- Leaf components never call `fetch`, and one endpoint never sits behind
  three keys that disagree; stale times and retries are `frontend-data`.

```ts
// features/orders/api/orders.queries.ts
export const orderKeys = {
  all: ["orders"] as const,
  lists: () => [...orderKeys.all, "list"] as const,
  list: (filters: OrderFilters) => [...orderKeys.lists(), filters] as const,
  detail: (id: string) => [...orderKeys.all, "detail", id] as const,
};

export const ordersQuery = (filters: OrderFilters) =>
  queryOptions({
    queryKey: orderKeys.list(filters),
    queryFn: ({ signal }) => call(api.GET("/orders", { params: { query: filters }, signal })),
    staleTime: 30_000,
  });

export const useOrders = (filters: OrderFilters) => useQuery(ordersQuery(filters));
```

## 5. Environment configuration

Whatever the browser can read is public. These put a value in the bundle:

| Stack | Public, in the bundle | Server only |
|---|---|---|
| Vite | `VITE_*` through `import.meta.env` | anything else (not exposed) |
| Next.js | `NEXT_PUBLIC_*`, inlined at build time | the rest, read in server code |
| Astro | `PUBLIC_*` | the rest, or `astro:env` fields with `access: "secret"` |
| SvelteKit | `$env/static/public` (`PUBLIC_*`) | `$env/static/private`, refused in client code by the build |
| Nuxt | `runtimeConfig.public` (`NUXT_PUBLIC_*`) | the rest of `runtimeConfig` |
| Expo | `EXPO_PUBLIC_*` | nothing on a device is secret: use a server |

- Never a secret behind a public prefix (private API keys, database URLs,
  signing keys, service tokens): a call that needs one goes through a
  server route (`frontend-security`). Keys designed to be public (a Stripe
  publishable key, a Sentry DSN, a domain-restricted maps key) are fine.
- Validate at build and start, so a missing value fails the deploy rather
  than a user's session (`@t3-oss/env-core` and `@t3-oss/env-nextjs` do the
  same and keep the server and client sets apart):

```ts
// src/lib/env.ts (zod 4; with zod 3, z.string().url())
import { z } from "zod";
export const env = z.object({ VITE_API_URL: z.url(), VITE_SENTRY_DSN: z.string().optional() })
  .parse(import.meta.env);
```

- One build promoted through environments (the same image in staging and
  production) cannot inline per-deploy values: serve them at runtime, as a
  `/config.json` written when the container starts and read before the app
  renders, or in Next read on the server and passed down.
- `.env.example` lists every name with no real value and is committed;
  `.env` and `.env*.local` are in `.gitignore`.

## 6. A typed API client

One module knows the base URL, credentials, headers and the error format;
everything else goes through it.

- With an OpenAPI document: types from `openapi-typescript`, calls through
  `openapi-fetch`; or query hooks generated by orval or Hey API
  (`@hey-api/openapi-ts`). Regenerate with a script (`pnpm gen:api`),
  never edit the output by hand.
- A TypeScript backend in the same repository: share its zod schemas from
  a package, or use its typed client (tRPC, Hono's `hc`).
- Failures become one error class the whole app understands
  (`frontend-errors`), built from the server's error body
  (`backend-errors`); network failures stay `TypeError`s.

```ts
// src/lib/api.ts
import createClient from "openapi-fetch";
import type { paths } from "./api.gen"; // npx openapi-typescript openapi.yaml -o src/lib/api.gen.ts

export const api = createClient<paths>({ baseUrl: env.VITE_API_URL, credentials: "include" });

export class ApiError extends Error {
  constructor(readonly status: number, readonly code: string, readonly fields: Record<string, string> = {},
              readonly requestId?: string, readonly retryAfter?: number) {
    super(`${status} ${code}`);
  }
}

/** The data of a successful call, or an ApiError built from the problem body. */
export async function call<T>(request: Promise<{ data?: T; error?: unknown; response: Response }>) {
  const { data, error, response } = await request;
  if (error === undefined) return data as T;
  const body = (error ?? {}) as { code?: string; errors?: { field: string; code: string }[] };
  const fields = Object.fromEntries((body.errors ?? []).map((e) => [e.field, e.code]));
  const retryAfter = Number(response.headers.get("retry-after")) || undefined;
  throw new ApiError(response.status, body.code ?? `http_${response.status}`, fields,
                     response.headers.get("x-request-id") ?? undefined, retryAfter);
}
```

## 7. Imports

- Path aliases: `@/` for `src/` in `tsconfig.json` `paths`, mirrored in
  the bundler (Next reads the tsconfig; Vite needs `vite-tsconfig-paths` or
  `resolve.alias`). No `../../../` chains.
- No barrel files in application code (an `index.ts` re-exporting a
  folder): each import loads the whole folder, slowing the dev server and
  the tests, side effects defeat tree shaking, and cycles hide behind them.
  Import from the file that defines the thing; for libraries with a huge
  barrel, import per module or icon, or use Next's `optimizePackageImports`.
- No import cycles: `import/no-cycle`, or
  `npx madge --circular --extensions ts,tsx src`.
- `import type` for types (`verbatimModuleSyntax` makes it a rule).

## 8. Dependencies

- **Needed?** The platform does a lot: `Intl` for dates, numbers, lists and
  relative times; `fetch` with `AbortSignal.timeout`; `crypto.randomUUID()`;
  `structuredClone`; `URLSearchParams`; `Object.groupBy`; `toSorted`.
- **Small, tree-shakable, alive?** Its cost on bundlephobia or pkg-size.dev
  (ES module exports, `"sideEffects": false`), a release in the last year,
  issues answered, types included, a licence you can ship.
- **Already there?** One date library, HTTP client, state library and icon
  set per project. Say in the report what you added and why (`code`).
- `npx knip` finds unused dependencies, files and exports; `pnpm why <pkg>`
  shows why something is installed and whether two versions ship.

## 9. Monorepos

Only when several apps share code (a web app, an admin and a mobile app
sharing types, schemas and components): pnpm workspaces with `workspace:*`
versions, and Turborepo or Nx to run and cache tasks (`turbo run build
--filter=web...`). Internal packages can export TypeScript source to the
apps beside them with no build step. One app is not a monorepo.

## 10. Lint rules that catch bugs

Turn on the rules that find real defects, and fix what they report rather
than silencing it: `react-hooks/rules-of-hooks` and `exhaustive-deps`
(conditional hooks, stale values in effects); `jsx-a11y` recommended
(`eslint-plugin-vuejs-accessibility` in Vue, Svelte's own warnings);
`import/no-cycle` (or `import-x/no-cycle`, slow, so in CI at least);
`@typescript-eslint/no-floating-promises` and `no-misused-promises` with
type-aware linting (`parserOptions.projectService: true`);
`@typescript-eslint/switch-exhaustiveness-check`; and
`no-restricted-imports` for what the project replaced (a second HTTP
client, deep imports into another feature). Biome and Oxlint are far
faster and cover many of these; check they have the rules you rely on
before switching.

## Check it

```sh
pnpm exec tsc --noEmit && pnpm lint
npx madge --circular --extensions ts,tsx src
npx knip
pnpm build
# no secret in what ships (.next/static for Next, build/ or out/ elsewhere)
grep -rlE "sk_live_|sk_test_|PRIVATE KEY|postgres(ql)?://" dist/ || echo "clean"
```

- One feature folder shows its routes, data, components and tests without
  searching; every route has a not-found and an error state; guarded
  routes redirect with a return path.
- `preview` on each route (`login` behind a sign-in): no console errors.

## Avoid

Folders by file kind in an app with several areas; a feature importing
another feature's internals; `fetch` in a leaf component; one endpoint
behind three query keys; barrel files; relative import chains; a secret
behind a public prefix; configuration that fails at runtime instead of at
build; a hand-edited generated client; a dependency for a few lines of
code, or a second one for a job the project already does; a monorepo for
one app.
