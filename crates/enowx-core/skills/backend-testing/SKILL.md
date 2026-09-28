---
name: backend-testing
description: "Testing a backend: which tests to write for a rule or an endpoint, running through the HTTP layer against a real database, fakes for other services, fixtures and factories, time, concurrency and permission cases. Read before writing or fixing tests for server code."
---

# Tests for server code

One part of the backend; the whole is in the `backend` skill. Use the
project's test framework, folders and helpers; with none, the stack's usual
one (Vitest, pytest, Go's `testing`, `cargo test`, Pest or PHPUnit).

## 1. What to test

- The rules first: each business rule has a test that fails when the rule is
  broken ("an order cannot be paid twice", "stock never goes below zero").
  Pure rules are tested as functions, fast and many.
- Each endpoint through its HTTP layer (the framework's test client:
  `app.request()`, `fastify.inject`, supertest, `httpx.AsyncClient`,
  `httptest`, `oneshot`, Laravel's `postJson`), checking the status, the
  body's shape and what changed in the database.
- The ways it fails, not only the happy path:
  - invalid input: `400` with the field errors;
  - not signed in: `401`; signed in as someone else: `403` or `404`, and the
    other user's record untouched;
  - missing record: `404`; conflict (duplicate, already paid, stale version):
    `409`;
  - a dependency failing: the right status and nothing half-written.
- Lists: the page size, the next page, the last page, an empty list, a
  filter that matches nothing.
- Anything two requests can race (stock, balances, unique values): run two at
  once and check the result is still right.
- A bug fix starts with a test that fails for the reported reason.

## 2. The database

- Tests use a real database of the same kind as production: PostgreSQL in a
  container (Testcontainers, or the one in `docker compose`) for PostgreSQL,
  a temporary file for SQLite. Not SQLite standing in for PostgreSQL, and not
  mocks of the query layer: they pass while the SQL is wrong.
- Migrations run once per test run; each test starts clean (a transaction
  rolled back after it, or truncation), so tests do not depend on each
  other's data or order.
- Data made by factories or small helpers (`createProduct({ stock: 2 })`)
  with only the fields the test cares about, not a shared fixture every test
  edits.

## 3. Other services

- Never the network in a test. Fake other services at their HTTP boundary
  (MSW or nock, respx, `httptest.Server`, wiremock, `Http::fake`) or behind
  the client module's interface, including their failures: a timeout, a
  `429`, a `500`, a malformed reply.
- Webhooks tested with a correctly signed payload, a wrong signature, and the
  same event twice.
- Jobs run inline or drained in the test, and checked for what they did.

## 4. Time and randomness

- The clock is passed in or faked (`vi.useFakeTimers`, `freezegun` or
  `time-machine`, a `Clock` interface, `Carbon::setTestNow`), so expiry,
  schedules and "created today" are tested without sleeping.
- Random ids and tokens do not appear in assertions; check their shape.

## 5. Writing them well

- A test name says the behaviour: `refuses_a_payment_for_a_paid_order`, not
  `test_pay_2`.
- Arrange, act, assert, with one reason to fail. Assert the status and the
  body separately, so a failure says which.
- Fast: the whole suite runs in well under a minute for a small service, so it
  is run on every change.
- Done means the new tests fail without the change and pass with it, and the
  whole suite passes. Say which test proves which rule in the report.

## Avoid

Only the happy path; mocks of the ORM; SQLite in tests for a PostgreSQL app;
a test that calls a real provider; tests that pass only in order; `sleep` to
wait for time or a job; snapshot tests of whole responses that nobody reads;
a test that cannot fail.
