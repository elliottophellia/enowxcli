---
name: backend-testing
description: "Testing a backend: which tests to write for a rule or an endpoint, the layers (unit, HTTP-level against a real database, contract), test database strategies, factories, fakes for other services at the HTTP boundary, time, auth and permission cases, jobs, webhooks, concurrency, snapshot cautions and fast parallel runs in CI. Read before writing or fixing tests for server code."
---

# Tests for server code

The generated suite: one happy-path test per endpoint, the ORM mocked so no
SQL ever runs, SQLite standing in for PostgreSQL, a real call to the payment
provider, `sleep(2)` to wait for a job, a snapshot of a whole response nobody
reads, and tests that pass alone and fail together. One part of the backend;
the whole is in the `backend` skill, the principles in `testing`, and the
depth in `testing-integration`, `testing-data`, `testing-flaky` and
`testing-ci`. Use the project's test framework, folders and helpers; with
none, the stack's usual one (Vitest, pytest, Go's `testing`, `cargo test`,
Pest or PHPUnit).

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

## 2. The layers

| Layer | What it proves | How | How many |
|---|---|---|---|
| Unit | a rule: pricing, state changes, a permission predicate, a parser | plain calls, no I/O | many, milliseconds each |
| HTTP | routing, validation, auth, the query, the response and error format | the test client against a real database | each endpoint and each way it fails |
| Contract | responses match the published API; a provider still gives what its consumers use | schema validation, Schemathesis, Pact | per API |
| End to end | a few journeys across the running system | real HTTP (`testing-e2e`) | a handful |

```ts
// Unit: the rule as a table.
it.each([
  { status: "open", payable: true },
  { status: "paid", payable: false },
  { status: "refunded", payable: false },
])("an order that is $status can be paid: $payable", ({ status, payable }) => {
  expect(canPay({ status })).toBe(payable);
});

// HTTP: status, body and the database, asserted separately.
it("refuses a second payment", async () => {
  const order = await createOrder({ status: "paid" });
  const res = await app.request(`/orders/${order.id}/pay`, { method: "POST", headers: authAs(order.owner) });
  expect(res.status).toBe(409);
  expect(await res.json()).toMatchObject({ code: "already_paid" });
  expect((await findPayments(order.id)).length).toBe(1);
});
```

- Contracts: validate responses against the OpenAPI document inside the HTTP
  tests; `schemathesis run http://localhost:8000/openapi.json` generates
  requests from the schema and finds `500`s and responses that break it;
  `oasdiff breaking old.yaml new.yaml` in CI catches a breaking change
  (`backend-api`); Pact when separate teams own consumer and provider.

## 3. The database

- Tests use a real database of the same kind as production: PostgreSQL in a
  container (Testcontainers, or the one in `docker compose`) for PostgreSQL,
  a temporary file for SQLite. Not SQLite standing in for PostgreSQL, and not
  mocks of the query layer: they pass while the SQL is wrong.
- Migrations run once per test run; each test starts clean (a transaction
  rolled back after it, or truncation), so tests do not depend on each
  other's data or order.

| Strategy | Use for | Watch |
|---|---|---|
| Transaction per test, rolled back | most endpoint tests; the fastest | the app must use the test's connection; after-commit hooks never fire; no concurrency |
| Truncate after each test | tests that commit, race, or use several connections | slower as tables grow |
| Template database per worker or test | parallel workers on PostgreSQL | nothing connected to the template while it is cloned |

- The framework's own first: Laravel `RefreshDatabase`, Django `TestCase`
  and `TransactionTestCase`, `#[sqlx::test]` (a fresh database per test).
  Details and traps: `testing-integration`.
- SQLAlchemy async, one transaction per test, the code's own commits turned
  into savepoints:

```python
@pytest.fixture
async def session(engine: AsyncEngine) -> AsyncIterator[AsyncSession]:
    async with engine.connect() as conn:
        outer = await conn.begin()
        session = AsyncSession(bind=conn, join_transaction_mode="create_savepoint",
                               expire_on_commit=False)
        yield session
        await session.close()
        await outer.rollback()

@pytest.fixture
async def client(session: AsyncSession) -> AsyncIterator[AsyncClient]:
    app.dependency_overrides[get_session] = lambda: session
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as c:
        yield c
    app.dependency_overrides.clear()
```

## 4. Data

- Data made by factories or small helpers (`createProduct({ stock: 2 })`)
  with only the fields the test cares about, not a shared fixture every test
  edits: fishery in TypeScript, factory_boy or polyfactory in Python, Laravel
  model factories, a builder function in Go or Rust.
- Defaults make a valid record; values that must be unique come from a
  sequence, so parallel workers never collide; relations are built on
  purpose (`createOrder({ owner })`). More in `testing-data`.

## 5. Other services

- Never the network in a test. Fake other services at their HTTP boundary
  (MSW or nock, respx, `httptest.Server`, WireMock, `Http::fake`) or behind
  the client module's interface, including their failures: a timeout, a
  `429`, a `500`, a malformed reply.
- The client's base URL comes from config, so a test points it at the fake.
  An unexpected request fails the test rather than reaching the internet:
  MSW's `server.listen({ onUnhandledRequest: "error" })` (MSW 3:
  `onUnhandledFrame`), `Http::preventStrayRequests()`, respx's default of
  failing on unmatched routes.

```ts
it("reports the provider's rate limit as retryable", async () => {
  server.use(http.post("https://api.payments.example/v1/refunds",
    () => new HttpResponse(null, { status: 429, headers: { "Retry-After": "2" } })));
  const res = await app.request(`/orders/${order.id}/refund`, { method: "POST", headers });
  expect(res.status).toBe(503);
  expect(res.headers.get("retry-after")).toBe("2");
});
```

```python
async def test_charge_timeout_is_a_503(client, respx_mock):
    respx_mock.post("https://api.payments.example/v1/charges").mock(side_effect=httpx.ConnectTimeout)
    res = await client.post("/orders/o1/pay")
    assert res.status_code == 503
```

## 6. Time and randomness

- The clock is passed in or faked (`vi.useFakeTimers`, `freezegun` or
  `time-machine`, a `Clock` interface, `Carbon::setTestNow`), so expiry,
  schedules and "created today" are tested without sleeping.
- Timers and tickers: Go's `testing/synctest` (Go 1.25) runs goroutines
  against a fake clock; Rust `#[tokio::test(start_paused = true)]` with
  `tokio::time::advance` (tokio's `test-util` feature); Laravel
  `$this->travelTo()`.
- The database has its own clock: inside a rolled-back transaction,
  PostgreSQL's `now()` is the same for the whole test. Set timestamps in the
  rows a test depends on.
- Random ids and tokens do not appear in assertions; check their shape.

## 7. Auth and permissions

- Every endpoint gets the matrix, as a table test:

| Caller | Expect |
|---|---|
| anonymous | `401` |
| signed in, without the permission | `403` |
| another user or tenant, their record's id | `404`, and the record unchanged |
| the owner, or a role the rule allows | `2xx` |

- The "someone else's id" case is the one that catches the commonest API
  flaw (broken object level authorisation, first in the OWASP API Top 10).
- Callers are made by a helper (a token signed with a test key, the
  framework's `actingAs`, `Sanctum::actingAs($user, ['orders:read'])`), not
  by calling sign-in in every test; sign-in has its own tests (wrong
  password, locked account, rate limit: `backend-auth`).

## 8. Jobs and webhooks

- Jobs run inline or drained in the test, and are checked for what they did.
  Assert the enqueue first (payload, and only after the commit), then run
  the handler directly with that payload and assert its effect; run it twice
  and assert the effect happened once (`backend-jobs`).
- Webhooks tested with a correctly signed payload, a wrong signature, and the
  same event twice. Add a timestamp outside the tolerance (Stripe's is 300
  seconds) and events arriving out of order; compute the signature in the
  test with a test secret and the provider's scheme.
- Webhooks you send: a fake receiver answering `500` then `200` proves the
  retry, the signature header and the delivery log (`backend-integrations`).

## 9. Concurrency

- Races run for real: separate connections, committed data, outside the
  rolled-back transaction (truncate for these tests). Many callers, one
  winner:

```go
var wg sync.WaitGroup
errs := make([]error, 10)
for i := range errs {
	wg.Go(func() { errs[i] = store.Reserve(ctx, productID, 1) }) // stock is 1
}
wg.Wait()
// exactly one nil error, and the stock is 0, not -9
```

- Repeat a race test (`go test -count=20`, a loop in the test) before
  trusting it; `go test -race` finds data races in memory, not in the
  database.

## 10. Snapshots

- Only for small, stable outputs someone reviews: the OpenAPI document, one
  error body, a rendered email. Ids and times replaced first (Vitest
  property matchers such as `{ id: expect.any(String) }`, insta redactions,
  syrupy or inline-snapshot in Python).
- Never a whole list or record response: it breaks on every added field and
  gets re-approved unread. Assert the fields the test is about.

## 11. Fast, in parallel

- Fast: the whole suite runs in well under a minute for a small service, so it
  is run on every change; a few minutes for a large one, and past about 10,
  sharded in CI (`testing-ci`).
- Containers start once per run; each parallel worker has its own database:
  Vitest runs files in parallel (name the database by `VITEST_POOL_ID`),
  pytest-xdist `-n auto` (by `worker_id`), `go test ./...` runs packages as
  separate processes (a database per package), `php artisan test --parallel`
  makes one per process, nextest with `#[sqlx::test]` one per test.
- A test that sometimes fails is a bug in the test or the code, not noise
  (`testing-flaky`).

## 12. Writing them well

- A test name says the behaviour: `refuses_a_payment_for_a_paid_order`, not
  `test_pay_2`.
- Arrange, act, assert, with one reason to fail. Assert the status and the
  body separately, so a failure says which.
- Done means the new tests fail without the change and pass with it, and the
  whole suite passes. Say which test proves which rule in the report.

## Check it

- Run the suite the way CI does: `npx vitest run`, `pytest -q -n auto`,
  `go test -race -count=1 ./...`, `cargo nextest run` (or `cargo test`),
  `php artisan test --parallel`.
- Revert the change and run the new tests: they fail, for the stated reason.
- Run the suite twice and in random order (`vitest --sequence.shuffle`,
  pytest-randomly): the same result.
- No `.only`, `skip` or `xfail` left behind: `rg -n '\.only\(|\.skip\(|@pytest\.mark\.skip|t\.Skip\('`.

## Avoid

Only the happy path; mocks of the ORM; SQLite in tests for a PostgreSQL app;
a test that calls a real provider; tests that pass only in order; `sleep` to
wait for time or a job; snapshot tests of whole responses that nobody reads;
a test that cannot fail; a race tested inside one rolled-back transaction;
an endpoint without its "someone else's id" test; a webhook test with the
signature check turned off; one database shared by parallel workers.
