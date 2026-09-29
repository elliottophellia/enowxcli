---
name: testing-e2e
description: "End-to-end tests that stay green: covering only the journeys that matter, Playwright for the web and Maestro or Detox for mobile, locators by role and label, waiting for conditions never for time, setting up data through APIs, reusing signed-in state, isolation and parallel runs, retries as a detector not a cure, traces on failure, running against preview deployments, and accessibility checks inside the journey. Read before writing or fixing end-to-end tests."
---

# End-to-end tests that stay green

A generated end-to-end suite clicks through every screen, finds elements by
CSS class and `nth-child`, waits `waitForTimeout(3000)` between steps, signs
in through the form in every test, shares one test account, and runs against
the dev server. It takes 40 minutes, fails one run in five, and gets rerun
until green, so nobody believes it. This skill: a few journeys, locators the
user would recognise, conditions instead of time, data set up through the
API, signed-in state reused, isolation, and traces that explain a failure.
Component tests are in `frontend-testing`, device specifics in
`mobile-testing`, principles in `testing`.

## 1. Which journeys

- 5 to 20 journeys that make money, lose data or break often: sign-up,
  sign-in and password reset, the core task (create, edit, submit the main
  thing), checkout in the provider's test mode, the permission that matters,
  settings that change behaviour, an import or export people rely on.
- One test per journey, start to visible result, in named steps; it checks
  what the user sees and that the outcome persisted (reload, or the API).
- Everything else goes lower: validation and component states in component
  tests, per-endpoint rules in integration tests. Keep the journey list
  visible (the spec file names can be it), so the suite stays small.

## 2. Playwright setup

```ts
import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined, // scale out with --shard
  reporter: process.env.CI ? [["blob"], ["github"]] : [["html"]],
  use: {
    baseURL: process.env.BASE_URL ?? "http://localhost:3000",
    trace: "on-first-retry",
    screenshot: "only-on-failure",
    video: "retain-on-failure",
    reducedMotion: "reduce",
    timezoneId: "UTC",
    locale: "en-GB",
  },
  projects: [
    { name: "setup", testMatch: /.*\.setup\.ts/ },
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"], storageState: "playwright/.auth/user.json" },
      dependencies: ["setup"],
    },
  ],
  webServer: process.env.BASE_URL
    ? undefined
    : {
        command: "npm run build && npm run start",
        url: "http://localhost:3000",
        reuseExistingServer: !process.env.CI,
        timeout: 120_000,
      },
});
```

- Defaults: 30 s per test, 5 s per `expect`. Do not raise them globally; mark
  a known slow journey with `test.slow()`.
- Playwright recommends one worker per CI machine for stability: get speed
  from sharding (`testing-ci`); more workers only for isolated tests.

## 3. Locators

In order of preference:

1. `page.getByRole("button", { name: "Place order" })`: what assistive
   technology sees, so a broken accessible name fails the test, as it should.
2. `page.getByLabel("Email")` for form fields.
3. `page.getByText("Order confirmed")` for content that is not a control.
4. `page.getByTestId("order-total")` (`data-testid`, renamed with
   `testIdAttribute`) for what has no role or stable text.
5. Never class names, `nth-child`, generated classes or DOM paths.

- Scope by chaining and filtering:
  `page.getByRole("row").filter({ hasText: "ord_42" }).getByRole("button", { name: "Refund" })`.
- A locator matching two elements throws on an action: make it specific
  rather than adding `.first()`. One fixed locale; test ids where copy
  changes often. `npx playwright codegen` proposes locators to clean up.

## 4. Conditions, never time

- Actions wait for the element to be attached, visible, stable, enabled and
  receiving events. Web-first assertions retry until their timeout:
  `await expect(page.getByRole("status")).toHaveText("Saved")`,
  `toBeVisible()`, `toHaveURL(/\/orders\/ord_/)`, `toHaveCount(3)`.
- Not `expect(await locator.isVisible()).toBe(true)`: it checks once and
  does not retry.
- Waiting on a request the screen does not show: register the wait before
  the action.

```ts
const saved = page.waitForResponse((r) => r.url().endsWith("/api/orders") && r.request().method() === "POST");
await page.getByRole("button", { name: "Place order" }).click();
await saved;
```

- State outside the page:
  `await expect.poll(() => orderStatus(id), { timeout: 10_000 }).toBe("paid")`,
  or a block with `await expect(async () => { ... }).toPass()`.
- `page.waitForTimeout()` is for debugging only; `networkidle` is
  discouraged too. `force: true` hides a real overlap bug.
- Time in the browser: `page.clock.install({ time: new Date("2026-09-29T09:00:00Z") })`
  then `page.clock.fastForward("01:00:00")` for expiry and "5 minutes ago".
  It fakes the browser's clock only; the server needs its own control.

## 5. Data through the API, unique per test

- Create what a journey needs through the API or a database helper; click
  only through the journey under test.
- Unique per test (worker index plus a UUID), so parallel workers and
  retries never collide; never one shared `test@test.com`. Fixtures package
  setup and teardown:

```ts
import { test as base, expect } from "@playwright/test";

type Project = { id: string; name: string };

export const test = base.extend<{ project: Project }>({
  project: async ({ request }, use, testInfo) => {
    const name = `Project ${testInfo.workerIndex}-${crypto.randomUUID()}`;
    const res = await request.post("/api/projects", { data: { name } });
    expect(res.ok()).toBeTruthy();
    const project: Project = await res.json();
    await use(project);
    await request.delete(`/api/projects/${project.id}`);
  },
});
```

- Reference data (plans, countries) is seeded once before the run.
- Mock only third parties the journey is not about (maps, analytics, a chat
  widget) with `page.route`; keep your own API real.

## 6. Signed-in state

```ts
// e2e/auth.setup.ts
import { test as setup, expect } from "@playwright/test";

const authFile = "playwright/.auth/user.json";

setup("sign in", async ({ page }) => {
  await page.goto("/sign-in");
  await page.getByLabel("Email").fill(process.env.E2E_USER_EMAIL!);
  await page.getByLabel("Password").fill(process.env.E2E_USER_PASSWORD!);
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page.getByRole("heading", { name: "Dashboard" })).toBeVisible();
  await page.context().storageState({ path: authFile });
});
```

- `playwright/.auth` goes in `.gitignore`: it holds live session cookies.
- One state file per role (admin, member), each from its own setup test; or
  sign in through the API in the setup to make it faster.
- Tests that change the account (password, deletion) create their own user.
- The sign-in journey itself runs signed out:
  `test.use({ storageState: { cookies: [], origins: [] } })`.
- A test account in a non-production environment, credentials from CI
  secrets; never a real person's account.

## 7. Isolation, parallel runs, retries

- Each test gets a fresh browser context (cookies, storage). No pages shared
  between tests, no `mode: "serial"` chains: a sequence is one test with
  `test.step()`s.
- `fullyParallel: true` works only when data is per test (section 5).
- Retries are a detector: a test that passes on retry is reported as flaky.
  Track those (`testing-flaky`); `--fail-on-flaky-tests` fails the run on
  any. Never raise retries to get green.
- Hunting a flaky test: `npx playwright test e2e/checkout.spec.ts
  --repeat-each=30 --retries=0 --workers=4`.
- Tags select suites: `test("checkout", { tag: "@smoke" }, ...)`, then
  `--grep @smoke` or `--grep-invert @quarantine`.

## 8. Failures you can read

- Traces on the first retry (or `retain-on-failure`): DOM snapshots per
  action, network, console and source (`npx playwright show-trace trace.zip`).
- Screenshots and video kept on failure; report, traces and `test-results/`
  uploaded as CI artefacts (`testing-ci` merges shard reports).
- A fixture listening to `page.on("pageerror")` fails the test on an
  unexpected script error. Locally: `--ui`, or `--debug` to step through.

## 9. Where it runs

- Against a production build (`npm run build && npm run start`), not the dev
  server: different code paths, overlays, slow first compiles.
- Per pull request against its preview deployment, on its own seeded
  database: `BASE_URL=https://pr-123.preview.example.com npx playwright test`.
  A protected preview needs the provider's automation bypass header, from a
  CI secret, in `use.extraHTTPHeaders`.
- Never against production with real users' data; a post-deploy smoke test
  there is separate and uses a dedicated synthetic account.
- Browsers where the users are: Chromium on every pull request; Firefox and
  WebKit nightly, or per pull request when analytics show real share
  (`devices["iPhone 15"]` for Safari on iOS, `devices["Pixel 7"]`).

## 10. Accessibility and visual checks in the journey

```ts
import AxeBuilder from "@axe-core/playwright";

const results = await new AxeBuilder({ page })
  .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"])
  .analyze();
expect(results.violations).toEqual([]);
```

- Scan at key steps: after the page loads, with a dialog open, after an
  error appears. Automated rules catch only part of the problems: do part
  of each journey by keyboard and check focus with `toBeFocused()`.
- `toMatchAriaSnapshot()` pins the structure of a key region (headings,
  landmarks, names) in a readable form.
- Screenshots (`await expect(page).toHaveScreenshot()`) only for stable
  screens, with fixed data, fonts and clock, animations disabled (the
  default), dynamic parts masked (`mask: [page.getByTestId("avatar")]`),
  and baselines made on the CI's operating system (the Playwright Docker
  image), since fonts render differently elsewhere.

## 11. Mobile end to end

- Maestro: YAML flows for iOS and Android apps (native, React Native,
  Flutter), with waits built in. Run
  `maestro test --format junit --output report.xml flows/`, select with
  `--include-tags smoke`, pass secrets with `-e USER_EMAIL=...`.

```yaml
appId: com.example.shop
tags:
  - smoke
---
- launchApp:
    clearState: true
- tapOn:
    id: "email"
- inputText: ${USER_EMAIL}
- tapOn: "Continue"
- extendedWaitUntil:
    visible: "Your orders"
    timeout: 10000
```

- Detox for React Native: `element(by.id("email"))`, and
  `waitFor(element(by.id("orders"))).toBeVisible().withTimeout(10000)`.
- The same rules: stable ids (test ids, accessibility identifiers), data
  through the API, a clean app state per flow. Device matrix, permissions
  and deep links in `mobile-testing`.

## Check it

- New or changed tests pass `--repeat-each=10 --retries=0` with several
  workers; then the full suite.
- Search the e2e folder for `waitForTimeout`, `.only`, `force: true`, CSS
  selectors and `nth-child`: none left.
- Make one test fail on purpose: the trace, screenshot and report appear in
  the artefacts, and the failure reads clearly.
- The journey list still fits on one screen.

## Avoid

Testing every screen end to end; CSS and XPath selectors; sleeps and
`waitForTimeout`; `force: true` clicks; signing in through the form in
every test; one shared account; data created by clicking; tests that depend
on each other; retries raised until green; the dev server or production as
the target; screenshots of changing screens; committed storage state.
