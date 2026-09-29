---
name: database-postgres
description: "PostgreSQL specifics: the types worth using (jsonb, arrays, ranges, uuid, citext), extensions (pg_trgm, pgvector, PostGIS, pg_stat_statements), row-level security, generated columns, LISTEN and NOTIFY, partitioning, vacuum, connection pooling, roles, configuration basics, and managed providers. Read when the project uses Postgres."
---

# PostgreSQL

The generated Postgres: `serial` ids and `timestamp` columns, `json` instead
of `jsonb`, the app connecting as `postgres`, default settings on a 16 GB
server, hundreds of direct connections from serverless functions, no
`pg_stat_statements`, a transaction left idle for a day while tables bloat,
and LISTEN/NOTIFY used as a job queue. This skill is what is specific to
Postgres; schema, queries, indexes and transactions in general are in the
other `database-*` skills.

## 1. Versions

17 and 18 are the current majors in 2026 (18 shipped in September 2025); a
new major comes each autumn and is supported for five years. Start on the
newest major your host offers; apply minor releases routinely (fixes only, a
restart). What changed that matters:

| Version | Worth knowing |
|---|---|
| 18 | `uuidv7()`; virtual generated columns (the new default); B-tree skip scan; asynchronous I/O; `OLD` and `NEW` in `RETURNING`; `pg_upgrade` keeps planner statistics; `EXPLAIN ANALYZE` shows buffers |
| 17 | `MERGE ... RETURNING`; `JSON_TABLE`; incremental base backups; `transaction_timeout`; vacuum needs far less memory |
| 16 | `pg_stat_io`; logical replication from standbys; `IS JSON` and SQL/JSON constructors |
| 15 | `MERGE`; `UNIQUE NULLS NOT DISTINCT`; `CREATE` on schema `public` no longer granted to all |
| 14 | Multiranges; `idle_session_timeout`; `DETACH PARTITION CONCURRENTLY` |
| 13 | `gen_random_uuid()` in core; B-tree deduplication |
| 12 | Stored generated columns; CTE inlining; `REINDEX CONCURRENTLY` |

## 2. Types worth using

- Keys `bigint generated always as identity`; `uuid` for UUIDs
  (`gen_random_uuid()` for v4, `uuidv7()` from 18).
- `timestamptz`, `date`, `interval`; `numeric(p, s)` for exact decimals;
  never the `money` type (it follows the server's locale).
- `text` with a CHECK rather than `varchar(n)`; `citext` for emails and
  usernames, so equality and unique constraints ignore case.
- `jsonb` (binary, indexable) rather than `json`: `->`, `->>`, `@>`, `?`,
  `jsonb_path_query`. `jsonb_set` rewrites the whole value, so large
  documents updated often are slow and bloat the table.
- Arrays (`text[]`) with GIN for tags; ranges (`tstzrange`, `daterange`)
  with `&&`, `@>` and exclusion constraints; `inet` for addresses;
  `tsvector` for full text; `bytea` only for small binaries.
- Generated columns: `GENERATED ALWAYS AS (expr) STORED` (12+) or `VIRTUAL`
  (18, computed on read); STORED when you index it.
- Enums: `ALTER TYPE ... ADD VALUE` is fast, but the new value cannot be
  used in the same transaction, and values cannot be removed.

## 3. Extensions

| Extension | For | Notes |
|---|---|---|
| `pg_stat_statements` | Statistics per query | Always on: `shared_preload_libraries`, then `CREATE EXTENSION` |
| `pg_trgm` | `ILIKE '%x%'`, fuzzy search, `similarity()` | GIN with `gin_trgm_ops` |
| `vector` (pgvector) | Embeddings and similarity search | HNSW index, below |
| `postgis` | Geography and geometry | `geography(Point, 4326)`, GiST, `ST_DWithin` |
| `citext` | Case-insensitive text | Unique constraints ignore case |
| `btree_gist` | Exclusion constraints mixing `=` and ranges | Bookings |
| `pgcrypto` | `gen_random_bytes()`, `digest()` | Hash passwords in the app (argon2id), not with `crypt()` |
| `unaccent` | Accent-insensitive search | Wrap in an immutable function to index it |
| `pg_partman`, `pg_cron` | Partition upkeep, jobs in the database | Where the host offers them |

Managed hosts allow a fixed list (`SELECT name FROM pg_available_extensions;`).
After upgrading an extension's package, `ALTER EXTENSION vector UPDATE;`.

```sql
CREATE EXTENSION IF NOT EXISTS vector;
ALTER TABLE documents ADD COLUMN embedding vector(1536);
CREATE INDEX CONCURRENTLY idx_documents_embedding
  ON documents USING hnsw (embedding vector_cosine_ops);
SELECT id FROM documents ORDER BY embedding <=> $1 LIMIT 10;
```

- `<->` L2, `<#>` negative inner product, `<=>` cosine distance; the index's
  operator class must match the query's operator.
- HNSW (`m = 16`, `ef_construction = 64`; raise `hnsw.ef_search` from 40 for
  recall) rather than IVFFlat, which needs the data loaded first. `vector`
  indexes up to 2,000 dimensions, `halfvec` up to 4,000. A `WHERE` filter can
  return fewer than `LIMIT` rows; pgvector 0.8+ has iterative scans
  (`SET hnsw.iterative_scan = relaxed_order`).

## 4. Row-level security

The second wall for tenant data: the database hides another tenant's rows
even when a query forgets its `WHERE`.

```sql
ALTER TABLE projects ENABLE ROW LEVEL SECURITY;
ALTER TABLE projects FORCE ROW LEVEL SECURITY;   -- the table owner too
CREATE POLICY tenant_isolation ON projects
  USING (tenant_id = current_setting('app.tenant_id', true)::bigint)
  WITH CHECK (tenant_id = current_setting('app.tenant_id', true)::bigint);
```

- Set the tenant per transaction, first thing:
  `SELECT set_config('app.tenant_id', $1, true);` (`true`: local to the
  transaction, so safe behind a transaction pooler). A session-level `SET`
  leaks to the next client of a pooled connection.
- A missing setting makes `current_setting(..., true)` NULL and the policy
  matches nothing: it fails closed.
- Superusers and `BYPASSRLS` roles skip policies, and so does the owner
  without `FORCE`. Views run with their owner's rights unless created with
  `security_invoker = true` (15+). Lead indexes with `tenant_id`.

## 5. LISTEN/NOTIFY and queues

- `NOTIFY` (payload up to 8,000 bytes) reaches sessions listening at the
  moment of commit; nothing is stored, so a listener that is down misses it.
  Use it to wake workers or drop caches, carrying ids, never as the queue.
  A listener needs its own session connection, not a transaction-pooled one.
- A committing transaction that sent a NOTIFY takes a database-wide lock
  during commit; at high write rates that serialises commits.
- The durable queue is a table read with `FOR UPDATE SKIP LOCKED`
  (`database-transactions`) or a library on it: pgmq, pg-boss,
  graphile-worker, River, Procrastinate.

## 6. Partitioning

For tables in the hundreds of gigabytes, or data dropped by age (events,
logs, metrics): removing a month becomes dropping a partition instead of a
huge `DELETE`.

```sql
CREATE TABLE events (
  id         bigint GENERATED ALWAYS AS IDENTITY,
  created_at timestamptz NOT NULL,
  payload    jsonb NOT NULL,
  PRIMARY KEY (id, created_at)       -- must include the partition key
) PARTITION BY RANGE (created_at);
CREATE TABLE events_2026_10 PARTITION OF events
  FOR VALUES FROM ('2026-10-01') TO ('2026-11-01');
```

- Unique constraints must include the partition key, and queries that do
  not filter on it visit every partition. Create partitions ahead
  (pg_partman or a scheduled job); retire old ones with
  `DETACH PARTITION ... CONCURRENTLY`, then drop.
- Not for a table of a few million rows, nor as a speed-up: that is an index.

## 7. Vacuum and bloat

- Updates and deletes leave old row versions; autovacuum reclaims them once
  no transaction can still see them. The enemy is whatever holds that
  horizon back: a session `idle in transaction` for hours, an abandoned
  replication slot, a long query on a replica with `hot_standby_feedback`.
  Set `idle_in_transaction_session_timeout`, watch `xact_start` in
  `pg_stat_activity`, drop replication slots no longer active.
- Autovacuum starts when 20% of a table has changed; on large busy tables
  lower it per table:
  `ALTER TABLE events SET (autovacuum_vacuum_scale_factor = 0.02, autovacuum_analyze_scale_factor = 0.01);`
- Transaction id age (`SELECT datname, age(datfrozenxid) FROM pg_database;`)
  must stay far from the 2 billion limit: autovacuum forces a freeze at 200
  million per table by default; alert well before 1 billion, since near the
  limit Postgres stops accepting writes.
- Bloated tables: `pg_repack` rebuilds online; `VACUUM FULL` locks the table
  for the whole rewrite. Indexes: `REINDEX INDEX CONCURRENTLY`.

## 8. Connections and pooling

- Each connection is a server process costing megabytes; past a few hundred
  active ones throughput drops. `max_connections` defaults to 100.
- Add it up: pool size times instances, plus workers, migrations and admin,
  under `max_connections`. Pools of 5 to 20 per instance; for the whole
  server, about twice the CPU cores in active connections is a good start.
- A pooler in front of many instances or serverless functions: PgBouncer, or
  the host's (Supabase Supavisor, Neon's pooled host, RDS Proxy). In
  transaction mode session state breaks: `SET` (use `SET LOCAL` or
  `set_config(..., true)`), session advisory locks, `LISTEN`, temporary
  tables, `WITH HOLD` cursors. Prepared statements work from PgBouncer 1.21
  with `max_prepared_statements` set; older setups disable them in the
  driver.
- Migrations and listeners connect directly or in session mode. Serverless
  and edge runtimes use an HTTP or WebSocket driver
  (`@neondatabase/serverless`), Cloudflare Hyperdrive, or the pooled URL
  with a pool of 1 per instance.

## 9. Configuration basics

| Setting | Default | Start with |
|---|---|---|
| `shared_buffers` | 128MB | 25% of RAM |
| `effective_cache_size` | 4GB | 50 to 75% of RAM (a planner hint) |
| `work_mem` | 4MB | 16 to 64MB; per sort or hash per query, so mind the connections |
| `maintenance_work_mem` | 64MB | 512MB to 2GB, for vacuum and index builds |
| `random_page_cost` | 4.0 | 1.1 on SSD |
| `statement_timeout` | 0, none | Per role: 15s for the web app, longer for jobs |
| `idle_in_transaction_session_timeout` | 0, none | 60s for app roles |
| `log_min_duration_statement` | -1, off | 500ms to 1s |
| `shared_preload_libraries` | empty | `pg_stat_statements`, and `auto_explain` if wanted |

Set per role where it differs (`ALTER ROLE app SET statement_timeout =
'15s';`). Managed hosts expose these as parameter groups or flags.

## 10. Roles and privileges

```sql
CREATE ROLE app_owner LOGIN;   -- owns the schema; migrations run as it
CREATE ROLE app LOGIN;         -- the running app: rows only
CREATE ROLE app_readonly LOGIN;
-- passwords are set by the operator from the secret store, never committed
CREATE SCHEMA app AUTHORIZATION app_owner;
GRANT USAGE ON SCHEMA app TO app, app_readonly;
ALTER DEFAULT PRIVILEGES FOR ROLE app_owner IN SCHEMA app
  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO app;
ALTER DEFAULT PRIVILEGES FOR ROLE app_owner IN SCHEMA app
  GRANT USAGE, SELECT ON SEQUENCES TO app;
ALTER DEFAULT PRIVILEGES FOR ROLE app_owner IN SCHEMA app
  GRANT SELECT ON TABLES TO app_readonly;
```

- Default privileges apply to objects that role creates: run migrations as
  `app_owner` (or `SET ROLE app_owner` first), or new tables end up
  unreadable by the app.
- No superuser, `CREATEROLE` or table ownership for the app.
  `pg_read_all_data` (14+) is a quick read-only grant, across every schema.
  `scram-sha-256` passwords (default since 14); `sslmode=verify-full` from
  outside a private network.

## 11. Providers and local development

| Provider | Know before choosing |
|---|---|
| Amazon RDS, Aurora | No true superuser (`rds_superuser`); parameter groups; RDS Proxy; Multi-AZ failover |
| Google Cloud SQL, AlloyDB | Connect through the Cloud SQL Auth Proxy or private IP; flags for settings |
| Supabase | Pooler on 6543 (transaction) and 5432 (session); client access leans on RLS |
| Neon | Branches per preview, scale to zero with a cold start; a `-pooler` host for pooled connections |

Compare the extensions allowed, connection limits per plan, backup and
point-in-time windows, regions, and whether logical replication is
available for moving out later. Locally, the production major in Docker,
bound to localhost:

```yaml
services:
  db:
    image: postgres:17
    environment: { POSTGRES_USER: app, POSTGRES_PASSWORD: app, POSTGRES_DB: app }
    ports: ["127.0.0.1:5432:5432"]
    volumes: ["pgdata:/var/lib/postgresql/data"]
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U app -d app"]
      interval: 5s
      retries: 10
volumes:
  pgdata:
```

From the `postgres:18` image on, mount the volume at `/var/lib/postgresql`:
the data directory moved to a versioned path beneath it. Backups
(`pg_dump -Fc`, pgBackRest, point-in-time recovery) are in
`database-operations`.

## Check it

```sql
SELECT version();
SELECT name, setting, unit FROM pg_settings
WHERE name IN ('shared_buffers', 'work_mem', 'max_connections', 'statement_timeout');
SELECT query, calls, round(mean_exec_time) AS mean_ms, round(total_exec_time) AS total_ms
FROM pg_stat_statements ORDER BY total_exec_time DESC LIMIT 10;
SELECT state, count(*) FROM pg_stat_activity GROUP BY state;
SELECT rolname, rolsuper, rolbypassrls FROM pg_roles WHERE rolcanlogin;
```

- The app's role is neither superuser nor owner; as that role, another
  tenant's rows are invisible and inserting one fails.
- No `idle in transaction` session over a minute; no inactive slot.

## Avoid

`serial`, `timestamp`, `json` and `money`; the app as superuser or owner;
default memory settings in production; running without
`pg_stat_statements`; hundreds of direct connections from functions;
session `SET` or advisory locks behind a transaction pooler; LISTEN/NOTIFY
as a queue; transactions left idle; `VACUUM FULL` on a live table;
partitioning a small table; RLS without `FORCE` or with the tenant set per
session; a local database on another major version than production.
