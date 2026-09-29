---
name: frontend
description: "Engineering a web frontend beyond its looks: where code goes, state and data, forms, accessibility, performance, testing, SEO, security and errors, the defaults for a new app, and which skill holds each. Read before building or changing a web application's behaviour, alongside `ui` for its look."
---

# Frontend engineering

A generated frontend looks right in the screenshot and fails in the first
minute of use: the filters are gone after a reload, data is fetched in an
effect with a race and no cache, a spinner never turns into an error, the
custom dropdown cannot be opened from the keyboard, the sign-in page ships
2 MB of JavaScript, and an API key sits in a `VITE_` variable for anyone
to read. This skill is the engineering under the look: how to read a
frontend project, how its pages should render, where each kind of state
lives, the defaults for a new app, what a feature needs before it is done,
and which `frontend-*` skill holds the detail. The look is the `ui` skill.

## 1. Read the project before changing it

Find out how this frontend already works, and keep to it. A second router,
data layer, form library or styling method beside the first is a defect.

- `package.json`: the scripts (`dev`, `build`, `typecheck`, `lint`,
  `test`, `test:e2e`), the framework and its major version (React 18 or
  19, Next 14 to 16, Vue 3, Svelte 4 or 5, Angular), the `engines` and
  `packageManager` fields. The lock file names the package manager
  (`pnpm-lock.yaml`, `package-lock.json`, `yarn.lock`, `bun.lock`): use
  that one and no other.
- The router and its conventions: `app/` (Next App Router), `pages/`,
  `src/routes/` (SvelteKit, TanStack Router), `app/routes.ts` (React Router
  7), a `router` module (Vue Router, Angular).
- The data layer: search for `useQuery`, `useSWR`, `createApi`, `loader`,
  `load(`, `useFetch` and the module that wraps `fetch`. New code uses the
  same.
- The component library (`components.json` means shadcn/ui), the tokens
  and icon set (`ui`), the form and schema libraries (react-hook-form, zod,
  valibot), the state library, the i18n setup (`i18n`).
- `tsconfig.json` (is `strict` on, which path aliases), the lint and format
  config (`eslint.config.*`, `biome.json`, `.prettierrc`), the test setup
  (`vitest.config.*`, `playwright.config.*`, a folder of test helpers).
- `.env.example` and how configuration reaches the browser; how it builds
  and deploys (a Dockerfile, `vercel.json`, `netlify.toml`, a CI workflow).

Then run what exists before touching anything, so what already fails is
known and not blamed on your change:

```sh
pnpm install --frozen-lockfile   # npm ci, yarn install --immutable, bun install --frozen-lockfile
pnpm typecheck                   # or tsc --noEmit, vue-tsc --noEmit, svelte-check, astro check
pnpm lint && pnpm test && pnpm build
```

## 2. How pages render

Choose per route, by the content, not once for the whole product:

| Content | Rendering | Typical stack |
|---|---|---|
| Changes when someone publishes: docs, blog, marketing, a business's pages | Static (SSG), rebuilt or revalidated on change | Astro, Next static or ISR, SvelteKit `prerender`, Nuxt `generate` |
| Public, changes per request or per visitor, must be indexed or shared | Server-rendered (SSR), streamed | Next App Router, SvelteKit, Nuxt, React Router 7 framework mode |
| An application behind a sign-in | Client-rendered (SPA) from a CDN | Vite with React, Vue or Svelte and a client router |
| Mostly static with a few interactive parts | Islands: static HTML, only the parts hydrate | Astro with `client:visible` or `client:idle` components |

- One product often mixes them: the marketing site static, the app an SPA,
  on one domain or two.
- An SPA is right behind a sign-in: crawlers never see those screens, and
  a static host is cheap and fast. Public content rendered only in the
  browser is indexed late or badly and shows empty link previews
  (`frontend-seo`).
- Server rendering has costs: a server to run, hydration work in the
  browser, and code that must not touch `window` during render. Do not
  choose it for an app nobody reaches through search.
- Progressive enhancement where the stack gives it cheaply: navigation is
  `<a href>`, forms are `<form>` with a submit button and an `action`, so
  they work before the JavaScript arrives and when it fails (Next server
  actions, React Router `<Form>`, SvelteKit form actions with
  `use:enhance`). In an SPA, at least real links and real forms, so
  middle-click opens a tab and Enter submits.

## 3. The kinds of state

Server data lives in a query cache and is never copied into a store
(`frontend-data`). What should survive a reload or a shared link (filters,
sort, page, tab, the selected record, the search) lives in the URL. Form
values live in the form library until they are submitted
(`frontend-forms`). What is left, such as a menu that is open or a panel
that is collapsed, is local component state, lifted only as far as the
nearest parent that needs it. Values computed from other values are
computed during render, not stored. A global store is for the little
client state that many distant components write. Where each piece goes,
with examples: `frontend-state`.

## 4. Defaults for a new app

When the project and the brief name nothing, start here, and say what you
chose and why in the report:

| Need | Default | Choose otherwise when |
|---|---|---|
| An app behind a sign-in | Vite, React, TypeScript, with TanStack Router (typed routes and search params) or React Router | the team writes Vue (Vite, Vue 3, Vue Router, Pinia) or Svelte (SvelteKit) |
| Public pages that need SEO or server rendering | Next.js with the App Router | the team knows SvelteKit or Nuxt; React Router 7 framework mode for a loader and action style |
| A content site: docs, blog, marketing | Astro | one page with little behaviour: plain HTML and CSS (`ui-stack-plain`) |
| Server data | TanStack Query | the project uses SWR or RTK Query, or the framework's loaders cover it |
| Forms | react-hook-form with zod | TanStack Form; Conform with server actions; VeeValidate in Vue; Superforms in SvelteKit |
| URL state | nuqs, or TanStack Router's search params | SvelteKit's `page.url`, Vue Router's `query` |
| Client state | local state, then context, then Zustand | Pinia in Vue, runes in Svelte |
| Components | the library `ui` names for the stack (shadcn/ui on Radix with Tailwind) | the project already has one |
| Unit and component tests | Vitest, Testing Library, MSW | Jest when the project already runs it |
| End-to-end tests | Playwright | Cypress when the project already runs it |
| Lint and format | ESLint (flat config) and Prettier | Biome, one fast tool, when it has the rules you rely on |
| Runtime and packages | Node 22 or 24 (the LTS lines), pnpm | the lock file in the repository decides |

- TypeScript with `strict` on, and `noUncheckedIndexedAccess` in a new
  project; no `any` (`code`).
- Target the browsers the users have. "Baseline widely available" (in
  every major browser for 30 months) is a sound default; do not ship
  polyfills for what every supported browser already has.

## 5. Starting a new app

- Scaffold with the framework's own tool and answer its questions with
  flags: an agent's shell cannot answer a prompt, and a waiting prompt
  hangs the command. `--help` lists the flags; if it still asks, pass the
  one it names.

```sh
pnpm create vite web --template react-ts   # an SPA with React and TypeScript
pnpm create next-app --help                # read the flags, then run it with every answer given
npx sv create --help                       # SvelteKit's scaffolder, the same way
```

- Before the first feature: `strict` TypeScript and the `@/` alias; lint
  and format with the rules in `frontend-architecture`; Vitest, Testing
  Library and MSW with a `renderWithProviders` helper (`frontend-testing`);
  Playwright for the first journey; the env module, API client and query
  client (`frontend-architecture`); i18n from the first string (`i18n`);
  the component library and tokens (`ui`).
- Scripts named as everyone expects (`dev`, `build`, `preview`,
  `typecheck`, `lint`, `test`, `test:e2e`), the lock file committed, and
  `.env.example` listing every variable.

## 6. Done means

A feature is done when:

- **Every state is built**: empty, loading (a skeleton on the first load
  only), error with a way out, success, and the slow case (`ui` section 8,
  `frontend-data`, `frontend-errors`).
- **The keyboard and a screen reader work**: everything reachable and
  operable, every control named, focus moved and returned where it should
  be (`frontend-accessibility`).
- **360 to 1440px**: nothing past the right edge, touch targets of 44px,
  the phone layout designed rather than squeezed (`ui-layout`).
- **Errors are handled where they happen**, and what the user typed is
  kept (`frontend-errors`).
- **The console is clean**: no errors, no key or hydration warnings.
- **Tests** cover the behaviour that matters (`frontend-testing`).
- **Typecheck and lint** are clean for what you touched.
- **The bundle** has not grown without a reason you can name
  (`frontend-performance`).
- **Text** goes through i18n (`i18n`), and no secret is in client code
  (`frontend-security`).
- `ui_check` and `preview` report nothing new on the screens you built.

## 7. Which skill for which job

| Job | Skill |
|---|---|
| Folders, component layers, routing, env config, the API client, dependencies | `frontend-architecture` |
| Where state lives: URL, local, context, stores, storage, state machines | `frontend-state` |
| Loading and saving server data: cache, waterfalls, mutations, pagination, real time | `frontend-data` |
| Form behaviour: validation timing, messages, submission, autosave, steps | `frontend-forms` |
| Semantics, ARIA patterns, keyboard, focus, live regions, assistive technology | `frontend-accessibility` |
| Core Web Vitals, budgets, images, fonts, JavaScript weight, caching | `frontend-performance` |
| Component and end-to-end tests, MSW, visual regression, flaky tests | `frontend-testing` |
| Public pages: titles, canonicals, sitemaps, social cards, structured data | `frontend-seo` |
| XSS, sanitising, CSP, tokens, CSRF, third-party scripts, dependencies | `frontend-security` |
| Failures: boundaries, status codes, offline, stale deploys, reporting | `frontend-errors` |
| The look: direction, layout, type, colour, components, pages and parts | `ui` |
| Animation and transitions | `motion` |
| Every piece of text a user reads, in every language | `i18n` |
| Structure, names, types and comments in any code | `code` |

Read the ones for the parts you build, when you come to them. The words on
the page follow `writing`; the server side is `backend`.

## 8. Habits that save a rework

- Build a thin slice end to end first: the route, its data, its four
  states and one test. Then widen and polish.
- When the API is not ready, agree the contract (`backend-api`), generate
  types from it, and serve it from MSW handlers; the same handlers drive
  the tests later.
- Hydration mismatches come from output that differs between server and
  browser: `Date.now()`, `Math.random()`, `window` or `localStorage` read
  during render, formatting in the machine's locale. Use `useId` for ids,
  format with an explicit locale and time zone, and read browser-only
  values after mount.
- Never silence a warning to finish: a key warning is a list bug, a
  hydration warning is a rendering bug, an `exhaustive-deps` warning is
  usually a stale value.
- Dependencies stay few and the project's own (`frontend-architecture`).

## Check it

- The project's own commands pass: typecheck, lint, tests, build. Say
  plainly which could not be run.
- `preview` on every screen you built: `url` and `start` (Vite:
  `http://localhost:5173/` with `npm run dev -- --port 5173 --strictPort`;
  Next: `http://localhost:3000/` with `npm run dev -- -p 3000`), and
  `login` with a seeded test account for screens behind a sign-in. Read its
  console errors, overflow, unnamed controls, contrast and touch targets
  at each width.
- `ui_check` on the files you changed.
- Walk the feature with the keyboard only, then with the network throttled
  and with the API failing (`frontend-errors`).
- Set filters, reload, press Back, and open a copied link in a private
  window: the same view each time.

## Avoid

A page that works only on the happy path; server data copied into a
store; filters that vanish on reload; fetch calls scattered through leaf
components; a clickable `div`; a secret behind a public environment
prefix; a second library beside the project's own for the same job; a
client-rendered marketing site; a feature reported done with console
errors, a failing typecheck or an untried keyboard path.
