---
name: database-transactions
description: "Transactions and concurrency: what isolation levels really guarantee, lost updates and write skew, locking reads, SKIP LOCKED queues, optimistic concurrency with versions, conditional updates, deadlocks and retries, keeping transactions short, idempotency and the outbox. Read before writing code where two requests can change the same data."
---

# Transactions and concurrency

The generated version reads a row, decides in code and writes it back,
inside a transaction it believes makes this safe. Under the default
isolation of Postgres or MySQL it does not: two requests read a stock of 1
and both sell it. Or it wraps the whole request, a payment call included, in
one transaction that holds row locks for two seconds, and deadlocks under
load with no retry. This skill is what isolation really guarantees and the
patterns that make concurrent writes correct. The short rules are in
`backend-data`; queues and the outbox dispatcher in `backend-jobs`.

## 1. What isolation levels guarantee

| Level | Postgres | MySQL (InnoDB) |
|---|---|---|
| Read committed | The default. Each statement sees what was committed before it began; an `UPDATE ... WHERE` waits for a locked row, then re-checks its condition on the new version | Available; takes fewer gap locks |
| Repeatable read | One snapshot per transaction; updating a row another transaction changed since fails with `40001` | The default. Plain `SELECT` reads the snapshot, but `UPDATE`, `DELETE` and locking reads act on the latest row, so read-then-write still loses updates |
| Serializable | SSI: an interleaving that could not happen one after the other fails one transaction with `40001` | Plain `SELECT`s take shared locks (outside autocommit); conflicts become waits and deadlocks |

- A transaction gives all-or-nothing, not mutual exclusion. Neither default
  level stops a read, decide in code, write sequence from losing an update.
- SQLite has one writer at a time, so it is serializable, but a deferred
  transaction that reads and then writes can fail with `SQLITE_BUSY`
  (`database-sqlite`). MongoDB transactions are snapshots whose write
  conflicts abort with a retryable label (`database-mongodb`).
- Choose the level per transaction when needed: `BEGIN ISOLATION LEVEL
  SERIALIZABLE;` (Postgres), `SET TRANSACTION ISOLATION LEVEL SERIALIZABLE;`
  before `START TRANSACTION` (MySQL), or the ORM's option (Prisma
  `isolationLevel: Prisma.TransactionIsolationLevel.Serializable`).

## 2. The anomalies, concretely

- **Lost update**: two requests read `balance = 100`, each subtracts 30 in
  code and writes 70. One withdrawal vanished.
- **Write skew**: two doctors are on call; each checks that the other is
  still on call and goes off. Both checks were true, the writes touched
  different rows, nobody is on call. Only serializable, or a lock on
  something both touch, prevents it.
- **Phantom**: "no booking overlaps, so insert" in two transactions; neither
  sees the other's new row. An exclusion or unique constraint, or a lock on
  the parent row, prevents it.
- **Check then insert**: "no user with this email, so insert" races the same
  way. The unique constraint is the only reliable judge.

## 3. Patterns, simplest first

1. **One conditional statement**, when the rule fits in a `WHERE`:

```sql
UPDATE accounts SET balance_cents = balance_cents - $2
WHERE id = $1 AND balance_cents >= $2
RETURNING balance_cents;
```

   No row back means the rule failed: roll back and answer 409. Safe under
   read committed: the second update waits for the row lock, then re-checks.
2. **A unique constraint as the judge** of "only one": insert and handle the
   violation (Postgres `23505`, MySQL `1062`, SQLite
   `SQLITE_CONSTRAINT_UNIQUE`), or `INSERT ... ON CONFLICT DO NOTHING
   RETURNING id` where no row means it exists. In Postgres a failed
   statement aborts the transaction (`25P02` for all that follows): catch it
   inside a savepoint, or avoid it with `ON CONFLICT`.
3. **A locking read**, when code must decide between read and write:

```sql
BEGIN;
SELECT status, total_cents FROM orders WHERE id = $1 FOR UPDATE;  -- others wait here
-- decide in code, quickly, with no network calls
UPDATE orders SET status = 'refunded', refunded_at = now() WHERE id = $1;
COMMIT;
```

   `FOR NO KEY UPDATE` (Postgres) suffices when the key does not change and
   does not block inserts of child rows. `NOWAIT` fails at once instead of
   waiting. To guard a rule over many rows, lock their parent (the shift,
   then check and change the doctors on it).
4. **Optimistic concurrency** for edits by people, whose think time no lock
   can span:

```sql
UPDATE documents SET body = $1, version = version + 1
WHERE id = $2 AND version = $3;    -- $3: the version the editor loaded
```

   Zero rows means someone saved first: answer 409 with the current version
   so the client can merge or reload (`backend-api`). Built in as Rails
   `lock_version`, JPA `@Version`, EF Core concurrency tokens; by hand
   elsewhere (Prisma `updateMany` with `version` in `where`, then `count`).
5. **Serializable with retry** for invariants across rows that no statement
   or lock expresses. Every transaction at that level is retried on `40001`.
6. **Advisory locks** for "one at a time" (a nightly job, a per-customer
   sync): `SELECT pg_try_advisory_xact_lock(hashtext('sync:' || $1));` inside
   a transaction, released at commit, safe behind transaction poolers. The
   session form `pg_advisory_lock` must be released by hand and breaks behind
   PgBouncer in transaction mode. MySQL: `GET_LOCK('sync:42', 0)`,
   `RELEASE_LOCK('sync:42')`.

## 4. Queues with SKIP LOCKED

Many workers each take different rows without waiting on one another
(Postgres 9.5+, MySQL 8.0+):

```sql
WITH next AS (
  SELECT id FROM jobs
  WHERE status = 'queued' AND run_at <= now()
  ORDER BY run_at, id
  LIMIT 10
  FOR UPDATE SKIP LOCKED
)
UPDATE jobs j
SET status = 'running', locked_until = now() + interval '5 minutes',
    attempts = attempts + 1
FROM next WHERE j.id = next.id
RETURNING j.*;
```

- Commit at once, work outside the transaction, then mark `done` or
  `failed`. A job whose `locked_until` passed is taken again (the lease), so
  handlers are idempotent.
- A partial index `ON jobs (run_at) WHERE status = 'queued'` keeps the pick
  fast; archive or delete finished rows, since dead ones slow the scan.
- Before writing your own, use a library that does this: pg-boss or
  graphile-worker (Node), Procrastinate (Python), River (Go), Oban (Elixir),
  Solid Queue or GoodJob (Rails).

## 5. Deadlocks and retries

- Two transactions each wait for a lock the other holds. Postgres notices
  after `deadlock_timeout` (1s) and aborts one with `40P01`; InnoDB notices
  at once and rolls back the smaller one with error `1213`.
- Prevent most of them: lock rows in one order (sort ids first:
  `SELECT ... WHERE id = ANY($1) ORDER BY id FOR UPDATE`), keep transactions
  short, touch fewer rows, and in MySQL index the `WHERE` of every update,
  since InnoDB locks each row it scans, not only those it changes.
- Retry the whole transaction, never the failed statement, on `40001` and
  `40P01` (Postgres), `1213` and `1205` (MySQL lock wait timeout), and
  `SQLITE_BUSY`; Prisma reports them as `P2034`. Three to five attempts,
  exponential backoff with jitter:

```ts
const RETRYABLE = new Set(["40001", "40P01"]); // SQLSTATE in err.code (node-postgres, postgres.js)

export async function withRetry<T>(run: () => Promise<T>, attempts = 5): Promise<T> {
  for (let attempt = 1; ; attempt++) {
    try {
      return await run();
    } catch (err) {
      const code = (err as { code?: string }).code;
      if (attempt >= attempts || !code || !RETRYABLE.has(code)) throw err;
      const delay = Math.min(1000, 20 * 2 ** attempt) * (0.5 + Math.random());
      await new Promise((resolve) => setTimeout(resolve, delay));
    }
  }
}
// await withRetry(() => db.transaction(async (tx) => { /* reads and writes */ }));
```

- What is retried must be safe to run twice: nothing inside sends an email,
  charges a card or calls another service.

## 6. Short transactions

- Inside: the reads and writes that belong together, for milliseconds.
  Outside: HTTP calls, payment providers, uploads, email, a user's decision,
  heavy computation. Do the slow part before (validate, compute, call the
  provider with an idempotency key) or after the commit (a job).
- Guards on the app's role, so a forgotten transaction cannot hold locks
  and block vacuum for hours:

```sql
ALTER ROLE app SET idle_in_transaction_session_timeout = '60s';
ALTER ROLE app SET statement_timeout = '15s';
ALTER ROLE app SET lock_timeout = '10s';
```

- Request-wide transactions (Django `ATOMIC_REQUESTS`, a middleware around
  every request) hold locks through the slow parts and run side effects
  before the commit. Prefer explicit transactions around the writes, and
  side effects after the commit: Django `transaction.on_commit()`, Rails
  `after_commit`, Laravel `DB::afterCommit()`.
- Nested transactions are savepoints: Django nested `atomic()`, SQLAlchemy
  `begin_nested()`, Rails only with `transaction(requires_new: true)` (without
  it the inner block joins the outer, and an inner `ActiveRecord::Rollback`
  is silently ignored). Prisma interactive transactions do not nest and
  default to a 2s wait for a connection and a 5s timeout.

## 7. Idempotency

A request that creates something (an order, a payment, a transfer) carries
an `Idempotency-Key` (`backend-api`), stored under a unique key:

```sql
CREATE TABLE idempotency_keys (
  user_id      bigint NOT NULL REFERENCES users (id),
  key          text NOT NULL,
  request_hash text NOT NULL,
  response     jsonb,
  created_at   timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (user_id, key)
);
```

- In the same transaction as the work: `INSERT ... ON CONFLICT DO NOTHING
  RETURNING key`. A row back: do the work and store the response. No row:
  the key exists, so return its stored response (a concurrent duplicate
  waits on the unique index until the first commits). The same key with a
  different `request_hash` is a 422.
- When the work calls a provider outside the transaction, commit the key
  first as started, pass the same key to the provider, and answer a
  duplicate with 409 until it is done. Purge keys after 24 hours.

## 8. The outbox, and replicas

- An event published if and only if the change commits goes into an
  `outbox` table in the same transaction; a dispatcher takes committed rows
  with `FOR UPDATE SKIP LOCKED`, publishes, marks them sent. Delivery is at
  least once, so consumers dedupe by event id (`backend-jobs`). Change data
  capture (Debezium on the WAL or binlog) is the heavier alternative.
- Replicas lag, from milliseconds to minutes under load. After a user
  writes, read from the primary for that user for a few seconds, or wait
  until the replica has replayed the write (Postgres: `pg_current_wal_lsn()`
  on the primary after commit, `pg_last_wal_replay_lsn()` on the replica).
  Never decide a write from a replica's read.

## Check it

- A concurrency test per race: the same request 10 to 20 times at once
  (`Promise.all`, a thread pool, `xargs -P`) against a row with room for one
  success. Assert one success, the rest 409, and the final state (stock 0,
  one order, a balance never below zero).
- Each concurrent request has its own connection and really commits; tests
  inside one rolled-back transaction cannot show a race.
- Force the retry path once (two transactions updating the same two rows in
  opposite orders) and see the retry succeed.
- In production: count `40001`, `40P01` and lock timeouts in the logs, and
  look for long-open transactions:

```sql
SELECT pid, now() - xact_start AS open_for, state, left(query, 80) AS query
FROM pg_stat_activity
WHERE xact_start IS NOT NULL
ORDER BY open_for DESC LIMIT 10;
```

## Avoid

Read, decide in code, write; trusting a transaction alone to prevent races;
`max(n) + 1`; a SELECT to check uniqueness before the INSERT; serializable
without a retry loop; retrying one statement instead of the transaction;
side effects inside a transaction that may be retried; HTTP calls or user
waits inside a transaction; locks taken in different orders; session
advisory locks behind a transaction pooler; request-wide transactions around
slow work; a replica's read deciding a write; concurrency tests on one
connection.
