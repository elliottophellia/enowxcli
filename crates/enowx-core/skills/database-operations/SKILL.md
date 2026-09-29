---
name: database-operations
description: "Running a database: automated backups and tested restores, point-in-time recovery, replicas and failover, connection pooling, monitoring the signals that matter, routine maintenance, upgrades, security and least privilege, data retention and deletion, seeds and local development. Read before setting up, operating or moving a database."
---

# Running a database

The generated setup: a nightly `pg_dump` onto the same disk as the database,
never restored; the port open to the internet with the default superuser; a
pool of 50 connections in every serverless function; no alert until the
disk is full; a major upgrade tried first in production; personal data kept
forever and copied to laptops for debugging. This skill is how a database is
run so its data survives mistakes, failures and audits. Engine details are
in `database-postgres`, `database-mysql`, `database-sqlite` and
`database-mongodb`.

## 1. Decide the targets first

- **RPO**: how much data may be lost. **RTO**: how long until it is back.
  Agree them with the owner in writing ("at most 5 minutes of data, back
  within an hour"); they choose the tools.

| Target | Needs |
|---|---|
| RPO a day, RTO hours | Daily logical dumps, off-site, restored on a schedule |
| RPO minutes | Point-in-time recovery: base backups plus continuous WAL or binlog archiving |
| RTO minutes | A standby ready to promote (managed high availability), as well as backups |
| Undoing a mistake (a bad `DELETE`) | PITR to just before it; replicas copy the mistake at once |

## 2. Backups

- Automated, encrypted, off-site (another account or provider, never the
  same disk or cloud account as the database), with retention written down,
  for example 7 to 14 daily, 4 to 8 weekly and 6 to 12 monthly, adjusted to
  legal requirements.

| Engine | Logical | Physical and point in time |
|---|---|---|
| PostgreSQL | `pg_dump -Fc`, `pg_dumpall --globals-only` for roles | pgBackRest, Barman or WAL-G, or the host's PITR |
| MySQL | `mysqldump --single-transaction`, MySQL Shell `util.dumpInstance()` | Percona XtraBackup plus binary logs |
| MariaDB | `mariadb-dump` | `mariadb-backup` plus binary logs |
| SQLite | `VACUUM INTO`, `.backup` | Litestream |
| MongoDB | `mongodump`, small databases only | Atlas backup, Percona Backup for MongoDB, snapshots |

- Managed databases: switch PITR on and check the window (RDS keeps up to 35
  days). Back up what a dump leaves out: roles and grants, the list of
  extensions, the configuration, and the key that decrypts the backups,
  stored apart from them.
- A stream straight to object storage leaves no plain copy on disk:

```bash
#!/usr/bin/env bash
set -euo pipefail   # a failed pg_dump fails the job
stamp=$(date -u +%Y%m%dT%H%M%SZ)
pg_dump -Fc -d "$DATABASE_URL" \
  | age -r "$BACKUP_AGE_RECIPIENT" \
  | aws s3 cp - "s3://$BACKUP_BUCKET/postgres/app-$stamp.dump.age"
```

- The job reports success or failure to monitoring; a backup that silently
  stopped weeks ago is the usual discovery during an incident.

## 3. Restores, tested

A backup never restored is a hope. Restore on a schedule (monthly at least,
and after any change to the backup setup) into a scratch instance,
scripted:

```bash
createdb app_restore_test
age -d -i "$BACKUP_AGE_KEY_FILE" app-20260929T020000Z.dump.age \
  | pg_restore --no-owner --no-privileges -d app_restore_test
psql -d app_restore_test -c "SELECT count(*), max(created_at) FROM orders;"
```

- Check what matters: row counts of the key tables against production, the
  newest timestamp (the RPO you really have), the app starting against it
  with a smoke test, and the total time taken (the RTO you really have).
  Record the results.
- Rehearse point-in-time recovery to a chosen moment
  (`recovery_target_time` in Postgres, `mysqlbinlog --stop-datetime` for
  MySQL) before you need it.
- A runbook: where the backups are, who holds the key, the commands, the
  expected duration.

## 4. Replicas and failover

- High availability from the provider (RDS Multi-AZ, Cloud SQL HA, Atlas
  replica sets) or Patroni for self-hosted Postgres; never hand-written
  failover scripts.
- Clients survive a failover: reconnect with backoff, retry idempotent work,
  connect through the provider's endpoint (short DNS TTL) rather than an
  instance address. libpq takes several hosts with
  `target_session_attrs=read-write`.
- Fail over on purpose (RDS reboot with failover) and time the app's
  recovery.
- Replicas lag: send them only reads that tolerate it, and keep reads that
  decide writes on the primary (`database-transactions`). A replica is not a
  backup; it copies a `DROP TABLE` within milliseconds.

## 5. Connection pooling

- Instances times pool size, plus workers and admin, stays under the
  server's limit with headroom. Pools of 5 to 20 per instance, a connect
  timeout of a few seconds, an idle timeout, and a maximum lifetime below the
  server's or proxy's idle cut-off.
- Serverless and edge: a pooler or an HTTP driver (PgBouncer or the
  provider's pooler, Neon's serverless driver, Prisma Accelerate, Cloudflare
  Hyperdrive, RDS Proxy, PlanetScale's serverless driver) and a pool of 1 per
  function instance. Transaction-mode pitfalls are in `database-postgres`.

## 6. Monitoring

Alert on what users feel and on resources that end in an outage:

| Signal | Postgres; MySQL | Alert when |
|---|---|---|
| Connections | `pg_stat_activity`; `Threads_connected` | Over 80% of the limit |
| Slow queries | `pg_stat_statements`; slow log, `sys.statement_analysis` | Top total time or p95 jumps after a deploy |
| Locks | `pg_blocking_pids()`, `deadlocks` in `pg_stat_database`; `sys.innodb_lock_waits` | Sustained waits, deadlock spikes |
| Long transactions | `xact_start`; `information_schema.innodb_trx` | Open for more than a few minutes |
| Replication lag | `replay_lag` in `pg_stat_replication`; `Seconds_Behind_Source` | Beyond what reads tolerate, for example 30s |
| Disk and IOPS | The host's metrics | 80% full, or on course to fill within two weeks |
| Cache hit ratio | `pg_statio_user_tables`; buffer pool reads | Below 0.99 for OLTP |
| Vacuum, wraparound | `n_dead_tup`, `age(datfrozenxid)` | Autovacuum behind; age over 1 billion |
| Backups | The backup job | Missed run, failed verification |

```sql
-- who blocks whom
SELECT pid, pg_blocking_pids(pid) AS blocked_by, wait_event_type, left(query, 60) AS query
FROM pg_stat_activity WHERE cardinality(pg_blocking_pids(pid)) > 0;
-- cache hit ratio
SELECT round(sum(heap_blks_hit) / nullif(sum(heap_blks_hit) + sum(heap_blks_read), 0), 4)
FROM pg_statio_user_tables;
-- largest tables, indexes included
SELECT relname, pg_size_pretty(pg_total_relation_size(relid)) AS size
FROM pg_statio_user_tables ORDER BY pg_total_relation_size(relid) DESC LIMIT 10;
```

Tools: the host's dashboards, postgres_exporter or mysqld_exporter with
Prometheus and Grafana, pganalyze, Percona Monitoring and Management.

## 7. Maintenance

- Postgres: autovacuum tuned for big tables, `ANALYZE` after bulk loads,
  `REINDEX INDEX CONCURRENTLY` and `pg_repack` for bloat. MySQL:
  `ANALYZE TABLE` after big changes, `OPTIMIZE TABLE` rarely (it rebuilds).
  SQLite: `PRAGMA optimize`, an occasional `VACUUM`.
- Cleanup jobs for expired rows (sessions, tokens, soft-deleted records past
  retention, old events): scheduled, idempotent, in batches. A single
  `DELETE` of millions of rows holds locks, bloats the table and floods the
  replicas; time-partitioned tables drop partitions instead.

```sql
-- repeat until it deletes 0 rows, pausing briefly between runs
DELETE FROM sessions
WHERE id IN (
  SELECT id FROM sessions WHERE expires_at < now() - interval '7 days'
  ORDER BY id LIMIT 5000
);
```

- Minor versions applied as released, after reading the notes (a few ask
  for a reindex).

## 8. Upgrades

| Method | Downtime | Use |
|---|---|---|
| Dump and restore | Grows with size | Small databases, changing hosts |
| `pg_upgrade --link` | Minutes | Same server; run with `--check` first |
| Logical replication to the new major, then switch | Seconds | Large Postgres; RDS Blue/Green |
| MySQL in-place | Minutes | 8.0 to 8.4, after MySQL Shell's `util.checkForServerUpgrade()` |
| MongoDB rolling | Little or none | One major at a time, then `setFeatureCompatibilityVersion` |

- Rehearse on a restored copy: the test suite and the slowest queries, since
  plans can change between majors. Write the rollback before starting, and
  keep a backup of the old version until the new has run cleanly for a
  while.
- After a Postgres upgrade, refresh statistics
  (`vacuumdb --all --analyze-in-stages`; from 18 `pg_upgrade` keeps most, and
  `vacuumdb --missing-stats-only` fills the rest) and update extensions
  (`ALTER EXTENSION ... UPDATE`).

## 9. Security

- Private network only: no public address, or an allow-list of fixed ones if
  it cannot be avoided. On a VPS, Docker's published ports bypass ufw: bind
  to `127.0.0.1:5432:5432` or do not publish the port.
- TLS for every connection leaving the machine, with the server verified
  (`sslmode=verify-full` in Postgres, `--ssl-mode=VERIFY_IDENTITY` in
  MySQL).
- A role per purpose (migration owner, app with rows only, read-only for
  reports and support, one per service); no shared admin password, no
  superuser in any app (`database-postgres` has the grants).
- Credentials from a secret manager or the platform's environment, rotated,
  never in the repository, logs or error messages.
- Encryption at rest (default on managed hosts; LUKS or encrypted cloud
  disks when self-hosted) and backups encrypted with their own key.
- Audit logs where required (pgaudit, or the host's): who read or changed
  sensitive tables. OS and database patches kept current.

## 10. Retention and deletion

- A retention period per kind of data, written down and enforced by the
  cleanup jobs: for example sessions 30 days, request logs 90 days, invoices
  as long as tax law requires.
- Deleting personal data on request (GDPR, Indonesia's UU PDP No. 27/2022
  and similar laws): find every table and service that holds it, delete or
  anonymise, record that it was done. Backups age out on their own schedule;
  the privacy policy says so, rather than anyone editing backups.
- Staging and analytics get anonymised or synthetic data, never a raw
  production copy: mask names, emails, phones, addresses and free text
  (PostgreSQL Anonymizer, or a scripted transform during the restore),
  keeping ids so relations still hold. Nothing from production on laptops.

## 11. Seeds and local development

- A seed script in the repository: deterministic (fixed ids, or a fixed
  faker seed such as `faker.seed(42)`), idempotent (upserts, safe to run
  twice), clearly fake (`Sample shop 1`, `owner@example.com`), covering the
  states the UI shows (empty, one, many, each status), and refusing to run
  when the URL points at production.
- Locally: Docker Compose with a named volume, the production major
  version, bound to localhost (the file is in `database-postgres`). Init
  scripts in `/docker-entrypoint-initdb.d` run only on an empty volume;
  `docker compose down -v` starts over.
- Every developer and CI run the same migrations from empty; nobody shares a
  development database.

## Check it

- This month's restore test exists, with counts, the newest timestamp and
  the time it took.
- From outside the private network the port does not answer
  (`nc -vz <public-ip> 5432` fails).
- The app's role cannot create tables or read other schemas.
- Alerts exist for disk, connections, replication lag and failed backups,
  and one was fired in a test.
- Cleanup jobs run and the tables they clean stay flat in size.
- The report says plainly what could not be checked (no console access, no
  restore performed).

## Avoid

Backups on the same disk or account as the database; backups never
restored; a replica called a backup; the port open to the internet or
published by Docker past the firewall; a superuser in the app; secrets in
the repository or logs; pools that add up past the connection limit; no
alert until the disk is full; million-row `DELETE`s in one statement; a
major upgrade first tried in production; personal data kept forever or
copied to laptops and staging; seeds that can run against production.
