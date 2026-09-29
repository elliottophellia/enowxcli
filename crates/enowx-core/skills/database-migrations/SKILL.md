---
name: database-migrations
description: "Changing a live database safely: the project's migration tool, migrations that never get edited once applied, expand and contract for zero downtime, locks to avoid, backfills in batches, data migrations apart from schema changes, testing on a copy, and ordering with deploys. Read before writing any migration."
---

# Migrations on live data

The generated migration: made by the ORM and committed unread, renaming a
column the running code still reads, adding a NOT NULL column and a foreign
key in one step on a table of 40 million rows, building an index without
`CONCURRENTLY` at noon, and a backfill that updates every row in one
transaction. On an empty development database each of these is instant,
which is why they ship; on production each is a lock or an outage. This
skill is how a schema change reaches a live database without stopping it.
What the schema should be is `database-schema`; the short rules for server
code are in `backend-data`.

## 1. The tool, by stack

Use the project's tool. Never tables made by hand, never a schema sync
(`prisma db push`, `drizzle-kit push`, TypeORM `synchronize: true`,
Sequelize `sync({ alter: true })`) against a database anyone shares.

| Stack | Create | Apply | Status, undo |
|---|---|---|---|
| Prisma | `prisma migrate dev --create-only --name add_status` | `prisma migrate deploy` | `prisma migrate status`; no down files |
| Drizzle Kit | `drizzle-kit generate` | `drizzle-kit migrate` | `drizzle-kit check` |
| Alembic | `alembic revision --autogenerate -m "add status"` | `alembic upgrade head` | `alembic current`, `alembic downgrade -1` |
| Django | `python manage.py makemigrations` | `python manage.py migrate` | `showmigrations`, `migrate app 0041` |
| Rails | `bin/rails g migration AddStatusToOrders status:string` | `bin/rails db:migrate` | `db:migrate:status`, `db:rollback` |
| Laravel | `php artisan make:migration add_status_to_orders_table` | `php artisan migrate --force` | `migrate:status`, `migrate:rollback` |
| sqlx | `sqlx migrate add -r add_status` | `sqlx migrate run` | `sqlx migrate info`, `sqlx migrate revert` |
| golang-migrate | `migrate create -ext sql -dir db/migrations -seq add_status` | `migrate -path db/migrations -database "$DATABASE_URL" up` | `version`, `down 1`, `force N` after a failure |
| goose | `goose create add_status sql` | `goose up` | `goose status`, `goose down` |
| Atlas | `atlas migrate diff add_status` | `atlas migrate apply` | `atlas migrate status`, `atlas migrate lint` |
| Flyway | a `V7__add_status.sql` file | `flyway migrate` | `flyway info`, `flyway validate` |
| EF Core | `dotnet ef migrations add AddStatus` | `dotnet ef database update` or a script (`migrations script --idempotent`) | `dotnet ef migrations list` |

A generated migration is a draft. Read the SQL it will run
(`python manage.py sqlmigrate app 0042`, `alembic upgrade head --sql`,
`php artisan migrate --pretend`, the `.sql` file Prisma or Drizzle wrote) and
fix what generators get wrong: a rename seen as drop plus add (the data is
lost), a type change that rewrites the table, a missing `CONCURRENTLY`, an
index duplicating one that exists, a default set in the ORM but not in the
database.

## 2. Rules

- A migration that ran anywhere shared (staging, production, main) is never
  edited or deleted; a fix is a new migration. Flyway and sqlx refuse a
  changed checksum, Prisma's `migrate dev` flags it, other tools let
  environments silently diverge.
- One concern per migration, named for it (`add_status_to_orders`); schema
  and data changes apart.
- A down migration where the change can be undone; where it cannot (a
  dropped column's data), the down raises with the reason.
- Migrations run once per deploy as their own step (a release or pre-deploy
  command, a Kubernetes Job), not by every instance at boot. Prisma, Rails
  and Flyway take a lock so a second runner waits; Laravel needs
  `migrate --isolated`; Django takes none.
- Postgres and SQLite run DDL inside the migration's transaction and roll it
  back whole on error. MySQL commits each DDL statement on its own, so a
  failed migration of three statements leaves some applied: one DDL
  statement per migration there.

## 3. Order with deploys

During a rolling deploy old and new code run together on one schema, so
every migration must work with both versions.

| Change | Order |
|---|---|
| Additive: table, nullable column, index | Migrate, then deploy the code that uses it |
| Destructive: drop a column or table | Deploy code that no longer uses it, then drop in a later release |
| Rename, type change | Expand and contract over several releases (section 4) |
| A new NOT NULL or CHECK rule | Code writes valid values everywhere, backfill, then the constraint |

ORMs that cache the column list fail when a column vanishes under them: tell
the ORM first (Rails `self.ignored_columns += ["total"]`), deploy, then drop.

## 4. Expand and contract

Replacing `orders.total` (numeric) with `orders.total_cents` (bigint)
without downtime:

1. **Expand**: add `total_cents bigint`, nullable, no default. Instant.
2. **Write both**: deploy code that writes both columns on every insert and
   update (or a trigger that fills the new from the old).
3. **Backfill** the existing rows in batches (section 8).
4. **Constrain**: NOT NULL through a validated CHECK, indexes built
   concurrently (section 5).
5. **Read the new column**; deploy.
6. **Stop writing the old one**; deploy.
7. **Contract**: drop `total` in a later release, after a backup.

Each step is its own deploy and can stop or roll back on its own. A table
rename works the same way, or with a view under the old name during the
move.

## 5. Postgres locks

Most `ALTER TABLE` forms take an ACCESS EXCLUSIVE lock. Even an instant one
waits for every running query on the table, and while it waits every new
query queues behind it: a five-minute report plus an instant ALTER is a
five-minute outage. Every migration on a live table starts with:

```sql
SET lock_timeout = '5s';          -- give up instead of queueing traffic behind us
SET statement_timeout = '15min';  -- what the step needs, never unbounded
```

and the deploy retries a step that timed out, a few times, with a pause.

| Operation | What happens | Do instead |
|---|---|---|
| `ADD COLUMN`, nullable or with a constant default | Catalog only (PG 11+) | Fine, with `lock_timeout` |
| `ADD COLUMN ... DEFAULT gen_random_uuid()` (volatile) | Rewrites the table | Add nullable, backfill, then set the default |
| `SET NOT NULL` | Scans the table under the lock | `CHECK (c IS NOT NULL) NOT VALID`, `VALIDATE`, `SET NOT NULL` (PG 12+ skips the scan), drop the CHECK |
| `ADD FOREIGN KEY` | Checks every row, locking both tables | `ADD CONSTRAINT ... NOT VALID`, then `VALIDATE CONSTRAINT` |
| `ADD CHECK` | Scans under the lock | `NOT VALID`, then `VALIDATE CONSTRAINT` |
| `ADD UNIQUE` | Builds the index under the lock | `CREATE UNIQUE INDEX CONCURRENTLY`, then `ADD CONSTRAINT ... UNIQUE USING INDEX` |
| `CREATE INDEX` | Blocks writes for the whole build | `CREATE INDEX CONCURRENTLY`, outside a transaction |
| `ALTER COLUMN TYPE` | Rewrites table and indexes | Expand and contract; `varchar(n)` to a larger `n` or to `text` is catalog only |
| `RENAME` column or table | Instant, breaks running code | Expand and contract |
| `DROP COLUMN` | Instant | Only after the code stopped using it |
| `VACUUM FULL`, `CLUSTER` | Rewrite under the exclusive lock | `pg_repack` |

`VALIDATE CONSTRAINT` takes a SHARE UPDATE EXCLUSIVE lock, so reads and
writes carry on while it scans. Building concurrently in each tool:

- Rails: `disable_ddl_transaction!` with `add_index ..., algorithm: :concurrently`.
- Django: `AddIndexConcurrently` in a migration with `atomic = False`.
- Alembic: `op.create_index(..., postgresql_concurrently=True)` inside
  `with op.get_context().autocommit_block():`.
- SQL-file tools: a migration holding only that statement, marked to run
  outside a transaction (goose: `-- +goose NO TRANSACTION`).
- A failed concurrent build leaves an invalid index. Find it with
  `SELECT indexrelid::regclass FROM pg_index WHERE NOT indisvalid;`, drop it
  with `DROP INDEX CONCURRENTLY`, and build again.

## 6. MySQL

- Name the algorithm, so MySQL refuses instead of silently copying the
  table: `ALTER TABLE orders ADD COLUMN note TEXT, ALGORITHM=INSTANT;` and
  `ALTER TABLE orders ADD INDEX idx_orders_status (status), ALGORITHM=INPLACE, LOCK=NONE;`.
- `INSTANT` adds or drops a column at any position from 8.0.29, up to 64
  row versions per table; after that the table needs a rebuild.
- Metadata locks: DDL waits behind any open transaction that touched the
  table, and everything queues behind the DDL; `lock_wait_timeout` defaults
  to a year. Run `SET SESSION lock_wait_timeout = 5;` first.
- Type changes and anything else that copies: gh-ost (reads the binlog,
  needs `binlog_format=ROW`) or pt-online-schema-change (triggers) on large
  tables, or deploy requests on PlanetScale and Vitess (`database-mysql`).

## 7. SQLite

DDL is transactional, but `ALTER TABLE` only adds, renames (3.25+) and drops
(3.35+) columns. A type, constraint or foreign key change is the table
rebuild in `database-sqlite`. It locks the whole database while it runs,
usually for milliseconds.

## 8. Backfills

- Outside the schema migration: a script or job that can run for hours,
  pause and resume, not one migration transaction.
- By primary key range, each batch its own short transaction, idempotent (it
  touches only rows not yet done), progress logged with the last id:

```sql
-- $1 walks from min(id) to max(id) in steps of 5000
UPDATE orders
SET total_cents = round(total * 100)
WHERE id >= $1 AND id < $1 + 5000
  AND total_cents IS NULL;
```

- 1,000 to 10,000 rows per batch, sized so each takes well under a second,
  with 50 to 500ms between batches; watch replica lag and pause when it
  grows.
- Count before and after: the rows still NULL reach 0 before the NOT NULL
  step.
- Never one `UPDATE` over the whole table: it holds its row locks until the
  end, leaves a dead copy of every row in Postgres, and floods the replicas.

## 9. Data migrations

- A change to the data (splitting a name, moving settings into a table) is
  its own migration or script, apart from the schema change, with the count
  of rows expected to change and a check of the counts afterwards.
- Never the app's current models inside a migration: they change later and
  the old migration breaks. Use SQL, `apps.get_model()` in Django's
  `RunPython`, or a small model class defined inside the migration.
- Keep the old data until the new is verified.

## 10. Destructive changes

- Say so in the report before it runs: which table or column, how many
  rows, whether the down migration can bring them back (usually not).
- Back up what is dropped
  (`pg_dump -Fc -t orders_legacy -f orders_legacy.dump "$DATABASE_URL"`) and
  record where the file is.
- Keep the old column or table through one release after the code stopped
  using it, then drop it.

## 11. Testing a migration

- Up, down, up on a scratch database: it catches a down that does not work
  and an up that cannot run twice.
- In CI, from empty to head, plus a check that models and migrations agree:
  `python manage.py makemigrations --check --dry-run`, `alembic check`,
  `prisma migrate diff ... --exit-code`, `atlas migrate lint`.
- For a large table, on a restored and anonymised copy: time it, and watch
  its locks from a second session:

```sql
SELECT a.pid, l.mode, l.granted, a.wait_event_type, left(a.query, 60)
FROM pg_locks l JOIN pg_stat_activity a USING (pid)
WHERE l.relation = 'orders'::regclass;
```

- Linters that know the dangerous forms: strong_migrations (Rails), squawk
  (`squawk migrations/*.sql` for Postgres SQL), django-migration-linter,
  `atlas migrate lint`.

## Check it

- The new migration is the only change in the migrations folder.
- Up, down and up pass on a scratch database, and the tests pass after.
- The SQL was read: `lock_timeout` set; indexes built concurrently;
  constraints added `NOT VALID` then validated; no rename or type change of a
  live column in one step; no backfill inside.
- For a large table, the duration and locks measured on a copy are in the
  report; if no copy was available, the report says so.
- Destructive steps are named in the report with the backup's location.

## Avoid

Editing an applied migration; `db push` or `synchronize` against a shared
database; a generated migration committed unread; renaming a column the
running code reads; dropping before the code stopped reading; `CREATE INDEX`
without `CONCURRENTLY` on a live Postgres table; foreign keys and NOT NULL
validated under the lock; ALTER with no `lock_timeout`; a backfill of every
row in one transaction; data and schema changes in one migration; app models
imported into migrations; every instance migrating at boot; several DDL
statements in one MySQL migration.
