---
name: database-schema
description: "Modelling a relational schema: entities and relationships from the domain, keys, column types for money, time, text, ids and enums, constraints that keep data true, naming, soft deletes, audit columns, multi-tenancy, hierarchies, history, and JSON columns. Read before creating or changing tables."
---

# Schema design

The generated schema: one table per screen, a UUIDv4 in a `varchar(36)` as
the key, every column nullable and `varchar(255)`, `price float`, `status`
holding any string, a `data json` column carrying half the model, no foreign
keys "for flexibility", naive timestamps, and a `type` plus `target_id` pair
pointing at any table. It works until the second writer, the first report or
the first bad row. This skill turns a domain into tables that keep themselves
true. Examples are PostgreSQL; MySQL and SQLite forms are in their skills,
and changing a live table is `database-migrations`.

## 1. From the domain to tables

- For each noun the business uses (customer, order, invoice, booking): who
  creates it, its states from start to end, what it belongs to, whether it
  is ever deleted. Then list the queries (screens, reports, jobs, filters):
  each should be a few joins on indexed keys.
- Normalise to about third normal form: each fact once, in the table whose
  key it describes.
- Copy on purpose what must not change later: an order line keeps
  `unit_price_cents`, the tax rate and the product name at the time of sale.
  Joining to today's price rewrites history; that is correctness, not
  denormalisation.
- Denormalise for speed only with a mechanism that keeps it right: a counter
  updated in the same transaction or by a trigger, a summary table refreshed
  by a job, a materialised view refreshed `CONCURRENTLY` (needs a unique
  index). Comment which column is derived and from what.

## 2. Keys

| Choice | Use when | Notes |
|---|---|---|
| `bigint generated always as identity` | The default | 8 bytes, ordered inserts, compact indexes and FKs |
| UUIDv7 in a `uuid` column | Ids made by clients or offline, merged databases, ids in URLs | Time-ordered, so inserts stay local; reveals creation time |
| UUIDv4 | An id that must reveal nothing, not even time | Random inserts scatter over the index; worst on MySQL's clustered key |
| Natural key (email, SKU) | Never as the primary key | A UNIQUE constraint: natural keys change |
| Composite key | Join tables, `(order_id, line_no)` | The pair is the identity |

```sql
CREATE TABLE orders (
  id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  public_id   text NOT NULL UNIQUE,            -- 'ord_' plus random, for URLs
  customer_id bigint NOT NULL REFERENCES customers (id) ON DELETE RESTRICT,
  status      text NOT NULL DEFAULT 'draft'
              CHECK (status IN ('draft', 'placed', 'paid', 'shipped', 'cancelled')),
  total_cents bigint NOT NULL CHECK (total_cents >= 0),
  currency    text NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
  paid_at     timestamptz,
  created_at  timestamptz NOT NULL DEFAULT now(),
  updated_at  timestamptz NOT NULL DEFAULT now(),
  CHECK (status NOT IN ('paid', 'shipped') OR paid_at IS NOT NULL)
);
```

- `GENERATED ALWAYS` refuses hand-written ids (`OVERRIDING SYSTEM VALUE` for
  imports; afterwards `SELECT setval(pg_get_serial_sequence('orders', 'id'),
  (SELECT max(id) FROM orders));`). No `serial` in new tables.
- UUIDv7: `uuidv7()` is built into PostgreSQL 18; before that, make it in the
  app (`v7()` from `uuid` 10+ in Node, `uuid.uuid7()` in Python 3.14,
  `uuid.NewV7()` in Go's google/uuid, `Uuid::now_v7()` in Rust).
- A public id apart from the internal one when sequential ids would reveal
  volume or invite enumeration. It is not permission: every access is still
  authorised.

## 3. Column types

| Data | Type | Not |
|---|---|---|
| Money | `bigint` minor units (`1999` cents, `1500000` rupiah) plus `currency` | `float`, `double`, `real`, `money` |
| Rates, FX, sub-cent prices | `numeric(19, 4)` or the scale the domain needs | `float` |
| A moment | `timestamptz` (stored as UTC) | `timestamp` without time zone |
| A calendar date | `date` | a timestamp at midnight |
| Wall-clock time in a place | `time` or `timestamp` plus an IANA zone column | a UTC instant |
| A duration | `interval`, or `bigint` named with its unit (`timeout_ms`) | text |
| Text | `text` with `CHECK (char_length(name) <= 200)` | `varchar(255)` by habit, `char(n)` |
| Yes or no | `boolean` | int flags, `'Y'` and `'N'` |
| Email | `citext`, or `text` with a unique index on `lower(email)` | a case-sensitive unique |
| Varying attributes | `jsonb` | `json`, entity-attribute-value tables |

- `timestamptz` stores an instant, not a zone. When local time is the fact (a
  class every Monday at 18:00 in Jakarta), store the local time and the zone
  (`Asia/Jakarta`): zone rules change, and a stored future instant can end up
  an hour off. `now()` is the transaction's start; `clock_timestamp()` the
  wall clock.
- Postgres `text` and `varchar(n)` perform the same; a CHECK keeps the limit
  changeable. MySQL needs `VARCHAR(n)` to index a column (`database-mysql`).

Enums, decided:

| Option | Changing the set | Choose when |
|---|---|---|
| `text` plus CHECK | Drop and re-add the constraint, `NOT VALID` then `VALIDATE` | Default: states owned by code |
| Native enum | `ALTER TYPE ... ADD VALUE` is cheap; removing one needs a new type | Stable sets where ordering or 4-byte storage matters |
| Lookup table plus FK | Insert a row | Values carry a label, sort order or active flag, or users edit them |

- `jsonb` only for what is truly schemaless or rarely queried (a webhook body
  as received, per-integration settings). What you filter, join, sort, sum or
  constrain is a real column. At least `CHECK (jsonb_typeof(s) = 'object')`.
- Arrays (`text[]`) for a few tags read with the row; links to other rows are
  a join table, since array elements cannot be foreign keys.

## 4. Constraints

- **NOT NULL by default.** Nullable only when the lifecycle says so
  (`paid_at` before payment), then tied to the state with a CHECK, as above.
- **A foreign key on every reference**, ON DELETE chosen per relation:

| ON DELETE | For |
|---|---|
| `RESTRICT` or `NO ACTION` (default) | Records with history: a customer with orders, a product on invoices |
| `CASCADE` | Parts meaningless alone: order lines, a post's attachments (mind huge cascades) |
| `SET NULL` | An optional link: `assignee_id` when the user leaves |

  `NO ACTION` checks at statement end (transaction end if `DEFERRABLE
  INITIALLY DEFERRED`), `RESTRICT` at once. Index the referencing column.
- **UNIQUE** for every "only one" rule: `UNIQUE (shop_id, sku)`; one active
  subscription per user as a partial unique index (`ON subscriptions
  (user_id) WHERE status = 'active'`). NULLs are distinct by default;
  `UNIQUE NULLS NOT DISTINCT` (PG 15+) when they should collide.
- **CHECK** for ranges and relations: `quantity > 0`, `ends_at > starts_at`,
  `discount_cents <= total_cents`, `num_nonnulls(post_id, photo_id) = 1`.
- **Exclusion constraints** for "no two overlap":

```sql
CREATE EXTENSION IF NOT EXISTS btree_gist;
CREATE TABLE bookings (
  id      bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  room_id bigint NOT NULL REFERENCES rooms (id),
  during  tstzrange NOT NULL CHECK (NOT isempty(during)),
  CONSTRAINT bookings_no_overlap EXCLUDE USING gist (room_id WITH =, during WITH &&)
);
```

  Ranges default to `[)`, so back-to-back bookings do not collide. A
  violation raises `23P01`; answer it with a 409.

## 5. Naming

- snake_case, no quoted identifiers; tables plural (`orders`, `order_items`)
  unless the project uses singular; no reserved words (`user`, `order`).
- Primary key `id`; foreign keys `<singular>_id`, or the role when two point
  at one table (`sender_id`, `recipient_id`); `paid_at`, `due_on`,
  `is_archived`; units in amounts (`total_cents`, `timeout_ms`).
- Constraints in Postgres's pattern (`orders_pkey`, `orders_customer_id_fkey`,
  `orders_sku_key`, `orders_status_check`), indexes as `idx_<table>_<columns>`.
  Postgres silently truncates names past 63 bytes; MySQL refuses past 64.

## 6. Audit columns

- `created_at timestamptz NOT NULL DEFAULT now()` and `updated_at` on every
  table that changes; `created_by` and `updated_by` referencing users where
  accountability matters (money, permissions, published content).
- An ORM's `updated_at` is skipped by raw SQL and bulk updates; a trigger is
  not (MySQL: `ON UPDATE CURRENT_TIMESTAMP(6)` on the column):

```sql
CREATE FUNCTION set_updated_at() RETURNS trigger LANGUAGE plpgsql
AS $$ BEGIN NEW.updated_at := now(); RETURN NEW; END $$;

CREATE TRIGGER orders_set_updated_at BEFORE UPDATE ON orders
  FOR EACH ROW EXECUTE FUNCTION set_updated_at();
```

## 7. Soft deletes

- Delete for real by default. `deleted_at timestamptz` only for undo, a legal
  or audit need, or rows other records must keep pointing at.
- The costs: every query must filter it (one forgotten `WHERE` lets a deleted
  user sign in); unique rules need partial indexes (`ON users (lower(email))
  WHERE deleted_at IS NULL`); foreign keys do not know the parent is gone;
  the table grows until a job purges it. Make the filter hard to forget: the
  ORM's scope (Laravel `SoftDeletes`, Rails `discard`) or a view.
- Often better: a lifecycle state (`archived`), or a hard delete plus an
  archive row (the row as `jsonb`, who, when) in the same transaction.

## 8. Multi-tenancy

| Model | Isolation | Cost | Choose when |
|---|---|---|---|
| Shared tables with `tenant_id` | Queries, plus RLS in Postgres | One schema, one migration run | Default: many small or medium tenants |
| Schema per tenant | Strong | Each migration runs N times; thousands of schemas strain the catalog | Tens to a few hundred tenants needing separation |
| Database per tenant | Strongest; own backups and region | Operations times N | Few large, regulated or data-residency tenants |

- Shared tables: `tenant_id NOT NULL` on every tenant row, leading its
  indexes, inside every unique rule (`UNIQUE (tenant_id, slug)`), with
  row-level security as the second wall (`database-postgres`). Composite
  foreign keys make a cross-tenant reference impossible:

```sql
ALTER TABLE projects ADD CONSTRAINT projects_tenant_id_id_key UNIQUE (tenant_id, id);
ALTER TABLE tasks ADD CONSTRAINT tasks_tenant_project_fkey
  FOREIGN KEY (tenant_id, project_id) REFERENCES projects (tenant_id, id);
```

## 9. Relationships

- One-to-many: the FK on the many side, indexed. One-to-one: an FK with
  UNIQUE, or the child's primary key is the parent's id.
- Many-to-many: a join table with a composite key and an index for the other
  direction. Once it carries attributes it is an entity; name it so.

```sql
CREATE TABLE project_members (
  project_id bigint NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
  user_id    bigint NOT NULL REFERENCES users (id) ON DELETE CASCADE,
  role       text NOT NULL CHECK (role IN ('owner', 'editor', 'viewer')),
  added_at   timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (project_id, user_id)
);
CREATE INDEX idx_project_members_user_id ON project_members (user_id);
```

- Polymorphic pairs (`commentable_type`, `commentable_id`) cannot have a
  foreign key. Instead: one nullable FK per parent with the `num_nonnulls`
  CHECK, a join table per parent, or a supertype table the parents share.

## 10. Hierarchies

| Pattern | Subtree read | Move | Choose when |
|---|---|---|---|
| `parent_id` plus recursive CTE | Recursion | One row | Default |
| `ltree` path (Postgres) | Index (`path <@ 'shop.phones'`) | Rewrite the subtree's paths | Deep trees read by subtree often |
| Closure table (ancestor, descendant, depth) | Index | Delete and insert pairs | Heavy reads, any engine |
| Nested sets | Fast | Rewrites half the table | Never |

```sql
WITH RECURSIVE tree AS (
  SELECT id, parent_id, name, 1 AS depth FROM categories WHERE id = $1
  UNION ALL
  SELECT c.id, c.parent_id, c.name, t.depth + 1
  FROM categories c JOIN tree t ON c.parent_id = t.id
  WHERE t.depth < 20            -- stops a cycle from running forever
)
SELECT * FROM tree;
```

## 11. History and documentation

- **Event tables** where history is the truth (`ledger_entries`,
  `stock_movements`): append-only, the current value checkable against them.
- **History tables** for "what did this row look like": the old row with
  `valid_from`, `valid_to`, `changed_by`, written by a trigger so no path
  skips it (or `paper_trail`, `django-simple-history`, `laravel-auditing`).
- **Temporal rows** for prices by period: `valid_during tstzrange` with an
  exclusion constraint per product.
- `COMMENT ON COLUMN` for what a name cannot say (units, what NULL means,
  what it derives from); an ER diagram generated from the live schema
  (`tbls`, SchemaSpy, `prisma-erd-generator`), never drawn by hand.

## Check it

- Apply to a scratch database and read it back with `\d+ orders` (psql).
- Insert the rows that must fail: a NULL, an orphan, a duplicate, a negative
  quantity, an overlapping booking. Each is refused with its code.
- Review nullable columns and suspect types one by one:

```sql
SELECT table_name, column_name, data_type, is_nullable
FROM information_schema.columns
WHERE table_schema = 'public'
  AND (is_nullable = 'YES'
       OR data_type IN ('real', 'double precision', 'money', 'timestamp without time zone'));
```

- Foreign keys without an index: the query in `database-indexes`.

## Avoid

Floats or `money` for amounts; timestamps without a zone; UUIDs in
`varchar(36)`; random UUIDv4 keys on hot tables; natural primary keys;
nullable by default; references without foreign keys; ON DELETE left to
chance; uniqueness checked only in code; `status` as free text; business
data hidden in `jsonb`; polymorphic type and id pairs; nested sets; soft
deletes by reflex; tenant rows whose unique rules forget `tenant_id`; today's
price joined onto last year's invoice.
