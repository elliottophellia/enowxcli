---
name: testing-integration
description: "Integration tests against real boundaries: the HTTP layer and a real database, containers for dependencies, isolating tests with transactions or fresh schemas, seeding, faking other services at the network edge, contract tests between services, testing migrations, controlling time, and keeping them fast enough to run on every change. Read before writing tests that cross a process or network boundary."
---

# Integration tests against real boundaries

The naive version mocks the ORM and the HTTP client, lets SQLite stand in for
PostgreSQL, shares one test database that every test and developer writes
to, and calls a real sandbox API from the suite. It stays green while the SQL
is wrong, a constraint is missing or a date is serialised in the wrong zone,
and it fails when two people run it at once. This skill: which boundaries to
test for real, how to provide them, how to isolate tests, how to fake what
you do not own, and how to keep it fast. Endpoint-by-endpoint cases for
servers are in `backend-testing`; principles in `testing`.

## 1. What to cover

| Boundary | Check |
|---|---|
| Routing and HTTP | the route, method, status, headers (`Location`, `Retry-After`), body shape |
| Validation | invalid input gives `400` or `422` with every field error; unknown fields refused |
| Auth | `401` without credentials; `403` or `404` for another user's record, which stays untouched |
| Queries | the right rows for filters, joins, sort order and pagination; no N+1 (count queries) |
| Constraints | unique, foreign key and check constraints map to the documented error (`409`) |
| Transactions | a failure halfway leaves nothing written |
| Serialisation | ISO 8601 times in UTC, money as integers, ids as strings, nulls, enum names |
| Error mapping | a dependency's timeout or `500` becomes the documented status, with nothing half-done |
| Jobs and events | enqueued with the right payload; the handler's effect when run |
| Caches | invalidated after a write |

One test per behaviour, failure paths included, through the framework's test
client (`app.request()`, `fastify.inject`, `httpx.AsyncClient`, `MockMvc`).

## 2. Real dependencies, started once

- The same engine and major version as production: PostgreSQL 17 in tests if
  production runs 17. Not SQLite or H2 standing in for PostgreSQL or MySQL:
  `ON CONFLICT`, JSONB, locking, collation and case rules differ.
- Testcontainers starts containers from the test code on random free ports
  and removes them afterwards (its Ryuk reaper), in Java, Go, Node, Python,
  .NET and Rust. It needs Docker or a compatible runtime (Podman, Colima,
  OrbStack) or Testcontainers Cloud.
- Or compose and CI job services, with tests reading `DATABASE_URL`.
- Start once per run (at most once per worker), never per test: a database
  container takes seconds. Per-test isolation comes from section 3.

```ts
// test/global-setup.ts (Vitest globalSetup)
import { PostgreSqlContainer } from "@testcontainers/postgresql";
import type { TestProject } from "vitest/node";

export default async function setup(project: TestProject) {
  const pg = await new PostgreSqlContainer("postgres:17-alpine").start();
  await migrate(pg.getConnectionUri());
  project.provide("databaseUrl", pg.getConnectionUri()); // inject("databaseUrl") in tests
  return async () => { await pg.stop(); };
}

declare module "vitest" {
  export interface ProvidedContext { databaseUrl: string }
}
```

```python
# conftest.py (recent releases also offer testcontainers.community.postgres)
from testcontainers.postgres import PostgresContainer

@pytest.fixture(scope="session")
def database_url():
    with PostgresContainer("postgres:17-alpine", driver="psycopg") as pg:
        url = pg.get_connection_url()
        run_migrations(url)
        yield url
```

```go
func TestMain(m *testing.M) {
	ctx := context.Background()
	pg, err := postgres.Run(ctx, "postgres:17-alpine",
		postgres.WithDatabase("app_test"),
		postgres.BasicWaitStrategies(),
	)
	if err != nil {
		log.Fatalf("start postgres: %v", err)
	}
	dsn = pg.MustConnectionString(ctx, "sslmode=disable")
	if err := migrateUp(dsn); err != nil {
		log.Fatalf("migrate: %v", err)
	}
	code := m.Run()
	_ = testcontainers.TerminateContainer(pg) // os.Exit skips deferred calls
	os.Exit(code)
}
```

- Java: `@Testcontainers` with `@Container @ServiceConnection static
  PostgreSQLContainer postgres = new PostgreSQLContainer("postgres:17-alpine");`
  in Spring Boot (the class is in `org.testcontainers.postgresql` from
  Testcontainers 2.0, in `org.testcontainers.containers` before).
- .NET: `new PostgreSqlBuilder("postgres:17-alpine").Build()` started in a
  class fixture, the app through `WebApplicationFactory<Program>`, Respawn to
  reset data between tests.
- Rust: `#[sqlx::test]` gives each test a fresh database from `DATABASE_URL`
  with `migrations/` applied; `testcontainers-modules` provides the server.
- Container reuse (`testcontainers.reuse.enable=true`) is for local runs only.

## 3. Isolating tests from each other

| Strategy | Good | Costs |
|---|---|---|
| Transaction per test, rolled back | fastest; nothing to clean | the app must use the test's connection, not its own pool; commit-time behaviour never runs; code with its own transactions needs savepoints; no concurrency tests |
| Truncate between tests | real commits, pools and threads work | slower as tables grow; parallel workers need separate databases |
| Template database cloned per test or worker | real commits, parallel-safe, fresh state | PostgreSQL-specific; nothing may be connected to the template while cloning |
| Schema per worker | one database, parallel workers | migrations must not hard-code the schema |

- Use the framework's own first: Django `TestCase` (transaction) and
  `TransactionTestCase` (truncation), pytest-django
  `django_db(transaction=True)`, Rails transactional tests, Laravel
  `RefreshDatabase` and `DatabaseTruncation`, Spring's `@Transactional`
  tests, Respawn in .NET.
- Transaction-per-test traps: `on_commit` hooks never fire (Django:
  `captureOnCommitCallbacks(execute=True)`; pytest-django:
  `django_capture_on_commit_callbacks`); PostgreSQL's `now()` is the
  transaction's start time, so it is the same for the whole test; a Spring
  `@Transactional` test with `webEnvironment = RANDOM_PORT` does not roll
  back what the server thread committed.
- Truncation in one statement, with the table list read once from
  `pg_tables` (minus the migrations table):
  `TRUNCATE orders, order_items, customers RESTART IDENTITY CASCADE;`
- Templates: pgtestdb (Go) and `#[sqlx::test]` (Rust) create a database per
  test from a migrated template; Testcontainers' PostgreSQL modules can
  `snapshot()` and `restoreSnapshot()` (Node) or `Snapshot` and `Restore`
  (Go) the migrated state.
- Parallel workers each get their own database or schema: pytest-xdist's
  `worker_id` fixture (`gw0`, `gw1`, or `master`), Laravel's
  `--parallel` databases suffixed per process. `go test ./...` runs
  packages as parallel processes, so each package's `TestMain` needs its own
  database or container.
- Seeding: reference data (roles, plans) once, with the migrations; the rest
  per test from factories with unique values (`testing-data`).

## 4. Other services, faked at the network edge

- Never call a real third-party API from the suite: rate limits, cost,
  flakiness and secrets in CI. Fake it at HTTP, so the real client code
  (serialisation, headers, retries, timeouts) runs.
- Tools: MSW's `setupServer` (fail on unhandled requests with
  `onUnhandledRequest: "error"` in MSW 2, `onUnhandledFrame: "error"` in
  MSW 3), nock, undici `MockAgent`; respx for httpx, responses for requests,
  pytest-httpserver; `httptest.NewServer` in Go; WireMock or MockWebServer on
  the JVM; WireMock.Net; the `wiremock` crate; Laravel `Http::fake()` with
  `Http::preventStrayRequests()`.
- Block real network access so a forgotten fake fails loudly: MSW's
  unhandled setting, `pytest --disable-socket --allow-hosts=127.0.0.1,localhost`
  (pytest-socket), `WebMock.disable_net_connect!(allow_localhost: true)`.
- Fake the failures too: a timeout, a reset connection, `429` with
  `Retry-After`, `500`, malformed JSON, a reply slower than the client's
  timeout. Check the retry count and that nothing was half-written.
- Cloud APIs: LocalStack for AWS, Azurite for Azure Storage, the Google
  Cloud emulators (Pub/Sub, Firestore, Spanner), DynamoDB Local, the
  Firebase Emulator Suite, all as containers.
- Recorded cassettes (pytest-recording, VCR, Polly.JS) only with credentials
  filtered out; hand-written fakes read better for failure cases.

```python
@respx.mock
def test_payment_stays_pending_when_the_provider_times_out(client, order):
    respx.post("https://payments.example.test/v1/charges").mock(side_effect=httpx.ConnectTimeout)

    res = client.post(f"/orders/{order.id}/pay")

    assert res.status_code == 503
    assert fetch_order(order.id).status == "pending"
```

## 5. Contracts between services

- When a consumer and a provider deploy separately, a contract test checks
  the provider still gives what the consumer uses, without a shared
  environment.
- Consumer-driven with Pact: the consumer's tests run against a Pact mock
  server and write a pact file; the provider replays it against the real
  service with provider states set up; a broker (Pact Broker or PactFlow)
  stores pacts and results, and `can-i-deploy` gates each deploy.

```ts
import { PactV4, MatchersV3 } from "@pact-foundation/pact";

const pact = new PactV4({ consumer: "web", provider: "orders-api" });

it("reads an order", () =>
  pact
    .addInteraction()
    .given("order ord_1 exists")
    .uponReceiving("a request for order ord_1")
    .withRequest("GET", "/orders/ord_1")
    .willRespondWith(200, (res) =>
      res.jsonBody({ id: "ord_1", status: MatchersV3.regex("paid|pending", "paid"), total: MatchersV3.integer(1500) }),
    )
    .executeTest(async (server) => {
      const order = await new OrdersClient(server.url).get("ord_1");
      expect(order.status).toBe("paid");
    }));
```

- Schema-based, when one team owns both sides or the API is public: generate
  OpenAPI from the code, generate or check the client from it, catch
  breaking changes with `oasdiff breaking base.yaml head.yaml`, and let
  Schemathesis generate requests from the schema:
  `schemathesis run http://localhost:8000/openapi.json --checks all`.

## 6. Migrations

- CI runs every migration from an empty database before the tests (the
  setup above does).
- Reversible migrations go up, down and up again on a copy
  (`alembic upgrade head && alembic downgrade -1 && alembic upgrade head`,
  `bin/rails db:migrate:redo`).
- Models and migrations agree: `alembic check`,
  `python manage.py makemigrations --check --dry-run`, or the ORM's
  equivalent, in CI.
- Data migrations run on generated data of production's shape (never copied
  personal data), asserting counts and a few transformed rows.
- Locks: lint with squawk (PostgreSQL) or strong_migrations (Rails); time a
  large-table rewrite against generated volume.

## 7. Time, background work and concurrency

- Inject a clock into the app and freeze it in the test (`testing-data`).
  The database has its own clock: set timestamps explicitly in inserted rows
  when a test depends on them.
- Background work runs inline or is drained, then its effect is asserted:
  Rails `perform_enqueued_jobs`, `Sidekiq::Testing.inline!`, Laravel
  `Queue::fake()` with `Queue::assertPushed()` (or the `sync` connection),
  Celery `task.apply()`, River's `rivertest.RequireInserted`, a BullMQ
  processor called with the job's data. More in `backend-jobs`.
- Wait for eventual effects by polling with a timeout (Awaitility,
  `vi.waitFor`, testify `assert.Eventually`), never a `sleep`.
- Unique keys per worker for everything shared: Redis prefixes, queue
  names, bucket prefixes. Never hard-code a port.
- Races are tested with real concurrency against the real database (not
  inside a rolled-back transaction):

```ts
it("sells the last item once when two orders race", async () => {
  const product = await createProduct({ stock: 1 });

  const results = await Promise.all([placeOrder(product.id), placeOrder(product.id)]);

  expect(results.map((r) => r.status).sort()).toEqual([201, 409]);
  expect((await getProduct(product.id)).stock).toBe(0);
});
```

## 8. Keeping it fast

- Containers once per run, migrations once (into a template or snapshot), a
  cheap reset per test; a reset costing more than the test is next to fix.
- Aim for one service's suite in a few minutes locally; past about 10, shard
  in CI (`testing-ci`) and hunt per-test containers, re-migrations, sleeps.
- Find the slow ones: `pytest --durations=10`, `gotestsum tool slowest`,
  nextest's `SLOW` lines.

## Check it

- Run in random order and in parallel (`pytest -p randomly -n auto`,
  `vitest run --sequence.shuffle`, `go test -shuffle=on -p 4 ./...`): green.
- Break the SQL (a wrong join condition) or drop a constraint: a test fails.
- With the network blocked, nothing reaches the internet.
- `docker ps` after the run shows no leftover containers.

## Avoid

SQLite or H2 standing in for the production database; mocks of the ORM or
query builder; one shared database for all tests and developers; a
container per test; hard-coded ports; tests that depend on order; calling
real providers from CI; transaction-per-test when the code commits, uses its
own pool or relies on `on_commit`; `sleep` to wait for a job.
