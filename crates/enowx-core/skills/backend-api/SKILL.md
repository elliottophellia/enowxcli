---
name: backend-api
description: "Designing an HTTP API and its contract: routes and methods, status codes, validating input, response shapes, pagination, filtering, idempotency, versioning, CORS, uploads. Read before adding or changing endpoints."
---

# The API and its contract

One part of the backend; the whole is in the `backend` skill. Follow the
project's existing style first (REST, RPC, GraphQL, tRPC, server actions); the
rules below are for REST over JSON and carry over to the others where they
apply.

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
| Conflicts with the current state (a duplicate, already paid, stale version) | `409` |
| Too many requests | `429`, with `Retry-After` |
| Our fault | `500`; `503` when a dependency is down |

`GET` never changes anything. `PUT` replaces the whole record, `PATCH`
changes the fields sent; most edits are `PATCH`.

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
  ISO 8601 with a time zone and ids in their real type.
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
  (`{ "amount": 1500000, "currency": "IDR" }`), never a float.
- Return only the fields the caller may see: build the response from a
  schema or a resource class, never by serialising the database row (it
  leaks password hashes, internal flags and future columns).
- `null` for a known empty value; leave a field out only when the contract
  says it is optional.

## 5. Lists

- Every list is paginated in the query: `?limit=` (default 25, maximum 100)
  and `?cursor=` for long or growing lists, or `?page=` with a `total` when
  the caller jumps to a page. The cursor is opaque (an encoded sort key and
  id), and the order is stable (the sort column, then the id).
- Filters and sort on named, allowed fields only: `?status=paid&sort=-createdAt`.
  Unknown filter or sort fields are a `400`, not ignored, and never pasted
  into SQL.
- Search with a minimum length and the same limit.

## 6. Writes that must happen once

- A create that must not repeat (an order, a payment, a message sent) takes
  an `Idempotency-Key` header: the first response is stored with the key for
  24 hours and returned again for the same key.
- Edits that two people can make at once carry a version (`version` or
  `updatedAt` in the body, or `If-Match` with an `ETag`); a stale one gets
  `409`.

## 7. Change and compatibility

- Adding a field or an endpoint is safe. Renaming, removing or changing a
  field's meaning breaks callers: add the new one, keep the old until
  callers have moved, then remove it. A public API carries a version
  (`/v1/...`).
- Describe the API where it cannot drift: OpenAPI generated from the schemas
  (zod-to-openapi, FastAPI's own, Scribe or Scramble for Laravel), or the
  shared types module for a client in the same repository.

## 8. Around the endpoints

- CORS lists the origins that may call; never `*` together with cookies or
  credentials.
- Uploads: a size limit, the type checked from the content rather than the
  extension, a generated file name, storage outside the web root (object
  storage with signed URLs for anything large), never executed or served
  with the uploaded content type unchecked.
- Rate limits on sign-in, sign-up, password reset, anything that sends email
  or costs money (`backend-security`).

## Avoid

Verbs in paths; `200` with an error inside; a body written to the database
as parsed; a database row as the response; floats for money; dates without a
zone; a list with no limit; sort or filter fields concatenated into SQL; a
create that runs twice when the client retries; a breaking change shipped
under the same version.
