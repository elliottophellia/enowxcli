---
name: backend-data
description: "Data access from the application: migrations, column types for money, time and ids, constraints, transactions, race-free writes (stock, balances, unique values), queries without N+1, pagination and indexes, seeds, connection pools, SQLite settings. Read before writing queries, models or migrations."
---

# Data

One part of the backend; the whole is in the `backend` skill. Schema design
in depth is the database specialist's; these are the rules every piece of
server code that touches data keeps.

## 1. Migrations

- Every change to the schema is a migration, made with the project's tool
  (Drizzle Kit, Prisma Migrate, Alembic, Laravel, sqlx, goose or atlas), and
  committed. Never tables created by hand, never an ORM's "sync the schema"
  against a real database.
- One migration per change, named for what it does
  (`add_status_to_orders`). A migration that has run anywhere is never
  edited; a fix is a new one.
- Reversible where it can be. A change that drops or rewrites data says so
  in the report before it runs; renaming a column in use is three steps (add,
  copy and write both, drop later).

## 2. Columns

- Money: an integer in the smallest unit (`1500000` rupiah, `1999` cents)
  with its currency, or `numeric(12, 2)`; never `float` or `double`.
- Time: `timestamptz` (or UTC in a text or integer column in SQLite), always
  UTC in storage, converted to the user's zone only when shown. Dates without
  a time (a birthday, a due date) as `date`.
- Ids: `bigint` identity, or UUIDv7 when ids are made outside the database or
  must not be guessable in URLs. Never an id built from a count.
- `NOT NULL` unless empty has a meaning; defaults in the database
  (`created_at default now()`); a `CHECK` for rules the data must always keep
  (`stock >= 0`, `status in (...)`).
- Foreign keys for every reference, with `ON DELETE` chosen on purpose
  (`restrict` for records with history such as orders, `cascade` for parts
  that mean nothing alone), and an index on the referencing column.
- Uniqueness the business depends on is a unique constraint (a lowercased
  email, a SKU per shop), not a check in code.

## 3. Transactions and races

- Writes that belong together run in one transaction: an order and its
  items, a payment and the balance it changes. On error the whole of it rolls
  back.
- Never read, decide in code, then write, when two requests can interleave.
  Let the database decide:
  - stock and balances with a conditional update, then check the rows
    affected:
    ```sql
    UPDATE products SET stock = stock - $2
    WHERE id = $1 AND stock >= $2
    ```
    zero rows means not enough stock: roll back and answer `409`;
  - "create if it does not exist" with the unique constraint and an upsert
    (`INSERT ... ON CONFLICT`), and a duplicate turned into `409`;
  - an edit two people can make at once with a `version` column checked in
    the `WHERE` of the update;
  - a sequence number (an invoice number per shop) from the database, never
    `max(n) + 1` in code.
- Keep transactions short: no calls to other services, no waiting on a user,
  inside one. Send the email after the commit (a job, `backend-jobs`).

## 4. Queries

- Always parameterised: the query layer's placeholders or builder; never
  values or column names pasted into a string. Sort and filter columns come
  from an allowed list.
- Select the columns you need. Related rows in one query (a join, `IN`, the
  ORM's eager load: `with`, `include`, `selectinload`, `prefetch_related`),
  never one query per row in a loop.
- Lists paginated in the query with a limit, by keyset on an indexed sort
  column and the id for long lists (`WHERE (created_at, id) < ($1, $2)
  ORDER BY created_at DESC, id DESC LIMIT 25`), by offset only for short
  lists or page numbers.
- Counts, sums and reports computed in SQL with `GROUP BY`, never by
  loading rows to add them up in code. Heavy ones cached or kept in a summary
  table that is updated when the data changes.
- An index for each filter and sort that runs on every request, composite in
  the order the query uses; `EXPLAIN` a query that is slow before adding one.

## 5. Deleting and history

- Delete for real unless the domain needs history or undo; then a
  `deleted_at` column, and every query and unique constraint made aware of
  it.
- Anything about money keeps an append-only record (the ledger, the stock
  movements) beside the current value, so the current value can be checked
  and rebuilt.

## 6. Seeds and test data

- A seed script, separate from the migrations and safe to run twice, with
  plausible sample data for development that is clearly sample
  (`Sample product 1`, `owner@example.com`), in the product's language; not
  invented real brands or people. Never run against production.

## 7. Connections

- One client or pool for the process, created at start; never a connection
  per request. On serverless platforms, the provider's pooler (PgBouncer, the
  Neon or Supabase pooler) and a small pool per instance.
- In development with hot reload, keep the client on a global so reloads do
  not open new pools (`backend-stack-next`).
- SQLite: `PRAGMA journal_mode = WAL`, `PRAGMA busy_timeout = 5000`,
  `PRAGMA foreign_keys = ON` on every connection, one writer at a time, and
  the file outside the web root and inside the backup.

## Avoid

Floats for money; times without a zone; a table created by hand; an edited
migration that already ran; stock or a balance updated with a read, a sum in
code and a write; `max(id) + 1`; a query in a loop; `SELECT *` sent to the
client; values or sort columns concatenated into SQL; a list with no limit;
an email sent inside a transaction; a pool per request.
