---
name: database-mysql
description: "MySQL and MariaDB specifics: InnoDB, utf8mb4 and collations, strict mode, time types, the clustered primary key and what it means for UUIDs, JSON, online schema changes, repeatable read and gap locks, replication lag, and hosted Vitess. Read when the project uses MySQL or MariaDB."
---

# MySQL and MariaDB

The generated MySQL: `utf8` tables that cannot store an emoji, latin1
columns joined to utf8mb4 ones so the index is skipped, an emptied
`sql_mode` that turns bad dates into `0000-00-00`, `TIMESTAMP` columns that
end in 2038, `CHAR(36)` UUID primary keys fragmenting every page, an ALTER
that copies a 200 GB table while replicas fall hours behind, and a select
then insert under repeatable read that deadlocks on gap locks. This skill is
what is specific to MySQL and MariaDB; the general rules are in the other
`database-*` skills.

## 1. Versions and flavours

- MySQL 8.4 LTS for new work (8.0 reached end of life in April 2026); 9.x
  innovation releases only for teams that upgrade every quarter. MariaDB: a
  current LTS, 11.4 or later.
- MariaDB has diverged (JSON stored as checked text, its own optimiser and
  replication, `RETURNING`, system-versioned tables); check which one runs
  with `SELECT VERSION();` before copying advice.
- 8.4 changes: `mysql_native_password` is off by default
  (`caching_sha2_password`, so old clients must be upgraded), `mysqlpump` is
  gone, and replication commands say source and replica
  (`SHOW REPLICA STATUS`, `CHANGE REPLICATION SOURCE TO`).

## 2. InnoDB, utf8mb4 and collations

- InnoDB for every table. MyISAM has no transactions or foreign keys, locks
  whole tables and recovers badly from crashes.
- `utf8mb4` everywhere: server, database, tables, columns and the connection
  (`charset=utf8mb4` in the DSN). `utf8` is `utf8mb3`, three bytes at most,
  and rejects or cuts emoji and some CJK characters.
- Collation `utf8mb4_0900_ai_ci` (MySQL 8 default) or `utf8mb4_uca1400_ai_ci`
  (MariaDB). `_ai_ci` ignores accents and case: `'José' = 'jose'` is true and
  a UNIQUE key treats them as duplicates. Right for emails and names;
  `utf8mb4_bin` or `utf8mb4_0900_as_cs` for codes, tokens and anything
  case-sensitive.
- Joined columns with different collations skip the index or fail with
  "Illegal mix of collations": one collation per database.

```sql
CREATE DATABASE app CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci;
```

## 3. Strict mode

- Keep the 8.x default `sql_mode`: `ONLY_FULL_GROUP_BY, STRICT_TRANS_TABLES,
  NO_ZERO_IN_DATE, NO_ZERO_DATE, ERROR_FOR_DIVISION_BY_ZERO,
  NO_ENGINE_SUBSTITUTION`. Without strict mode, long strings are truncated,
  invalid dates become zeros and numbers are clipped, each with only a
  warning.
- MariaDB's default leaves out `ONLY_FULL_GROUP_BY`: add it.
- Frameworks can override it (Laravel's `'strict' => true` in
  `config/database.php` must stay true). Check what a session really has:
  `SELECT @@SESSION.sql_mode;`.
- CHECK constraints are enforced from MySQL 8.0.16; before that they were
  parsed and ignored. MariaDB enforces them from 10.2.

## 4. Types

| Data | Use | Note |
|---|---|---|
| Money | `BIGINT` minor units, or `DECIMAL(19,4)` | Never `FLOAT` or `DOUBLE` |
| A moment | `DATETIME(6)` holding UTC | `TIMESTAMP` converts through the session zone and ends 2038-01-19 |
| Boolean | `BOOLEAN` | It is `TINYINT(1)`: any small integer fits unless a CHECK says 0 or 1 |
| Text | `VARCHAR(n)` sized to the rule | `TEXT` takes no literal default and needs a prefix to be indexed |
| UUID | `BINARY(16)` | `CHAR(36)` costs 36 bytes or more in every index |
| JSON | `JSON` (binary in MySQL; checked `LONGTEXT` in MariaDB) | Index through generated columns |
| Fixed set | `VARCHAR` plus CHECK, or `ENUM` | Adding an `ENUM` value at the end can be instant; anything else rebuilds |

Set the session zone to UTC on connect (`SET time_zone = '+00:00'`, or the
driver's option) and convert for display in the application.

## 5. The clustered primary key

- InnoDB stores rows inside the primary key's B-tree, in key order, and
  every secondary index entry carries the primary key to find its row.
- A short key (`BIGINT UNSIGNED AUTO_INCREMENT`, 8 bytes) keeps every index
  small. Random keys (UUIDv4) insert all over the tree: page splits,
  half-empty pages, a working set that outgrows the buffer pool.
  Time-ordered UUIDv7 in `BINARY(16)` appends like an auto-increment:

```sql
CREATE TABLE orders (
  id          BINARY(16) NOT NULL,            -- UUIDv7 made by the app
  customer_id BIGINT UNSIGNED NOT NULL,
  total_cents BIGINT NOT NULL CHECK (total_cents >= 0),
  created_at  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
  PRIMARY KEY (id),
  KEY idx_orders_customer_id_created_at (customer_id, created_at),
  CONSTRAINT orders_customer_id_fkey FOREIGN KEY (customer_id) REFERENCES customers (id)
) ENGINE=InnoDB;
-- convert with UUID_TO_BIN(?) and BIN_TO_UUID(id); the swap flag is for v1 UUIDs only
```

- A table with no primary key gets a hidden row id, and row-based
  replication may scan the whole table on the replica for each changed row.
  Always declare one; `sql_require_primary_key = ON` enforces it.

## 6. Indexes and JSON

- General rules in `database-indexes`. MySQL adds: prefix indexes for long
  strings (`INDEX (title(50))`), keys up to 3,072 bytes, functional indexes
  (`INDEX ((lower(email)))`, 8.0.13+), descending indexes, invisible indexes
  to rehearse a removal (`ALTER TABLE orders ALTER INDEX idx_x INVISIBLE;`),
  and histograms for unindexed filters
  (`ANALYZE TABLE orders UPDATE HISTOGRAM ON status;`).
- JSON values are indexed through a virtual generated column (added in
  place), arrays through a multi-valued index (8.0.17+) that `MEMBER OF`,
  `JSON_CONTAINS` and `JSON_OVERLAPS` use:

```sql
ALTER TABLE events
  ADD COLUMN customer_id BIGINT UNSIGNED
    GENERATED ALWAYS AS (CAST(payload->>'$.customer_id' AS UNSIGNED)) VIRTUAL,
  ADD INDEX idx_events_customer_id (customer_id);

ALTER TABLE carts ADD INDEX idx_carts_product_ids
  ((CAST(payload->'$.product_ids' AS UNSIGNED ARRAY)));
-- WHERE 42 MEMBER OF (payload->'$.product_ids')
```

## 7. Online schema changes

- Each ALTER runs as INSTANT (metadata only), INPLACE (rebuilt while writes
  continue) or COPY (a new table, writes blocked). Name the one you expect so
  MySQL fails instead of falling back to a copy:

```sql
SET SESSION lock_wait_timeout = 5;   -- the default is a year
ALTER TABLE orders ADD COLUMN note VARCHAR(500) NULL, ALGORITHM=INSTANT;
ALTER TABLE orders ADD INDEX idx_orders_status_created_at (status, created_at),
  ALGORITHM=INPLACE, LOCK=NONE;
```

- INSTANT (8.0.29+) adds or drops a column at any position, renames columns,
  changes defaults and appends to an ENUM, up to 64 row versions per table;
  then a rebuild (`ALTER TABLE orders FORCE`) resets the count. Changing a
  column's type is a COPY.
- Metadata locks: every DDL, even INSTANT, needs an exclusive metadata lock.
  It waits behind any open transaction that touched the table, and every
  query after it queues behind the DDL. Look for long transactions first
  (`information_schema.innodb_trx`) and keep `lock_wait_timeout` short.
- A replica applies an ALTER only after the primary finished it, and blocks
  its apply thread for as long: an hour-long INPLACE build is an hour of
  replica lag. For big tables and COPY changes use gh-ost (copies through the
  binlog, no triggers, can pause; needs `binlog_format=ROW`, the default) or
  pt-online-schema-change (triggers). Both fill a shadow table and swap names
  at the end.
- DDL commits implicitly and cannot roll back: one DDL statement per
  migration (`database-migrations`).

## 8. Transactions and locks

- REPEATABLE READ by default. Plain `SELECT`s read a snapshot from the first
  read; `UPDATE`, `DELETE` and `SELECT ... FOR UPDATE` act on the latest
  committed rows and take next-key locks (the record and the gap before it)
  on every index entry they scan.
- Gap locks block inserts into the scanned range. "`SELECT ... FOR UPDATE`
  finds nothing, then `INSERT`" from two sessions: both hold the gap, both
  insert, deadlock. Use `INSERT ... ON DUPLICATE KEY UPDATE`, or insert and
  handle error 1062.
- Without an index on the `WHERE` of an `UPDATE` or `DELETE`, InnoDB locks
  every row it scans, possibly the whole table.
- READ COMMITTED per session (`SET SESSION TRANSACTION ISOLATION LEVEL READ
  COMMITTED;`) drops most gap locking and many deadlocks, when the code does
  not rely on a repeatable snapshot.
- Deadlocks roll back at once with error 1213; `innodb_lock_wait_timeout`
  (50s by default) raises 1205. Retry the whole transaction
  (`database-transactions`). `SHOW ENGINE INNODB STATUS` shows the latest
  deadlock; `innodb_print_all_deadlocks = ON` logs them all.
- `FOR UPDATE SKIP LOCKED` and `NOWAIT` exist from 8.0 for queues.
- `AUTO_INCREMENT` has gaps (rollbacks, bulk inserts); never use it as an
  invoice number.

## 9. Configuration and connections

| Setting | Default | Start with |
|---|---|---|
| `innodb_buffer_pool_size` | 128MB | 50 to 75% of RAM on a dedicated server, or `innodb_dedicated_server = ON` |
| `max_connections` | 151 | The sum of the pools plus headroom |
| `innodb_lock_wait_timeout` | 50s | 5 to 10s for web traffic |
| `max_execution_time` | 0, none | A ceiling in ms for `SELECT`s, per session |
| `slow_query_log`, `long_query_time` | off, 10s | on, 0.5 to 1s |
| `innodb_flush_log_at_trx_commit` | 1 | Keep 1: durable commits |
| `binlog_expire_logs_seconds` | 30 days | Long enough for point-in-time recovery |

`wait_timeout` (8 hours) closes idle connections: keep the pool's maximum
connection lifetime below it. Many instances go through ProxySQL or the
host's proxy (RDS Proxy).

## 10. Replication

- Asynchronous by default: the primary commits without waiting, and
  replicas lag from milliseconds to hours during big writes or DDL. Watch
  `SHOW REPLICA STATUS` (`Seconds_Behind_Source`, both threads running) or a
  heartbeat table (pt-heartbeat) for a truer figure.
- GTIDs (`gtid_mode = ON`) make failover and re-pointing replicas simple.
- Read your own writes: read from the primary for that user for a few
  seconds after a write, or wait on the replica for the write's GTID
  (`WAIT_FOR_EXECUTED_GTID_SET`). Never decide a write from a replica read.
- Semi-synchronous replication or Group Replication (InnoDB Cluster) when
  losing the last transactions on failover is not acceptable; managed high
  availability does it for you.

## 11. Hosted

| Host | Know |
|---|---|
| PlanetScale (Vitess) | Branches and deploy requests run schema changes online, with a revert window; check the current limits on foreign keys before relying on them |
| Amazon RDS, Aurora MySQL | Parameter groups, Multi-AZ, RDS Proxy; Aurora replicas share storage, so lag is usually well under a second |
| Google Cloud SQL, Azure Database for MySQL | Flags or server parameters, a high-availability option |
| Vitess, self-hosted | Sharding for very large data; a real operations commitment |

## 12. Backups

- Logical: `mysqldump --single-transaction --routines --triggers --events
  app > app.sql` (consistent for InnoDB without locking), or MySQL Shell's
  `util.dumpInstance()` and `util.loadDump()`, parallel and far faster on big
  databases. MariaDB: `mariadb-dump`.
- Physical: Percona XtraBackup (MySQL) or `mariadb-backup`, plus binary logs
  replayed with `mysqlbinlog` for point-in-time recovery. Restores are tested
  on a schedule (`database-operations`).

## Check it

```sql
SELECT VERSION(), @@sql_mode, @@character_set_server, @@collation_server,
       @@transaction_isolation;
SELECT table_name, engine, table_collation FROM information_schema.tables
WHERE table_schema = DATABASE()
  AND (engine <> 'InnoDB' OR table_collation NOT LIKE 'utf8mb4%');
SELECT * FROM sys.statement_analysis LIMIT 10;
SELECT * FROM sys.innodb_lock_waits;
```

- `EXPLAIN ANALYZE` for changed queries; `SHOW ENGINE INNODB STATUS` after a
  concurrent test for deadlocks.
- DDL rehearsed with an explicit `ALGORITHM` on a copy of the table, with
  replica lag watched while it runs.

## Avoid

`utf8` (utf8mb3); mixed collations; a non-strict `sql_mode`; MyISAM;
`TIMESTAMP` for business times or dates past 2038; floats for money;
`CHAR(36)` or random UUID primary keys; tables without a primary key; an
ALTER without `ALGORITHM` and a short `lock_wait_timeout` on a big table;
several DDL statements in one migration; select then insert under
repeatable read; unindexed `UPDATE` and `DELETE` filters; `AUTO_INCREMENT`
as a gap-free number; a replica read deciding a write.
