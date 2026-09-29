---
name: backend-api
description: "Designing an HTTP API and its contract: routes and methods, status codes, validating input, response shapes, pagination, filtering, sorting and sparse fields, bulk and long-running operations, conditional requests, idempotency keys, rate limit headers, versioning and deprecation, OpenAPI, API keys or OAuth for third parties, content negotiation, CORS, uploads. Read before adding or changing endpoints."
---

# The API and its contract

The generated API has verbs in its paths, `200` with an error inside, lists
with no limit, a create that runs twice on a retry, and a field renamed
under the same version. One part of the backend; the whole is in `backend`.
Follow the project's existing style first (REST, RPC, GraphQL, tRPC, server
actions); these rules are for REST over JSON and carry over where they fit.

## 1. Routes

- Resources as plural nouns, the action in the method: `GET /orders`,
  `POST /orders`, `GET /orders/{id}`, `PATCH /orders/{id}`,
  `DELETE /orders/{id}`. Not `/getOrders` or `/orders/create`.
- One level of nesting where the child belongs to the parent
  (`/orders/{id}/items`); deeper than that, give the child its own route.
- An action that is not create, read, update or delete becomes a sub-resource
  named as a noun or a verb on the record: `POST /orders/{id}/refunds`,
  `POST /orders/{id}/cancel`.
- One case everywhere, the project's: `kebab-case` paths, and `camelCase` or
  `snake_case` fields, never mixed.
- `/me` for the caller's own things (`GET /me/orders`). Nothing private in a
  path or query string (emails, tokens): URLs land in logs and history.

## 2. Methods and status codes

| Situation | Status |
|---|---|
| Read, or update that returns the record | `200` |
| Created | `201`, with the record and a `Location` header |
| Accepted for later (a job was queued) | `202`, with where to check |
| Done, nothing to return | `204` |
| Malformed or invalid input | `400` (or `422` if the project uses it for validation) |
| Not signed in, or the token is bad | `401` |
| Signed in but not allowed | `403`; `404` when even revealing the record exists would leak |
| No such record | `404` |
| A method the path does not take | `405`, with an `Allow` header |
| Conflicts with the current state (a duplicate, already paid, stale version) | `409` |
| `If-Match` no longer matches; `If-Match` required but missing | `412`; `428` |
| Body over the size limit; a content type the endpoint does not take | `413`; `415` |
| Too many requests | `429`, with `Retry-After` |
| Our fault | `500`; `503` when a dependency is down |

`GET` never changes anything. `PUT` replaces the whole record, `PATCH`
changes the fields sent; most edits are `PATCH`, with merge semantics
(RFC 7396): a field left out stays as it is, `null` clears it.

## 3. Input

- Validate every input at the boundary with the project's schema library
  (zod or valibot, pydantic, a Form Request, serde with validation): body,
  query string, path parameters and headers you read.
- Refuse unknown fields, or strip them, but never pass the parsed body to the
  database as it is: copy the fields a caller may set, so nobody sets
  `role`, `price` or `ownerId` by adding them to the JSON (mass assignment).
- Types and limits in the schema: required or not, string lengths, number
  ranges, formats (email, URL, UUID), array sizes. A request body over a set
  size (1 MB for JSON unless the feature needs more) is refused before it is
  parsed.
- Trim strings; normalise what compares (emails lowercased); parse dates as
  ISO 8601 with a time zone and ids in their real type. Bodies arrive as
  `application/json` or get `415`; one that does not parse is a `400`
  (`malformed_json`), never a `500`.
- Report every field that failed in one response, with a stable code per
  field, so a form can mark all of them at once (the format is in
  `backend-errors`).

## 4. Responses

- One shape everywhere. A record is returned as itself
  (`{ "id": "ord_…", "status": "paid", ... }`); a list as
  `{ "items": [...], "nextCursor": "…" }` (or with `total` when paging by
  number). Keep the project's shape if it already has one.
- Ids as strings. Times as ISO 8601 in UTC (`2026-09-29T10:15:00Z`). Money as
  an integer in the smallest unit with its currency
  (`{ "amount": 1500000, "currency": "IDR" }`), never a float. Enums as
  lowercase strings, documented as a list that can grow.
- Return only the fields the caller may see: build the response from a
  schema or a resource class, never by serialising the database row (it
  leaks password hashes, internal flags and future columns).
- `null` for a known empty value; leave a field out only when the contract
  says it is optional. A create or update returns the record as saved.

## 5. Lists

- Every list is paginated in the query: `?limit=` (default 25, maximum 100)
  and `?cursor=` for long or growing lists, or `?page=` with a `total` when
  the caller jumps to a page. The cursor is opaque (an encoded sort key and
  id), and the order is stable (the sort column, then the id). A `total` is
  a count on every page: offer it with page numbers, not on large tables.
- Filters as named parameters in the schema: `?status=paid,refunded` (OR
  within a field, AND across fields), ranges as a half-open pair
  (`?createdFrom=2026-09-01T00:00:00Z&createdTo=2026-10-01T00:00:00Z`, from
  inclusive, to exclusive), search as `?q=`. Keep a bracket style
  (`?total[gte]=1000`) where the project already has one.
- Sort as `?sort=-createdAt,name`: a list, `-` for descending, allowed
  fields only, the id added as the last key so pages never repeat or skip.
- Unknown filter or sort fields are a `400`, not ignored, and never pasted
  into SQL. Search with a minimum length and the same limit.
- Sparse fields (`?fields=id,status`) only for large records; related
  records with `?expand=customer` from an allowed list, one level deep,
  loaded in one query. Small APIs need neither.

## 6. Bulk endpoints

- Hundreds of writes in one call: `POST /products/batch` with
  `{ "items": [...] }`, a documented maximum (100 to 1000), `413` past it,
  and an idempotency key.
- All or nothing by default: one transaction, and a failure lists each bad
  item by index (`items[3].sku`). When items are independent and the client
  can retry the failures, answer `200` with a result per item, in order
  (`{ "results": [{ "index": 1, "status": 409, "code": "sku_taken" }] }`).
- Deletes as `POST /orders/batch-delete` with the ids (a body on `DELETE`
  is dropped by some proxies). Past a few thousand rows it is an import: an
  upload and a job (section 7).

## 7. Long-running operations

Work that takes more than a few seconds (an export, an import, a report)
answers at once with a status resource:

```http
POST /exports              202 Accepted, Location: /exports/exp_31
GET  /exports/exp_31       200 { "id": "exp_31", "status": "running", "progress": 40 }
GET  /exports/exp_31       200 { "id": "exp_31", "status": "succeeded", "resultUrl": "…" }
```

- States `queued`, `running`, `succeeded`, `failed` (with a problem object),
  `cancelled`; `Retry-After: 3` on the status response paces the polling.
- The result is a resource or a signed URL (`backend-files`); cancel with
  `POST /exports/{id}/cancel`; the status is kept for days. Webhooks or SSE
  (`backend-realtime`) can tell the client sooner; polling always works.

## 8. Writes that must happen once

- A create that must not repeat (an order, a payment, a message sent) takes
  an `Idempotency-Key` header: the first response is stored with the key for
  24 hours and returned again for the same key. The client makes one key (a
  UUID) per operation and resends it only when retrying that operation.

```sql
CREATE TABLE idempotency_keys (
  caller_id    bigint      NOT NULL,
  key          text        NOT NULL CHECK (length(key) <= 255),
  request_hash text        NOT NULL,
  status       smallint,              -- null while the first request runs
  response     jsonb,                 -- body and the headers that matter
  created_at   timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (caller_id, key)
);
```

- Same key and request (hash of method, path and body), finished: replay
  the stored status, `Location` and body exactly, with
  `Idempotent-Replayed: true`. Still running: `409` with `Retry-After: 1`.
  Different request: `422`. Missing where required: `400`. A `400` for bad
  input is not stored, so the fixed request can reuse the key; rows past 24
  hours are deleted by a scheduled job.
- Work that stays in your database inserts the key in the same transaction:
  a rollback removes both, and the retry runs fresh. Work that calls a
  provider commits the key first (so a concurrent retry sees `409`) and
  passes a key derived from it (`<key>:charge`) to the provider's own
  idempotency, so a crash between the call and the save is safe to retry.

## 9. Edits that two people can make at once

- The record carries a `version` (`backend-data`), returned in the body and
  as a strong `ETag` (`"7"`); a weak one (`W/"7"`) never matches `If-Match`.
- The update sends it back, one way per API: `If-Match: "7"` (`412` when
  stale, `428` when required but missing), or `version` in the body (`409`,
  `stale_version`). The SQL update is conditional
  (`WHERE id = $1 AND version = $2`), not a check before it; the client
  shows the current record rather than overwriting it.
- `If-None-Match` on a `GET` with the `ETag` gets `304` (`backend-caching`).

## 10. Rate limit headers

Every `429` carries `Retry-After` in seconds. The quota left goes in the
IETF draft fields or the `X-RateLimit-Limit`, `-Remaining`, `-Reset` trio
(one, documented, exposed through CORS). Limits per key or user, stricter
per IP when anonymous (`backend-security`).

```http
HTTP/1.1 429 Too Many Requests
Retry-After: 30
RateLimit-Policy: "default";q=100;w=60
RateLimit: "default";r=0;t=30
```

## 11. Change, versions and deprecation

- Adding a field or an endpoint is safe. Renaming, removing or changing a
  field's meaning breaks callers: add the new one, keep the old until
  callers have moved, then remove it. A public API carries a version
  (`/v1/...`). Also breaking: a new required input, a changed type, format
  or default, tighter validation, another status or error code.

| Strategy | Example | Use |
|---|---|---|
| Path | `/v1/orders` | The default for public APIs: visible in logs, easy to route and test; never `?version=` |
| Date-pinned header | `Api-Version: 2026-09-01`, pinned per account | A large public API changing often; needs a layer translating old shapes |
| Media type | `Accept: application/vnd.example.v2+json` | Rarely: invisible in URLs and awkward with curl |
| None | Your own frontend is the only client | Expand and contract, server deployed first |

- On its way out, it says so on every response (`Deprecation`, RFC 9745;
  `Sunset`, RFC 8594); usage per caller is watched, callers are told, and
  it goes after the date (6 to 12 months for a public API):

```http
Deprecation: @1788220800
Sunset: Tue, 01 Jun 2027 00:00:00 GMT
Link: <https://example.com/docs/migrate-to-v2>; rel="deprecation"
```

## 12. OpenAPI as the contract

- Describe the API where it cannot drift: OpenAPI generated from the schemas
  (zod-to-openapi, FastAPI's own, Scribe or Scramble for Laravel,
  `@fastify/swagger`, springdoc-openapi, ASP.NET Core's, utoipa, huma), or
  the shared types module for a client in the same repository; spec first
  (oapi-codegen, openapi-typescript) when it is agreed before the code.
- OpenAPI 3.1: each operation with an `operationId`, schemas, the problem
  responses, the security scheme and an example; served at `/openapi.json`
  and rendered in the docs (`docs-api`). In CI, lint it, fail on breaking
  changes against the main branch, and fuzz the running API against it:

```sh
npx @redocly/cli lint openapi.json
oasdiff breaking base-openapi.json openapi.json --fail-on ERR
schemathesis run http://localhost:3000/openapi.json
```

## 13. Around the endpoints

- Third parties: a developer's server acting for its own account uses an
  API key in `Authorization: Bearer sk_live_…`, never the query string
  (`backend-auth`); an app acting for your users uses OAuth 2 (code flow,
  PKCE, scopes named for actions such as `orders:read`, revocable grants),
  never a pasted password or key. Events go out as signed webhooks
  (`backend-integrations`).
- JSON in and out; errors as `application/problem+json`; an `Accept` you
  cannot produce gets `406`, or JSON anyway if the project does that.
- gzip or Brotli for responses over about 1 KB (at the proxy or CDN when
  there is one), with `Vary: Accept-Encoding`; not for images or archives.
- CORS lists the origins that may call; never `*` together with cookies or
  credentials. The origin is matched exactly against config and echoed with
  `Vary: Origin`; `Access-Control-Max-Age: 7200` (Chromium's cap; without it
  a preflight is cached 5 seconds and precedes nearly every call);
  `Access-Control-Expose-Headers` for what the client reads (`Location`,
  `ETag`, `Retry-After`, `X-Request-Id`, the rate limit fields).
- Uploads: a size limit, the type checked from the content rather than the
  extension, a generated name, storage outside the web root (object storage
  and signed URLs when large), never executed or served with the uploaded
  type unchecked (`backend-files`).
- Rate limits on sign-in, sign-up, password reset, anything that sends email
  or costs money (`backend-security`).

## Check it

- `curl -si` each new endpoint: the status, `Location` on a create, the
  problem body on a failure; `?limit=1000` clamped, `?sort=passwordHash` 400.
- The same create sent twice with one `Idempotency-Key` makes one record and
  replays the same body; a `PATCH` with an old `If-Match` gets `412`.
- A preflight (`curl -si -X OPTIONS` with `Origin` and
  `Access-Control-Request-Method`) shows the origin and max age; the OpenAPI
  lint and breaking-change diff pass, or the break is in the report.

## Avoid

Verbs in paths; `200` with an error inside; a body written to the database
as parsed; a database row as the response; floats for money; dates without a
zone; a list with no limit; sort or filter fields concatenated into SQL; a
create that runs twice when the client retries; a weak `ETag` for
`If-Match`; personal data in URLs; a breaking change shipped under the same
version; an endpoint removed without `Sunset` and a date.
