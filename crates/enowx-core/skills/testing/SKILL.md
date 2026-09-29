---
name: testing
description: "Writing tests that earn their keep: what tests are for, choosing the level (unit, integration, end-to-end) by risk, testing behaviour through public surfaces, a new test that fails first for the right reason, determinism, speed, names that describe behaviour, reading the project's existing setup first, and reporting which test proves what. The principles and which skill holds each. Read before writing or changing tests."
---

# Tests that earn their keep

A generated test suite is easy to spot: tests that mirror the implementation
line by line, a mock for every collaborator, assertions that cannot fail
(`expect(result).toBeDefined()`), snapshot files nobody reads, `sleep(2000)`
before an assertion, tests that pass alone and fail together, and 100%
coverage without a single edge case. It is green the day it is written and
never catches anything afterwards. This skill is the principles, the order
of work, and which skill holds the detail for each part.

## 1. What tests are for

- **Protecting behaviour people rely on** (users, callers, other services),
  so the code can change safely. A refactor keeps the tests green; a change
  of behaviour turns them red. A test that breaks on every refactor protects
  the implementation, not the behaviour.
- **Executable documentation**: the names and cases say what the code does,
  including the odd cases, and they cannot go out of date.
- **Pinning fixed bugs**: every bug fixed gets a test that failed for the
  reported reason (`testing-reproduce`).
- A test is worth writing when the bug it can catch is likely or costly and
  the test is cheap to run and keep. A test whose only way to fail is a
  refactor costs more than it returns.
- Tests are code: clear names, no dead helpers. But prefer readable over
  clever: repeat a line of setup when it makes the test readable on its own.

## 2. Read the project first

Before writing a test, find these, and match them:

| Find | Where to look |
|---|---|
| Framework and runner | `package.json` scripts and devDependencies, `pyproject.toml` `[tool.pytest.ini_options]`, `_test.go` files, `Cargo.toml` dev-dependencies, `build.gradle` or `pom.xml`, test `*.csproj`, `Gemfile`, `composer.json` |
| Where tests live, how they are named | colocated `*.test.ts`, `tests/`, `test_*.py`, `*_test.go`, `#[cfg(test)]` modules and `tests/`, `src/test/java`, `spec/` |
| Fixtures, factories, helpers | `conftest.py`, `test/helpers`, `testutil`, `factories/`, `spec/support`, a `renderWithProviders` |
| How the database and services are provided | `docker-compose.yml`, Testcontainers setup, `.env.test`, in-memory fakes |
| How CI runs them | `.github/workflows/`, `.gitlab-ci.yml`, `Makefile`, `justfile` |

Run the existing suite once before changing anything and note what already
fails, so an old failure is neither blamed on your change nor claimed fixed.
Use the project's framework, assertion style, fixtures and naming; never add
a second test framework or assertion library.

How to run one test, by stack:

```bash
npx vitest run src/cart.test.ts -t "applies the discount"
npx jest src/cart.test.ts -t "applies the discount"
npx playwright test e2e/checkout.spec.ts:42
pytest tests/test_cart.py::test_applies_discount
go test ./cart -run 'TestTotal/discount$' -count=1
cargo test applies_discount            # or: cargo nextest run applies_discount
./gradlew test --tests 'shop.CartTest.appliesDiscount'
mvn test -Dtest='CartTest#appliesDiscount'
dotnet test --filter "FullyQualifiedName~CartTests.AppliesDiscount"
bundle exec rspec spec/cart_spec.rb:42
php artisan test --filter=applies_discount
```

## 3. Choose the level by risk and cost

| What is tested | Level | Skill |
|---|---|---|
| Calculations, parsing, validation rules, state machines, formatting | Unit | `testing-unit` |
| An endpoint with its database, queries and constraints, serialisation, a client against a faked service, a job's effect | Integration | `testing-integration`, `backend-testing` |
| A component's behaviour as the user sees it | Component | `frontend-testing` |
| A journey that makes money or loses data (sign-up, checkout, the core task) | End to end | `testing-e2e` |
| The agreement between services owned by different teams | Contract | `testing-integration` |

- The test pyramid (many unit, fewer integration, few end-to-end tests) fits
  logic-heavy code. The testing trophy (static checks at the base, most
  tests at integration level, few end-to-end) fits web applications, where
  the bugs live in the wiring between parts. In practice: pure logic gets
  unit tests, anything that touches a database or the network gets
  integration tests, and 5 to 20 journeys get end-to-end tests.
- Choose the lowest level that can show the failure you care about: it is
  faster, and its failure points at the cause.
- Do not test what the type checker or linter already proves.
- Speed budgets to aim for: a unit test in milliseconds and the unit suite
  in seconds; the integration suite in a few minutes; end-to-end shards
  under 10 to 15 minutes in CI.

## 4. The rules for each test

- **Arrange, act, assert**, separated by a blank line. One act per test.
- **One behaviour per test.** Several assertions are fine when they describe
  one outcome (the status and the body), not two behaviours.
- **Assert on outcomes the caller sees**: a return value, a status and body,
  rows in the database, a message published, text on screen. Not private
  methods, internal call order or how many times a helper ran, unless the
  call is the outcome (one charge, one email).
- **Assertions that can fail**: the exact value (`toBe(9_000)`), not
  `toBeTruthy()`; never an expected value computed with the code under test.
- **Edge cases and failure paths**, not only the happy path: empty,
  boundaries, invalid input, permissions, a failing dependency. The case
  checklist is in `testing-unit`.
- **Deterministic**: time frozen or injected, randomness seeded, the network
  faked at the edge, time zone and locale fixed, no dependence on order.
- **Fast**: no sleeps; expensive setup (a container) once per run.
- **Independent**: each test creates its own data and runs alone, in any
  order, in parallel.
- **Names describe behaviour**: `refuses_a_payment_for_a_paid_order`,
  `it("returns 404 for another user's order")`, not `test_pay_2` or
  `it("works")`.
- No `.only` or skipped tests committed; a skip carries a reason and an
  issue link.

```ts
// Mirrors the implementation: passes whatever the discount rule is.
it("calls calculateDiscount", () => {
  const spy = vi.spyOn(pricing, "calculateDiscount");
  checkout(cart);
  expect(spy).toHaveBeenCalled();
});

// States the rule, and fails when the rule breaks.
it("gives 10% off orders of 100.00 or more", () => {
  const cart = cartWith({ subtotalCents: 10_000 });

  const order = checkout(cart);

  expect(order.discountCents).toBe(1_000);
  expect(order.totalCents).toBe(9_000);
});
```

## 5. Red first

1. Write or change the test.
2. Run it and watch it fail, and read why: it must fail on the assertion
   about the behaviour, not on an import error, a typo, a missing fixture or
   a timeout.
3. Make the change. Run the test: it passes.
4. Run the neighbouring tests, then the whole suite.

A test added to code that already works (coverage, characterisation) is
proved differently: break the code on purpose (flip a comparison, delete a
line), see the test fail, restore. A test that cannot fail is worse than
none: it reports safety that is not there.

Mutation testing automates that check: the tool changes the code (`>` to
`>=`, a call removed, a constant returned) and reruns the tests; a mutant
that survives marks an assertion gap. Run it on the module you changed:

```bash
npx stryker run --incremental --mutate "src/pricing/**/*.ts"   # StrykerJS
dotnet stryker                                                  # Stryker.NET
cargo mutants -f src/pricing.rs                                 # or --in-diff pr.diff
mutmut run && mutmut results                                    # Python
mvn org.pitest:pitest-maven:mutationCoverage                    # PIT (Gradle: info.solidsoft.pitest)
vendor/bin/infection --threads=max --git-diff-lines             # PHP
```

## 6. Coverage is a map, not a goal

- Coverage shows what no test executes. Read the uncovered branches in the
  code you changed and decide which matter: error paths and boundaries
  usually do.
- Executed is not checked: 100% line coverage with weak assertions proves
  nothing. Mutation testing measures whether the assertions have teeth.
- Prefer branch coverage to line coverage where the tool offers it.
- Keep the project's threshold; never lower it and never add tests that run
  code without asserting to reach it. Coverage of the changed lines is a
  better gate than a global number (`testing-ci`).

```bash
npx vitest run --coverage
pytest --cov=src --cov-branch --cov-report=term-missing
go test -coverprofile=cover.out ./... && go tool cover -func=cover.out
cargo llvm-cov --html
```

## 7. Report which test proves what

Done means: the new tests fail without the change and pass with it, and the
whole existing suite passes (or its failures are named and shown to fail on
the base commit too). The report states:

```
Added
- tests/orders/test_pay.py::test_refuses_payment_for_paid_order: a second
  payment returns 409 and creates no second charge (failed before the fix:
  two charges)
Ran
- pytest tests/orders -q: 48 passed
- pytest -q: 612 passed, 2 skipped (skips pre-existing)
Not run
- e2e: no browser available here
```

Never claim a suite passed that was not run; say plainly what could not be
run and why.

## 8. Which skill for which job

- `testing-unit`: finding cases, tables of cases, test doubles, property
  tests, snapshots, each language's idioms.
- `testing-integration`: real databases and HTTP, containers, isolation,
  fakes at the network edge, contracts, migrations.
- `testing-e2e`: journeys with Playwright, Maestro or Detox, locators,
  signed-in state, traces.
- `testing-reproduce`: a bug report turned into a failing test, shrinking,
  `git bisect`.
- `testing-flaky`: tests that pass and fail without a change: causes,
  repeats, quarantine.
- `testing-data`: factories, unique and seeded values, time, golden files,
  no real personal data.
- `testing-ci`: pipelines: order, sharding, caching, services, reports,
  required checks.
- `backend-testing`: server rules and endpoints through HTTP and a real
  database.
- `frontend-testing`: components with Testing Library, MSW, accessibility,
  visual checks.
- `mobile-testing`: app tests, the device matrix, Maestro, Detox, XCUITest,
  Espresso.

## Check it

- The new test failed before the change (keep the output) and passes after.
- One test, then its file, then the full suite, with the project's own
  commands; counts recorded.
- The new tests run in random order and repeatedly:
  `vitest run --sequence.shuffle`, `pytest -p randomly` (pytest-randomly),
  `go test -shuffle=on -count=5 -race`.
- The diff has no `.only`, no unexplained skip, no `sleep` or
  `waitForTimeout`, no real clock or network in the new tests.

## Avoid

Tests that mirror the implementation; mocks of your own pure code, or of
libraries you do not own; assertions that cannot fail; large snapshots
nobody reviews; sleeps; shared mutable fixtures; order dependence; the real
network or clock; skipping the red step; lowering thresholds, deleting or
skipping failing tests to get green; claiming a pass that was not run.
