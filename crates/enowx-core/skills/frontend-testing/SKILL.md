---
name: frontend-testing
description: "Testing a frontend: what to test at which level, component tests with Testing Library, end-to-end tests with Playwright, mocking the network with MSW, accessibility checks, visual regression when it pays, and keeping tests fast and stable. Read before writing or fixing frontend tests."
---

# Testing a frontend

Generated tests assert that a component renders, snapshot a whole page
nobody will read the diff of, mock `fetch` by hand so the real client is
never exercised, find buttons by CSS class, wait with `setTimeout(2000)`,
and pass whether the feature works or not. Good tests do what a user does,
check what a user sees, and fail when the behaviour breaks and only then.
This is how to write them. The whole frontend is in `frontend`.

## 1. What to test at which level

| Level | Tool | What | How many |
|---|---|---|---|
| Unit | Vitest | pure logic: formatters, reducers, schema rules, price and date maths | many, milliseconds each |
| Component | Vitest, Testing Library, MSW | a component or a screen's behaviour as a user meets it: states, forms, interactions | most of the suite |
| End-to-end | Playwright | the journeys that earn money or lose data: sign-up, sign-in, checkout, the core create and edit flow, permissions | a few per journey, not per screen |
| Visual | Playwright screenshots, Chromatic | stable, shared components and key pages | only where it pays |
| Static | TypeScript, ESLint | types and known bug patterns | always on |

- Not worth testing: a library's own behaviour (that react-hook-form
  validates), styles and class names, implementation details (state
  variables, which hook was called), and large DOM snapshots, which break
  on every change and get updated without reading.
- A bug fixed gets the test that would have caught it, at the lowest level
  that can.

## 2. Component tests

- Vitest with `environment: "jsdom"` (or `happy-dom`) and a setup file
  importing `@testing-library/jest-dom/vitest`. When layout, focus or CSS
  matter, Vitest's browser mode runs the same tests in a real browser.
- Query as a user finds things, in this order: `getByRole` with its name,
  `getByLabelText`, `getByText`, `getByPlaceholderText`, `getByAltText`;
  `getByTestId` last. A control a test cannot reach by role and name is an
  accessibility bug (`frontend-accessibility`).
- `userEvent`, not `fireEvent`: `const user = userEvent.setup()`, then
  `await user.click()`, `user.type()`, `user.keyboard("{Enter}")`,
  `user.tab()`.
- Waiting: `findBy*` for what appears later, `waitFor` for an assertion
  that becomes true, `queryBy*` to assert absence,
  `waitForElementToBeRemoved` for a loading state leaving.
- Assert what the user sees: text, roles, `toBeDisabled()`, `toHaveFocus()`,
  `toHaveAccessibleDescription()`, the URL; never internal state.
- Render with the providers the app uses, fresh for every test:

```tsx
// src/test/render.tsx
export function renderWithProviders(ui: ReactElement) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } }, // errors show at once
  });
  return {
    user: userEvent.setup(),
    ...render(<QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>),
  };
}
```

  Add the router (a memory history at the route under test), i18n and
  theme providers the same way. With retries left on, every error test
  waits through three backoffs.

```tsx
test("sends a reminder for an overdue invoice", async () => {
  const { user } = renderWithProviders(<InvoicesPage />);
  const row = await screen.findByRole("row", { name: /INV-204/ });
  await user.click(within(row).getByRole("button", { name: "Send reminder" }));
  expect(await screen.findByRole("status")).toHaveTextContent("Reminder sent");
});
```

## 3. The network with MSW

- MSW handlers describe the API once, and serve the tests (`setupServer`
  from `msw/node`), the browser during development (`setupWorker` from
  `msw/browser`) and stories. Never mock `fetch` or the query hooks by
  hand: the real client, keys and error mapping should run.
- Fail loudly on anything unmocked, and reset overrides between tests:

```ts
// src/test/server.ts
export const server = setupServer(...handlers);

// src/test/setup.ts (Vitest's setupFiles)
beforeAll(() => server.listen({ onUnhandledRequest: "error" }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());
```

```ts
// src/test/handlers.ts
export const handlers = [
  http.get("/api/orders", ({ request }) => {
    const status = new URL(request.url).searchParams.get("status");
    return HttpResponse.json({ items: orders.filter((o) => !status || o.status === status), nextCursor: null });
  }),
];

// In one test: the endpoint fails once, then recovers
server.use(
  http.get("/api/orders", () => HttpResponse.json({ code: "unavailable" }, { status: 503 }), { once: true }),
);
```

- Every screen that loads data gets its four states tested: loading
  (`delay()` in the handler), empty, error (with its retry), success. And
  the statuses it treats specially: 401, 403, 404, 409, 422, 429
  (`frontend-errors`). `HttpResponse.error()` is a network failure.
- Fixtures from small factories (`buildOrder({ status: "paid" })`) typed
  with the API's generated types, so a contract change breaks them at
  compile time.

## 4. Forms

Fill fields by their labels, submit with the button's role, then assert:
the error message beside each invalid field (`toHaveAccessibleDescription`),
focus on the first invalid field, server `422` errors on their fields,
other failures at the form with the input kept, the success message or
the redirect, and one request for a double click (count calls in the
handler). The rules are in `frontend-forms`.

## 5. Accessibility checks

- vitest-axe or jest-axe in component tests, on each state (a dialog open,
  errors shown). jsdom has no layout, so contrast is not checked there.
- `@axe-core/playwright` in end-to-end tests, where contrast is real
  (`frontend-accessibility` has the call).
- Keyboard paths as tests: `user.tab()` and `toHaveFocus()` for focus
  order, Escape closing a dialog, focus returning to its opener.
- Playwright's `toMatchAriaSnapshot` checks the accessibility tree of a
  region (roles, names, structure) instead of its HTML.
- Automated checks are a floor, not the pass.

## 6. End-to-end with Playwright

```ts
// playwright.config.ts
export default defineConfig({
  testDir: "e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  reporter: process.env.CI ? [["html"], ["github"]] : "list",
  use: {
    baseURL: "http://localhost:4173",
    trace: "on-first-retry",
    screenshot: "only-on-failure",
    locale: "en-GB",
    timezoneId: "UTC",
    contextOptions: { reducedMotion: "reduce" },
  },
  projects: [
    { name: "setup", testMatch: /.*\.setup\.ts/ },
    { name: "desktop", use: { ...devices["Desktop Chrome"], storageState: "e2e/.auth/user.json" }, dependencies: ["setup"] },
    { name: "phone", use: { ...devices["Pixel 7"], storageState: "e2e/.auth/user.json" }, dependencies: ["setup"] },
  ],
  webServer: { command: "pnpm build && pnpm preview --port 4173", url: "http://localhost:4173", reuseExistingServer: !process.env.CI },
});
```

- Against a production build (`build` then `preview`), not the dev server,
  which differs (Strict Mode, unminified code, hot reload).
- Sign in once in a setup project and save `storageState`; the credentials
  come from environment variables, and `e2e/.auth` is in `.gitignore`:

```ts
// e2e/auth.setup.ts
import { expect, test as setup } from "@playwright/test";

setup("sign in", async ({ page }) => {
  await page.goto("/sign-in");
  await page.getByLabel("Email").fill(process.env.E2E_EMAIL!);
  await page.getByLabel("Password").fill(process.env.E2E_PASSWORD!);
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page.getByRole("heading", { name: "Orders" })).toBeVisible();
  await page.context().storageState({ path: "e2e/.auth/user.json" });
});
```

- Locators by role, label and text; a `data-testid` only where nothing
  else is stable; never CSS chains or XPath. `npx playwright codegen`
  suggests locators to start from.
- Web-first assertions wait by themselves (`await
  expect(locator).toBeVisible()`, `toHaveText`, `toHaveURL`); never
  `page.waitForTimeout`.
- Each test makes its own data through the API (the `request` fixture, or
  a seed endpoint that exists only in test environments), with unique
  names; no test depends on another or on their order.
- Third-party services you do not own are stubbed with `page.route`, or
  run in their test mode.
- In CI: `npx playwright install --with-deps chromium`, `--shard=1/4`
  across machines with `npx playwright merge-reports` after, the HTML
  report and traces kept as artifacts; `npx playwright show-trace` to
  read one.

## 7. Visual regression

- Only for stable, shared components and a few key pages; everything else
  is covered better by behaviour tests.
- `await expect(page).toHaveScreenshot("orders.png", { maxDiffPixelRatio:
  0.01, mask: [page.getByTestId("updated-at")] })`: animations are
  disabled by default; also fix fonts (loaded before the shot), data and
  time (`page.clock.setFixedTime`), and reduced motion.
- Screenshots differ between operating systems: take and compare them in
  the same environment, the official Playwright Docker image at the
  package's version. Update them on purpose (`--update-snapshots`), and
  look at each changed image.
- With Storybook, Chromatic (or Percy, Argos) adds a review step for
  visual changes.

## 8. Stable and fast

- Time: `vi.useFakeTimers()` with `vi.setSystemTime(new
  Date("2026-03-02T09:00:00Z"))`, and `userEvent.setup({ advanceTimers:
  vi.advanceTimersByTime })` with them; `page.clock.install()` in
  Playwright.
- Time zone and locale fixed: `TZ=UTC` in the test script (or
  `process.env.TZ = "UTC"` in a global setup, before any date is made),
  and an explicit locale.
- Data: seeded or hand-written fixtures; `faker.seed(1)` when using faker;
  no `Math.random()` in tests.
- Motion off: reduced motion emulated in Playwright; nothing waits for an
  animation.
- Isolation: each test builds its own state; Testing Library cleans up
  automatically when Vitest `globals` is on, otherwise `afterEach(cleanup)`.
  Run `vitest --sequence.shuffle` now and then to expose order
  dependence.
- A flaky test is a bug, in the test or the app: reproduce it with
  `--repeat-each=20` (Playwright), fix it, or quarantine it with an issue
  linked. CI retries exist to collect a trace, not to hide failures.
- Coverage (`vitest run --coverage`, the v8 provider) shows what no test
  touches; read it for untested error paths in logic that matters. It is a
  signal, never a target.

## Check it

- The suites pass: `pnpm test` (Vitest), `pnpm exec playwright test`.
- Break the code a new test covers (flip a condition, return early) and
  see the test fail; a test that never failed proves nothing.
- New end-to-end tests pass `--repeat-each=10`; the component suite passes
  shuffled.
- Each data screen has tests for loading, empty, error and success; each
  form for errors, server errors and success.
- The report says which suites ran, which could not (no browser, no seeded
  account), and why.

## Avoid

Tests that only check something rendered; snapshots of whole pages;
`fetch` or query hooks mocked by hand; queries by class name; `fireEvent`
where `userEvent` fits; fixed sleeps; retries left on in component tests;
end-to-end tests that click through setup, share data or depend on order;
running end-to-end against the dev server; screenshots compared across
operating systems; flaky tests retried until green; coverage chased as a
number.
