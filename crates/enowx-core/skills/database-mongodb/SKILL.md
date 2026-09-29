---
name: database-mongodb
description: "MongoDB done right: modelling documents by how they are read and written, embedding versus referencing, bounded arrays, schema validation, compound indexes by the equality-sort-range rule, the aggregation pipeline, transactions, write and read concerns, and when a relational database fits better. Read when the project uses MongoDB."
---

# MongoDB

The generated MongoDB: collections shaped like SQL tables with `$lookup` on
every read, or the opposite, a user document holding every order ever
placed in an array that grows toward 16 MB; no validation, so `price` is a
number in one document and a string in the next; an index per field and
none matching the sort; `skip(50000)` for page 1,001; multi-document writes
without a transaction; reads sent to lagging secondaries. This skill is
modelling by access pattern and the settings that keep data correct. When
the data is strongly relational, section 10 and `database` say why Postgres
may fit better.

## 1. Versions and drivers

- MongoDB 8.0 is the current major (7.0 still common); Atlas also runs the
  rapid releases. Always a replica set, even of one node: transactions and
  change streams need it. `db.version()`, `rs.status()`.
- The official drivers (Node `mongodb`, PyMongo with its async client, Go,
  Java) or an ODM on them: Mongoose, Beanie, Spring Data. One `MongoClient`
  per process; it holds the pool (`maxPoolSize` 100 by default).

## 2. Model by access pattern

- Start from the reads and writes: which screens load what together, what
  changes together, how often, and how each part grows.
- Embed what is read together, owned by the parent and bounded: an order's
  lines and shipping address.
- Reference what grows without bound, is shared, or is read on its own: a
  customer's orders, comments on a popular post, the products an order
  points to.
- A document is capped at 16 MB and 100 levels of nesting; well before that,
  big documents are slow to read, update and replicate. An array that grows
  with use (events, comments, followers) is its own collection holding the
  parent's id.

| Pattern | Use | Example |
|---|---|---|
| Extended reference | Copy the few fields always shown with a reference | `customer: { _id, name }` on the order, refreshed when the name changes |
| Subset | Embed the latest N, keep the rest apart | A product with its 10 newest reviews |
| Bucket | Many small points grouped per document | One document per sensor per hour, or a time series collection (5.0+) |
| Computed | Totals kept on write | `reviewCount`, `ratingSum` with `$inc` |
| Schema versioning | `schemaVersion` on each document; code reads both shapes while migrating | `schemaVersion: 2` |
| Outlier | Overflow documents for the rare huge parent | The account with a million followers |

Relational data spread over collections and joined with `$lookup`
everywhere is a sign it wants a relational database.

## 3. Schema validation

Without validation the data drifts, and every shape ever written must be
read forever. Validate in the database as well as the app:

```js
db.createCollection("orders", {
  validator: {
    $jsonSchema: {
      bsonType: "object",
      required: ["customerId", "status", "totalCents", "currency", "createdAt"],
      properties: {
        customerId: { bsonType: "objectId" },
        status: { enum: ["draft", "placed", "paid", "cancelled"] },
        totalCents: { bsonType: ["int", "long"], minimum: 0 },
        currency: { bsonType: "string", pattern: "^[A-Z]{3}$" },
        createdAt: { bsonType: "date" },
        items: {
          bsonType: "array",
          maxItems: 500,
          items: {
            bsonType: "object",
            required: ["productId", "qty", "unitPriceCents"],
            properties: { qty: { bsonType: ["int", "long"], minimum: 1 } }
          }
        }
      }
    }
  },
  validationLevel: "strict",
  validationAction: "error"
});
```

- Integers: the Node driver stores whole numbers as `int` or `double`
  depending on size, so allow `["int", "long"]` rather than `long` alone.
  Money as integer minor units or `Decimal128`, never double.
- Change validators with `collMod`; `validationLevel: "moderate"` checks only
  documents that already pass, for a gradual migration.
- In the app: Mongoose schemas (`strict` by default drops unknown fields)
  with required fields, or zod or pydantic at the boundary. The database
  validator catches the other writers: scripts, other services.
- Shape changes: migrate-mongo or a script per change, idempotent and
  batched, with `schemaVersion` so code reads old and new while it runs.

## 4. Indexes by ESR

- Compound index order: Equality fields, then Sort, then Range. For
  `find({ tenantId, status: "open", createdAt: { $gte: since } }).sort({ priority: -1 })`:

```js
db.tickets.createIndex({ tenantId: 1, status: 1, priority: -1, createdAt: 1 });
```

- A sort no index serves happens in memory, limited to 100 MB per stage
  (spilling to disk by default from 6.0).
- Covered queries: when the filter and projection use only indexed fields
  (with `_id` excluded or indexed), no document is read.
- Unique indexes for rules, partial indexes
  (`partialFilterExpression: { status: "active" }`) for sparse data, both
  together for "one active per user".
- TTL indexes delete documents a set time after a date field
  (`{ expireAfterSeconds: 86400 }`), checked every 60 seconds: sessions,
  tokens, temporary data.
- Multikey indexes on arrays allow one array field per compound index.
  Wildcard indexes only for truly dynamic fields.
- At most 64 indexes per collection, each costing writes and memory; the
  indexes in use should fit in RAM. Builds do not block reads and writes
  since 4.2 but load the server: big ones at quiet times, one at a time.
- `explain("executionStats")`: `IXSCAN`, not `COLLSCAN`; `totalKeysExamined`
  and `totalDocsExamined` close to `nReturned`; no `SORT` stage on a sorted
  query.

## 5. Aggregation pipeline

- `$match` first (only the leading `$match` and `$sort` use indexes), then
  `$project` or `$unset` to drop fields, then `$group`, `$sort`, `$limit`.
- `$lookup` sparingly, on an indexed `foreignField`, with a sub-pipeline
  that filters and projects; a lookup per row over a big collection is
  MongoDB's N+1.
- `$facet` for several summaries of one match; each result is one document,
  so under 16 MB.
- Stages may use 100 MB of memory; from 6.0 they spill to disk by default,
  before that pass `allowDiskUse: true` for big sorts and groups.
- `$merge` or `$out` to materialise a heavy report into a collection on a
  schedule, instead of running it per page view.

```js
db.orders.aggregate([
  { $match: { tenantId, status: "paid", createdAt: { $gte: start, $lt: end } } },
  { $group: { _id: "$customerId", orders: { $sum: 1 }, revenueCents: { $sum: "$totalCents" } } },
  { $sort: { revenueCents: -1 } },
  { $limit: 20 }
]);
```

## 6. Atomicity and transactions

- A single-document write is atomic, embedded arrays included. Put what must
  change together in one document, and change it with update operators,
  never read, modify in code, write:

```js
const res = await db.collection("products").updateOne(
  { _id: productId, stock: { $gte: qty } },
  { $inc: { stock: -qty } }
);
if (res.modifiedCount === 0) throw new ConflictError("out of stock");
```

- `findOneAndUpdate` with a condition to claim a job or compare a `version`;
  `upsert: true` over a unique index for create-if-missing, retried once on
  a duplicate key error (`E11000`).
- Multi-document transactions for changes that span documents: short (60
  seconds is the default limit) and small (under 1,000 documents modified,
  as the docs advise), through `withTransaction`, which retries on
  `TransientTransactionError` and `UnknownTransactionCommitResult`:

```js
const session = client.startSession();
try {
  await session.withTransaction(async () => {
    const debit = await accounts.updateOne(
      { _id: from, balanceCents: { $gte: amount } },
      { $inc: { balanceCents: -amount } },
      { session }
    );
    if (debit.modifiedCount !== 1) throw new ConflictError("insufficient funds"); // aborts
    await accounts.updateOne({ _id: to }, { $inc: { balanceCents: amount } }, { session });
  });
} finally {
  await session.endSession();
}
```

## 7. Write and read concerns

- `w: "majority"` (the default since 5.0) for data that matters: it is
  acknowledged once most members have it and survives a failover. `w: 1` can
  lose acknowledged writes when the primary fails before replicating.
- `retryWrites=true`, the default in current drivers, retries a write once
  after a network error or failover.
- Read preference `primary` (the default) for anything that decides a
  write; `secondaryPreferred` only for reads that tolerate lag, such as
  reports, with `maxStalenessSeconds` (90 at least).
- Read concern `majority` to see only data that cannot be rolled back;
  causally consistent sessions to read your own writes from secondaries.

## 8. Queries and pagination

- Page by range, not `skip`, which walks every skipped entry each time.
  Newest first, with an index `{ tenantId: 1, createdAt: -1, _id: -1 }`:

```js
db.orders.find({
  tenantId,
  $or: [
    { createdAt: { $lt: last.createdAt } },
    { createdAt: last.createdAt, _id: { $lt: last._id } }
  ]
}).sort({ createdAt: -1, _id: -1 }).limit(50);
```

- Project only the fields needed. No `$where` or server-side JavaScript; no
  unanchored `$regex` over big collections (a text index or Atlas Search).
- Operator injection: a body such as `{ "password": { "$ne": null } }`
  passed into a filter matches everything. Validate types at the boundary,
  with Mongoose `sanitizeFilter` as a second guard.
- `countDocuments(filter)` counts exactly and scans; `estimatedDocumentCount()`
  reads metadata for a whole collection.

## 9. Atlas and operations

- Atlas Search (`$search`, Lucene) for full text and fuzzy matching; Atlas
  Vector Search (`$vectorSearch`) for embeddings with filters. Both are
  separate indexes defined beside the collection.
- Change streams (`collection.watch()`) to react to writes, resumable from a
  resume token.
- Backups: Atlas continuous backup with point-in-time restore; self-managed,
  Percona Backup for MongoDB or filesystem snapshots; `mongodump` only for
  small databases. Restores tested (`database-operations`).
- Monitoring: the profiler for slow operations
  (`db.setProfilingLevel(1, { slowms: 100 })`), `db.currentOp()`,
  `rs.printSecondaryReplicationInfo()` for lag, cache and connections.

## 10. When a relational database fits better

- Many relationships queried from several directions (orders, customers,
  products, invoices, and reports across them).
- Integrity across entities: references that must exist, uniqueness across
  collections, invariants over several records.
- Ad hoc reporting with joins, or people who need SQL.

Choose MongoDB when documents are truly read and written whole, their shapes
vary, and the team already runs it well. Postgres `jsonb` covers many
schemaless needs inside a relational database (`database-postgres`).

## Check it

```js
db.orders.find(filter).sort(sort).explain("executionStats")  // IXSCAN; keys and docs near nReturned
db.getCollectionInfos({ name: "orders" })[0].options.validator // a validator exists
db.orders.aggregate([{ $indexStats: {} }])                      // accesses.ops per index: unused ones
db.orders.aggregate([{ $project: { size: { $bsonSize: "$$ROOT" } } }, { $sort: { size: -1 } }, { $limit: 5 }])
```

- The largest documents are far below 16 MB, and every growing array has a
  bound.
- A concurrent test: 20 parallel orders for a stock of 1 give one success.
- Writes that matter use `w: "majority"`; reads that decide writes go to the
  primary.

## Avoid

SQL tables copied one to one into collections with `$lookup` on every read;
unbounded arrays; documents near 16 MB; no validator; numbers stored as
strings, money as doubles; `skip` for deep pages; indexes that ignore ESR;
in-memory sorts of big results; read-modify-write in code instead of `$inc`
and conditional updates; multi-document changes without a transaction;
`w: 1` for money; secondaries deciding writes; request bodies passed
straight into filters; `mongodump` as the only backup of a large database.
