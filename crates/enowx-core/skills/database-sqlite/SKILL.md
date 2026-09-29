---
name: database-sqlite
description: "SQLite done right: when it is the right database, WAL mode and busy timeouts, one writer at a time, STRICT tables and foreign keys, type affinity traps, migrations, backups, replication with Litestream or libSQL, and using it in tests. Read when the project uses SQLite or is choosing it."
---

# SQLite

The generated SQLite: the default rollback journal, so every write blocks
every reader; no busy timeout, so the second concurrent request fails with
"database is locked"; foreign keys declared but never enforced because the
pragma is off; an `INTEGER` column quietly holding the text `'abc'`; dates
stored three ways; the file copied with `cp` while the app writes; and
SQLite standing in for Postgres in tests. Set up right, SQLite is a serious
database for the right job. The short rules for server code are in
`backend-data` section 7.

## 1. When it is the right database

- Yes: local-first and desktop apps, mobile, CLI tools and agents, embedded
  devices, a single-server web app with moderate writes (one writer handles
  thousands of short write transactions per second on an SSD), read-heavy
  sites, analytics on a file, and tests of code that runs on SQLite.
- No: several app servers writing to one file over a network (never on NFS
  or SMB, where locking is unreliable), sustained heavy concurrent writes,
  per-user roles and network access control, data bigger than one disk
  comfortably holds.
- In between: one server with Litestream backups goes a long way; libSQL,
  Turso or Cloudflare D1 when it must be served or replicated (section 8).

## 2. Pragmas on every connection

Only `journal_mode = WAL` is stored in the file; the rest resets for each
connection, so set them where connections open:

```sql
PRAGMA journal_mode = WAL;     -- readers and the writer stop blocking each other
PRAGMA busy_timeout = 5000;    -- wait up to 5s for the lock instead of failing
PRAGMA foreign_keys = ON;      -- off by default, per connection
PRAGMA synchronous = NORMAL;   -- safe with WAL: a power cut may lose the last commits, never corrupts
PRAGMA cache_size = -64000;    -- 64 MB page cache (negative means KiB)
PRAGMA temp_store = MEMORY;
```

- `synchronous = FULL` where even the last commits must survive a power cut
  (money on a device with no server behind it).
- Per stack: Go DSNs take them (`modernc.org/sqlite`:
  `file:app.db?_pragma=busy_timeout(5000)&_pragma=journal_mode(WAL)`;
  `mattn/go-sqlite3`: `_busy_timeout=5000&_journal_mode=WAL&_foreign_keys=1`);
  Python's `sqlite3.connect(path, timeout=5)` sets the busy timeout; Laravel
  11+ has `busy_timeout`, `journal_mode` and `synchronous` keys on its
  sqlite connection; Rails 7.1+ sets WAL and friends by default.

## 3. One writer at a time

- With WAL, readers run beside one writer; a second writer waits up to
  `busy_timeout` for the first to commit. Throughput comes from short write
  transactions, not parallel ones.
- Start every transaction that will write with `BEGIN IMMEDIATE`. A plain
  `BEGIN` starts as a reader; if another connection commits before it
  writes, its write fails at once with `SQLITE_BUSY` (its snapshot is stale),
  and the busy timeout does not help.

```sql
BEGIN IMMEDIATE;
UPDATE products SET stock = stock - 1 WHERE id = ?1 AND stock >= 1;
INSERT INTO order_items (order_id, product_id, qty) VALUES (?2, ?1, 1);
COMMIT;
```

  In drivers: better-sqlite3 `db.transaction(fn).immediate()`, Python
  `isolation_level="IMMEDIATE"`, `mattn/go-sqlite3` `_txlock=immediate`.
- In a server: one connection (or a pool of one) for writes and a pool for
  reads, or a queue in front of the writes. Many threads writing through
  their own connections mostly wait on each other.
- Write transactions last milliseconds, with no network calls inside
  (`database-transactions`).
- A read transaction left open stops checkpoints from resetting the WAL, and
  the `-wal` file grows without bound. Close reads promptly; watch the size
  of `app.db-wal`.

## 4. STRICT tables and type affinity

Without STRICT a column's type is only a preference: an `INTEGER` column
keeps `'abc'` as text, `VARCHAR(10)` holds a megabyte, and whether `'1' = 1`
depends on the column. STRICT tables (3.37+) refuse wrong types:

```sql
CREATE TABLE orders (
  id          INTEGER PRIMARY KEY,   -- the rowid: 64-bit, assigned automatically
  public_id   TEXT NOT NULL UNIQUE,
  customer_id INTEGER NOT NULL REFERENCES customers (id) ON DELETE RESTRICT,
  status      TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'paid', 'cancelled')),
  total_cents INTEGER NOT NULL CHECK (total_cents >= 0),
  is_gift     INTEGER NOT NULL DEFAULT 0 CHECK (is_gift IN (0, 1)),
  created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  metadata    TEXT CHECK (metadata IS NULL OR json_valid(metadata))
) STRICT;
```

- STRICT allows `INTEGER`, `REAL`, `TEXT`, `BLOB` and `ANY`; length limits
  are CHECKs (`CHECK (length(name) <= 200)`).
- `INTEGER PRIMARY KEY` is the rowid itself. `AUTOINCREMENT` only when ids
  must never be reused after deletes; it costs a lookup per insert.
- Dates in one format for the whole database: ISO-8601 UTC text
  (`2026-09-29T10:15:00.000Z`, sorts correctly, readable in a console) or
  Unix time as INTEGER with the unit in the name (`created_at_ms`). Never
  both. `unixepoch()` (3.38+) and `strftime` convert.
- Booleans are 0 and 1 with a CHECK; money is INTEGER minor units; UUIDs are
  16-byte BLOBs or TEXT.
- JSON is built in (3.38+): `json_extract(metadata, '$.color')` or
  `metadata ->> '$.color'`, and binary `jsonb` storage from 3.45.
- `LIKE` and `COLLATE NOCASE` fold case for ASCII only. For unique emails
  store them lowercased, or index `lower(email)`.

## 5. Foreign keys, schema changes, migrations

- With `foreign_keys` off, `REFERENCES` is decoration. Check existing data
  with `PRAGMA foreign_key_check;` before trusting it.
- `ALTER TABLE` can add a column (not UNIQUE or PRIMARY KEY; NOT NULL only
  with a constant default), rename a column (3.25+) or table, and drop a
  column (3.35+; not keys, indexed, unique or referenced columns). Anything
  else is the documented rebuild:

```sql
PRAGMA foreign_keys = OFF;     -- before BEGIN: it is ignored inside a transaction
BEGIN;
CREATE TABLE orders_new (/* the new definition */) STRICT;
INSERT INTO orders_new (id, customer_id, total_cents, created_at)
  SELECT id, customer_id, total_cents, created_at FROM orders;
DROP TABLE orders;
ALTER TABLE orders_new RENAME TO orders;
-- recreate the indexes, triggers and views that belonged to orders
PRAGMA foreign_key_check;      -- must return no rows
COMMIT;
PRAGMA foreign_keys = ON;
```

- Use the stack's migration tool (Drizzle Kit, Prisma, Alembic with
  `render_as_batch=True`, which does the rebuild for you, Django, Rails,
  Laravel, sqlx, goose). DDL is transactional, so a failed migration rolls
  back whole.
- Apps without a migration tool (mobile, desktop, CLI) keep the schema
  version in `PRAGMA user_version`: read it on open, apply the missing steps
  in one transaction, set it.

## 6. Performance

- Bulk inserts in one transaction: each standalone insert is a transaction
  with its own sync to disk, so thousands of rows take seconds instead of
  milliseconds.
- Keep connections open and statements prepared; never a connection per
  request.
- Indexes as in `database-indexes`, checked with `EXPLAIN QUERY PLAN`:
  `SCAN` is a full scan, `SEARCH ... USING INDEX` is not, `USE TEMP B-TREE
  FOR ORDER BY` is a sort.
- Statistics: `PRAGMA optimize;` before closing a short-lived connection; on
  a long-lived one, `PRAGMA optimize=0x10002;` when it opens and
  `PRAGMA optimize;` every few hours.
- `VACUUM` rewrites the file to reclaim space: it needs free disk the size
  of the file and blocks writers. `auto_vacuum = INCREMENTAL` takes effect on
  a new database or after a `VACUUM`.

## 7. Backups

- Never `cp` a live database: a copy taken mid-write, or without its `-wal`
  file, can be corrupt or missing recent commits.
- Consistent copies while the app runs:

```sh
sqlite3 app.db "VACUUM INTO '/backups/app-$(date +%Y%m%d%H%M).db'"   # compacted copy
sqlite3 app.db ".backup '/backups/app.db'"                           # online backup API
```

- Continuous: Litestream streams WAL changes to S3-compatible storage, so
  seconds of data are at risk, and restores with
  `litestream restore -o app.db s3://bucket/app.db`. Run it beside the app
  (a sidecar or a systemd unit), and test the restore (`database-operations`).
- A restored file passes `PRAGMA integrity_check;` (prints `ok`) and
  `PRAGMA foreign_key_check;` (no rows).
- The file lives outside the web root, inside what is backed up, and in
  containers on a persistent volume, never in the image layer.

## 8. Replicated and hosted SQLite

| Option | What it is | Choose when |
|---|---|---|
| Litestream | Continuous backup of one file to object storage | One server; backup and disaster recovery |
| libSQL and Turso | An SQLite fork with a server, HTTP access and embedded replicas that sync | Reads near users, or apps that keep a local replica |
| Cloudflare D1 | SQLite databases served to Workers | Apps already on Workers; 10 GB per database on paid plans |
| LiteFS | FUSE-based replication across Fly.io machines | Read replicas on Fly; check its maintenance status first |

## 9. Tests

- An in-memory database per test (`:memory:`) with the migrations applied.
  Each `:memory:` connection is a separate database: for several
  connections in one test use a named shared one
  (`file:test1?mode=memory&cache=shared`) or a temporary file.
- Many tests: migrate one template file once, copy it per test or per
  worker.
- The same pragmas as production, foreign keys above all, or violations go
  unnoticed in tests.
- SQLite does not stand in for Postgres or MySQL: types, locking, dialect
  and constraint behaviour differ (`backend-testing`).

## Check it

```sh
sqlite3 app.db "PRAGMA journal_mode; PRAGMA integrity_check; PRAGMA foreign_key_check;"
sqlite3 app.db "SELECT name FROM sqlite_master WHERE type = 'table' AND sql NOT LIKE '%STRICT%';"
sqlite3 app.db "EXPLAIN QUERY PLAN SELECT id FROM orders WHERE customer_id = 1 ORDER BY created_at DESC;"
```

- `journal_mode` prints `wal`. Confirm `foreign_keys` and `busy_timeout`
  from the app's own connection: a fresh CLI session always reports the
  defaults.
- Two writers at once in a test (two connections, `BEGIN IMMEDIATE`): one
  waits, both succeed; a load test shows no "database is locked".
- The latest backup restores, opens and passes `integrity_check`.

## Avoid

The rollback journal for a server app; no busy timeout; foreign keys never
switched on; tables without STRICT; dates in mixed formats; deferred `BEGIN`
for read-modify-write; long transactions holding the writer or pinning the
WAL; `cp` of a live file; the file on a network share or in a container
layer; one transaction per row in a bulk load; SQLite standing in for
another engine in tests.
