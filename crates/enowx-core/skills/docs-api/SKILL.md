---
name: docs-api
description: "API reference documentation: OpenAPI as the source of truth for HTTP APIs, generated reference with real examples, authentication, errors, pagination, rate limits and versioning explained once, request and response examples in curl and the main languages, SDK docs, library API docs with doc comments and tested examples, and GraphQL schemas. Read before documenting an API or a library's public interface."
---

# API reference people can build against

The generated reference lists every route with each field typed `string`
and an example value of `"string"`, no error responses, authentication
described three different ways, pagination left to guess, and a spec that
stopped matching the code releases ago; its library docs say
`@param id the id`. This skill: one contract that cannot drift, entries
with real requests and responses, the concepts every call shares explained
once, and doc comments whose examples are tested. The server side of these
rules is in `backend-api` and `backend-errors`.

## 1. One source of truth for an HTTP API

The OpenAPI document is the contract: version 3.1, or 3.2 once your tools
support it. Choose one direction and keep to it:

- **Code first**: the framework generates the document from handlers and
  schemas. FastAPI; NestJS with `@nestjs/swagger`; zod with
  `@asteasolutions/zod-to-openapi`, or Hono's `@hono/zod-openapi`; utoipa
  or aide in Rust; huma in Go; Scramble or Scribe in Laravel;
  drf-spectacular for Django REST framework; springdoc-openapi;
  `Microsoft.AspNetCore.OpenApi` in .NET 9 and later.
- **Spec first**: you write `openapi.yaml` and generate server interfaces
  and clients from it: oapi-codegen or ogen for Go, openapi-typescript for
  TypeScript types, Orval or Hey API for clients, openapi-generator for
  the rest.
- Never both, and never a hand-kept spec beside code that does not read
  it.

CI keeps it honest:

```sh
npx @redocly/cli lint openapi.yaml       # or: spectral lint openapi.yaml
git show origin/main:openapi.yaml > base.yaml
oasdiff breaking base.yaml openapi.yaml --fail-on ERR
```

- Code first, CI regenerates the document and fails when the committed
  copy differs (`git diff --exit-code openapi.yaml`); spec first, contract
  tests call the running API with it (Schemathesis).
- Render it with Scalar (reference plus a request client), Redoc (three
  panels, read only) or Stoplight Elements, served from the docs site;
  Swagger UI for internal "try it" pages. Hosted platforms (Mintlify,
  ReadMe, Bump.sh) read the same file.
- In 3.1 a nullable field is `type: [string, "null"]`, and schema examples
  are `examples: [...]`; `nullable` and `example` are the 3.0 forms.

## 2. Every operation

- `operationId`: stable and verb first (`createOrder`). SDK generators name
  methods after it, so renaming it breaks users.
- `summary`: the action in a few words ("Create an order"). `description`:
  what it does, its side effects (sends an email, charges a card, starts a
  job), the scope or role it needs, whether it is idempotent.
- Every parameter and field: a description, type and format, constraints
  (`minimum`, `maxLength`, `pattern`, `enum`), default, required or not.
  The description says what the name cannot: units, time zone, what `null`
  means.
- Responses: the success, and every error the operation really returns,
  as shared components.
- Examples with realistic values (never `string`, `0` or `true`),
  consistent across operations: the order created in one example is the
  one fetched in the next.
- `deprecated: true`, with the replacement and the removal date in the
  description. Tags per resource, in the order a reader meets them.

```yaml
paths:
  /orders:
    post:
      operationId: createOrder
      summary: Create an order
      description: |
        Creates an order in `pending` state. Needs the `orders:write` scope.
        Send an `Idempotency-Key` header to retry safely.
      tags: [Orders]
      security: [{ bearerAuth: [] }]
      parameters:
        - $ref: "#/components/parameters/IdempotencyKey"
      requestBody:
        required: true
        content:
          application/json:
            schema: { $ref: "#/components/schemas/OrderCreate" }
            examples:
              twoShirts:
                summary: Two shirts, standard shipping
                value:
                  customerId: cus_4Qm8Zt
                  items: [{ sku: TEE-BLK-M, quantity: 2 }]
      responses:
        "201":
          description: Created. `Location` holds the order's URL.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/Order" }
        "422": { $ref: "#/components/responses/ValidationFailed" }
        "429": { $ref: "#/components/responses/RateLimited" }
```

## 3. What every call shares, explained once

A guide page for each, linked from every operation it applies to:

- **Authentication**: how to get credentials (where, and which role may
  create them), how to send them (`Authorization: Bearer <token>`), test
  and live keys, each scope and what it allows, expiry and rotation, and
  what `401` means next to `403`. One whole request that works with a test
  key.
- **Errors**: the format, ideally RFC 9457 problem details (`type`,
  `title`, `status`, `detail`, `instance`, plus fields such as a list of
  per-field `errors`), and a table of every error code: status, meaning,
  what the caller should do. Each `type` URL opens its row.
- **Pagination**: `limit` with its default and maximum, `cursor`, the
  response fields, the sort order, and a loop that fetches every page.
- **Rate limits**: the limits if they are public, the headers sent
  (`Retry-After`, and `RateLimit` with `RateLimit-Policy` from the IETF
  draft, or `X-RateLimit-*`), and what a client does on `429`: wait for
  `Retry-After`, then back off with jitter.
- **Idempotency**: which operations take `Idempotency-Key`, how long keys
  are kept, what a reused key with a different body returns.
- **Versioning and deprecation**: the scheme (a `/v1` path, or a dated
  version header such as Stripe's `Stripe-Version`), what counts as
  breaking, the notice period, how a deprecation is announced (the
  `Deprecation` header of RFC 9745 and `Sunset` of RFC 8594, a message to
  the keys still calling it), and the API's own dated changelog
  (`docs-changelog`; `oasdiff changelog` drafts one from two versions).
- **Webhooks**: each event type with an example payload; delivery (method,
  timeout, which status counts as received); signature verification with
  working code (HMAC-SHA256 over the raw body, a constant-time comparison,
  a timestamp tolerance such as 5 minutes); the retry schedule; no
  ordering guarantee; duplicates dropped by event id; receiving them
  locally. OpenAPI 3.1 describes payloads under `webhooks`; the Standard
  Webhooks spec gives header names and a signing scheme to follow.
- **Environments**: base URLs for production and sandbox, in `servers`. A
  "try it" console sends to the sandbox, never to production.

## 4. Code samples

- curl first, since everyone has it; then the languages your users write,
  through your SDKs when you have them.
- One option per line, the secret from an environment variable, the full
  URL. Send JSON with `-H` and `-d`: `--json` needs curl 7.82 or later.

```sh
curl https://api.example.com/v1/orders \
  -H "Authorization: Bearer $ACME_API_KEY" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: 5f0c9a8e-2b7d-4e21-9c3a-6d1f8b4e7a20" \
  -d '{"customerId": "cus_4Qm8Zt", "items": [{"sku": "TEE-BLK-M", "quantity": 2}]}'
```

- Then the response: the status line and the body, trimmed to what
  matters. Samples come from the spec or run in tests; a sample that no
  longer works is worse than none.

## 5. SDK documentation

- The reference generated from the types (TypeDoc, rustdoc, mkdocstrings
  or Sphinx, pkg.go.dev), plus pages written by hand: install, creating the
  client (configuration, timeouts, retries), authentication, pagination
  helpers, errors (which error type maps to which API error), webhook
  verification, and an upgrade guide per major version.
- Generated SDKs (Speakeasy, Stainless, Fern, openapi-generator, Kiota)
  ship a default README: rewrite its introduction and examples.

## 6. Doc comments on a library's public interface

Every public item gets one. The first sentence is the summary (lists and
search show only it). Then what the signature does not say: behaviour,
parameters and return value when names and types are not enough, errors
and when they happen, panics, safety conditions, an example, and the
version it arrived in for newer items. Never restate the types: no
`@param {string} text` in TypeScript, where the signature has the type.

| Language | Syntax | Sections | Examples run by |
|---|---|---|---|
| Rust | `///`, and `//!` for a module | `# Examples`, `# Errors`, `# Panics`, `# Safety` | `cargo test --doc` |
| Python | Docstrings, Google or NumPy style | `Args:`, `Returns:`, `Raises:`, `Examples:` | `pytest --doctest-modules` |
| TypeScript | TSDoc `/** */` | `@param name - text`, `@returns`, `@throws`, `@example` with a fenced block, `@deprecated`, `{@link}` | Example files checked by `tsc` |
| Go | `//` starting with the item's name | A `Deprecated:` paragraph | `func ExampleName()` with `// Output:` |
| Java | Javadoc `/** */`, or `///` Markdown from JDK 23 | `@param`, `@return`, `@throws`, `{@snippet}` | Snippets from compiled files |
| C# | `///` XML | `<summary>`, `<param>`, `<returns>`, `<exception>` | Tests beside the examples |

```rust
/// Parses a duration such as `90s`, `15m` or `2h` into seconds.
///
/// # Errors
///
/// Returns [`ParseError::Unit`] when the suffix is not `s`, `m` or `h`.
///
/// # Examples
///
/// ```
/// assert_eq!(timeparse::parse_seconds("15m")?, 900);
/// # Ok::<(), timeparse::ParseError>(())
/// ```
pub fn parse_seconds(input: &str) -> Result<u64, ParseError> {
    // ...
}
```

## 7. Tested, linted, published

- Doctests: `cargo test` runs a library crate's examples (the hidden
  `# Ok::<...>` line lets an example use `?`); `pytest --doctest-modules`
  runs docstrings, and `--doctest-glob="*.md"` Markdown pages; `go test`
  runs each `Example` function with an `// Output:` comment, which
  pkg.go.dev also shows. TypeScript has no doctest: keep examples as files
  under `examples/` that `tsc` checks, and include them in the docs.
- Missing docs are reported, and CI fails on the report:
  `#![warn(missing_docs)]` and clippy's `missing_errors_doc` and
  `missing_panics_doc` in Rust; ruff's `D` rules with
  `convention = "google"` in Python; revive's `exported` rule in Go;
  `GenerateDocumentationFile` (warning CS1591) in C#.
- Broken references fail the build too: `sphinx-build -W`, TypeDoc's
  `--treatWarningsAsErrors`, and in Rust
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`.
- Changes to the public surface are reviewed on purpose: API Extractor's
  `.api.md` report for TypeScript, `cargo semver-checks` for Rust.

## 8. GraphQL and other contracts

- Descriptions in the schema, on types, fields, arguments and enum values:
  GraphiQL and every client's tooling show them.

```graphql
"""A customer's order. Amounts are in the currency's smallest unit."""
type Order {
  id: ID!
  "Moves from PENDING to PAID or CANCELLED, never back."
  status: OrderStatus!
  customerName: String @deprecated(reason: "Use customer { name }. Removed after 2027-03-31.")
}
```

- Example operations for the main tasks, with variables and the response;
  the error shape (the values of `errors[].extensions.code`) and partial
  data.
- Pagination (Relay connections: `first`, `after`, `edges`, `node`,
  `pageInfo`) and the depth or cost limits, explained once. GraphiQL or
  Apollo Sandbox to explore; SpectaQL or Magidoc for a static reference.
- Protobuf and gRPC: a comment on every service, rpc, message and field,
  enforced by `buf lint`'s comment rules, rendered by protoc-gen-doc or the
  Buf Schema Registry. Event streams: AsyncAPI 3.0.

## Check it

- The linter passes, `oasdiff breaking` shows only the changes you meant,
  and the rendered reference builds.
- Every operation has a summary, a description, examples and its error
  responses; no example value is `string`, `0` or `foo`.
- Every curl sample ran against a test environment, or your report says
  it did not.
- Doctests and example tests pass (`cargo test --doc`,
  `pytest --doctest-modules`, `go test ./...`), and the missing-docs lints
  are clean.
- Authentication, errors, pagination, rate limits and versioning are each
  explained on one page and linked, never repeated.

## Avoid

A hand-written spec drifting beside the code; examples of `"string"`;
error responses left out; authentication explained three ways; field
descriptions that repeat the field name; untested samples;
`@param id the id`; renaming an `operationId` or a public item without a
deprecation; limits, prices or guarantees the owner never stated.
