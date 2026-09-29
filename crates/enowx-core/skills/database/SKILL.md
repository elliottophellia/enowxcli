---
name: database
description: "Designing and changing databases: modelling data so it stays true, constraints, migrations safe to run on live data, queries and indexes, transactions and concurrency, each engine's specifics, and running it. The principles, the defaults, and which skill holds each. Read before any schema, migration or query work."
---

# Databases

The generated version: no constraints because the app validates, prices in
floats, timestamps without a time zone, a migration that rewrites a large
table at noon and blocks every write while it runs, `SELECT *` inside a loop,
an index on every column and none on the one the slow query filters, backups
that have never been restored. This skill is the frame the rest hangs on: how
to read the project, the principles, the defaults, when each engine fits, and
which skill holds the detail. Data access from server code (money, time, ids,
race-free writes) is summarised in `backend-data`; this family goes deeper.

## 1. Read the project before changing anything

- **Engine and version**: `SELECT version();` (Postgres),
  `SELECT VERSION();` (MySQL), `select sqlite_version();`, `db.version()`
  (MongoDB), the image tag in `docker-compose.yml`, the provider in the
  infrastructure code. Which migrations are safe, and which features exist,
  depend on the version.
- **Access layer**: the ORM or query builder and where the models live
  (`schema.prisma`, Drizzle schema files, SQLAlchemy models, Django
  `models.py`, ActiveRecord, Eloquent, sqlx, GORM, Ent, EF Core). Write in
  that layer; raw SQL where it cannot express the query, still parameterised.
- **Migration tool and history**: the folder, the naming, the last few
  migrations, SQL or code, and the current state (`prisma migrate status`,
  `alembic current`, `python manage.py showmigrations`,
  `bin/rails db:migrate:status`, `php artisan migrate:status`,
  `sqlx migrate info`, `goose status`, `flyway info`).
- **Conventions in the schema**: plural or singular tables, the id type,
  timestamp columns, soft deletes, the tenant column, how constraints and
  indexes are named. Follow them even where this family would choose
  otherwise, unless the task is to change them.
- **Seeds, fixtures and tests**: where seed data lives, how tests get a
  database (a container, a template file, a transaction rolled back per
  test), and the connection config (`DATABASE_URL`, pool size, SSL mode, a
  pooler in front).
- **The load**, when you have access: table sizes and the hot queries
  (`pg_stat_user_tables`, `pg_stat_statements`, the MySQL slow log). A
  migration that is instant on 1,000 rows can block writes on 50 million.

## 2. Principles

1. **The database enforces what must always be true.** NOT NULL, foreign
   keys, UNIQUE and CHECK hold for every writer: two app versions during a
   deploy, a script, a backfill, an admin console, next year's service.
   Validation in the app gives friendly messages; the constraint keeps the
   data true.
2. **Model the domain, then test the model against its queries.** Tables
   come from the things the business names and their lifecycle; the list of
   screens, reports and jobs says whether the shape answers them cheaply.
3. **Migrations are code**: versioned, reviewed, one concern each,
   reversible or saying why not, tried on a copy first, never edited once
   applied anywhere shared.
4. **Measure before tuning.** `EXPLAIN (ANALYZE, BUFFERS)` (or the engine's
   equivalent) before and after an index or a rewrite. An index no plan uses
   is only a cost on every write.
5. **Short transactions**: locks held for milliseconds, never across a
   network call or a user's think time.
6. **Least privilege**: the app's role reads and writes rows and cannot
   alter the schema; migrations run as the owner; no app runs as superuser.
7. **A backup exists once a restore from it has been done and timed.**

## 3. Choosing an engine (only when the project has none)

| Situation | Choose |
|---|---|
| A new web app or service; relations, money, reporting | PostgreSQL 17 or 18 |
| Local-first, desktop, mobile, CLI, embedded, a small single-server app | SQLite |
| The team or host already runs MySQL; Vitess or PlanetScale scale-out | MySQL 8.4 LTS or MariaDB |
| Documents read and written whole, shape varies per record, an Atlas estate | MongoDB |
| Cache, rate limits, sessions, leaderboards | Redis or Valkey beside the main database |
| Analytics over billions of rows | ClickHouse, DuckDB or a warehouse, fed from the primary |

- PostgreSQL by default: strict types, transactional DDL, many index kinds,
  `jsonb` when a shape truly varies, extensions (full text, trigrams,
  vectors, geography), and every host offers it.
- Do not add a second database for what Postgres covers at the project's
  size: `jsonb` before a document store, full text and `pg_trgm` before a
  search engine, pgvector before a vector database up to a few million
  vectors, a `SKIP LOCKED` table before a message broker.
- A cache is never the source of truth: it can be dropped and rebuilt.

## 4. Defaults (the detail is in the skill named)

- Primary keys `bigint generated always as identity`; UUIDv7 when ids are
  made outside the database or appear in URLs (`database-schema`).
- Money as integer minor units beside a currency code, `numeric` for rates;
  instants as `timestamptz` in UTC; calendar dates as `date`.
- NOT NULL unless empty means something; every reference a foreign key with
  a chosen ON DELETE, and an index on the referencing column.
- snake_case names; every constraint and index named.
- Live migrations by expand and contract, with `lock_timeout` set, indexes
  built concurrently, backfills in batches outside the migration
  (`database-migrations`).
- Queries parameterised, only the needed columns, keyset pagination for long
  lists, no query inside a loop (`database-queries`).
- Races settled by conditional updates, unique constraints or row locks,
  never by reading, deciding in code, then writing (`database-transactions`).
- Postgres with `pg_stat_statements` on, `statement_timeout` and
  `idle_in_transaction_session_timeout` on the app role, a pooler when
  connections are many (`database-postgres`).
- SQLite with WAL, `busy_timeout`, `foreign_keys=ON` and STRICT tables
  (`database-sqlite`).
- Automated, encrypted, off-site backups with a restore tested on a schedule
  (`database-operations`).

## 5. How a change is made

1. Write down the change and the queries it must serve.
2. Read the real schema from the database (`\d+ orders` in psql,
   `SHOW CREATE TABLE orders`, `.schema orders` in sqlite3), not only the
   model files, which drift.
3. Generate the migration with the project's tool and read the SQL it will
   run (`prisma migrate dev --create-only`, `python manage.py sqlmigrate app
   0042`, `alembic upgrade head --sql`). Fix what the generator got wrong: a
   rename done as drop and add, a missing `CONCURRENTLY`, a default that
   rewrites the table.
4. Classify it: additive and instant, additive but slow (an index, a
   validation, a backfill), or destructive (drop, type change, rename). Slow
   and destructive ones follow `database-migrations`.
5. Change the models, queries, seeds and factories that touch it in the same
   change.
6. Run it up, down and up on a scratch database; run the tests; `EXPLAIN`
   the queries it was for.
7. Report what changed, what it locks and for how long on production-sized
   data where known, and anything destructive, before it runs anywhere
   shared.

## 6. Definition of done

- The migration applies to an empty database and to one at the previous
  version, and rolls back, or the report says why it cannot.
- Each new constraint refuses a bad row with the expected error (Postgres:
  `23502` not null, `23503` foreign key, `23505` unique, `23514` check,
  `23P01` exclusion).
- The affected queries return the right rows, edge cases included (NULLs,
  empty results, both ends of a date range), with a plan that uses the
  intended index at realistic sizes.
- Seeds and factories are updated and a seed still runs twice cleanly.
- Nothing dropped, truncated or rewritten without saying so, and a backup
  taken before anything destructive.
- Tests pass against the same engine and major version as production.

## 7. Which skill for which job

| Job | Skill |
|---|---|
| Principles, defaults, choosing an engine, this map | `database` |
| Tables, keys, types, constraints, naming, soft deletes, tenants, trees, history | `database-schema` |
| Changing a live schema: tools, expand and contract, locks, backfills | `database-migrations` |
| Correct and fast SQL, N+1, pagination, EXPLAIN, ORM habits | `database-queries` |
| Which index, column order, partial, GIN, BRIN, unused ones, building live | `database-indexes` |
| Isolation, lost updates, row locks, SKIP LOCKED, retries, idempotency | `database-transactions` |
| PostgreSQL types, extensions, RLS, vacuum, pooling, roles, config | `database-postgres` |
| MySQL and MariaDB: InnoDB, collations, online DDL, gap locks, replicas | `database-mysql` |
| SQLite: pragmas, one writer, STRICT, backups, Litestream, tests | `database-sqlite` |
| MongoDB: documents by access pattern, ESR indexes, pipelines, concerns | `database-mongodb` |
| Backups and restores, replicas, monitoring, upgrades, security, retention | `database-operations` |
| The short version for server code | `backend-data` |

Read the ones for the parts you touch, when you come to them.

## Check it

- The migration folder has one new file per change and no edits to applied
  ones (`git diff --stat` on it).
- On a scratch database: apply, roll back, apply; a schema dump afterwards
  equals the old one plus the change (`pg_dump --schema-only`,
  `mysqldump --no-data`, `sqlite3 app.db .schema`).
- Each new constraint tried with a row that breaks it; each new or changed
  query run with `EXPLAIN (ANALYZE, BUFFERS)` against realistic row counts.
- The test suite green against the real engine; the seed run twice.
- The report says plainly what could not be run: no production-sized copy,
  no access to the managed instance, no restore performed.

## Avoid

Rules kept only in application code; floats for money; `timestamp` without a
zone; `varchar(255)` on every column by habit; editing a migration that
already ran; `ALTER TABLE` on a large table with no `lock_timeout`; an index
no `EXPLAIN` asked for; a transaction held open across an HTTP call; the app
connecting as superuser; a second database for what Postgres already does;
production data copied to a laptop; a backup that was never restored.
