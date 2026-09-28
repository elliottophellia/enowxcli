---
name: backend-errors
description: "Handling errors in a backend: one error format, mapping failures to status codes in one place, what the client sees versus what the log keeps, validation errors, retryable failures, messages through i18n. Read when adding endpoints or changing how failures are reported."
---

# Errors

One part of the backend; the whole is in the `backend` skill.

## 1. One format

Every error response has the same shape. Keep the project's; with none, use
problem details (RFC 9457) as `application/problem+json`:

```json
{
  "type": "https://example.com/errors/out-of-stock",
  "title": "Out of stock",
  "status": 409,
  "code": "out_of_stock",
  "detail": "Only 2 left of Kopi Susu 250ml.",
  "errors": [{ "field": "items[0].quantity", "code": "too_many" }]
}
```

- `code` is stable and machine-readable (`out_of_stock`, `email_taken`); the
  client decides what to show from it, never from `detail`.
- `errors` lists every invalid field at once for a validation failure.
- A request id is in a header (`X-Request-Id`) on every response, so a user's
  report can be matched to the log.

## 2. Mapped in one place

- The service raises domain errors that say what went wrong in the domain's
  words: `NotFound`, `Forbidden`, `Conflict("already_paid")`,
  `Validation(fields)`, `RateLimited`. It does not know about HTTP.
- One error handler (middleware, an exception handler, `IntoResponse`, the
  framework's renderer) turns them into status codes and the format above.
  Handlers do not build error responses by hand, each a little differently.
- Anything unexpected becomes a `500` with a generic message and the request
  id; the details go to the log only.

## 3. What each side sees

- The client: the status, the code, a message it can show, the fields.
  Never a stack trace, an SQL statement or error, a file path, a library
  name, a hostname or a secret. In development the details can be shown; the
  switch is the environment, not a flag someone forgets.
- The log: everything, once, at the boundary: the error with its cause
  chain and stack, the request id, the route, the user id. Not the request
  body when it can hold passwords, tokens or card numbers.
- Errors that are the caller's fault (`4xx`) are logged at `info` or `warn`,
  ours (`5xx`) at `error`. A flood of `4xx` is a signal, not an incident.

## 4. Do not hide failures

- No empty `catch`, and no `catch` that logs and carries on as if it
  worked. Handle an error where something useful can be done (a retry, a
  fallback, a clear message); otherwise let it reach the handler.
- Keep the cause when wrapping (`cause`, `from`, `%w`, `#[source]`), so the
  log shows the whole chain.
- Existence checks do not leak: "no account with that email" is the same
  response as "wrong password" on sign-in, and a record the caller may not
  see is a `404`, not a `403`, when its existence is itself private.

## 5. Failures that can be retried

- Say which ones: `429` and `503` carry `Retry-After`; a timeout or a lost
  connection to a dependency is a `503` or `504`, not a `500`.
- A write that may have happened before a failure is made safe to repeat
  (idempotency keys in `backend-api`), so the client's retry does not double
  it.
- Time limits on everything the request waits for: the database query, each
  outbound call (`backend-integrations`).

## 6. Messages people read

- Messages for users go through i18n (the `i18n` skill): the server sends the
  `code` and the parameters (`{ "code": "out_of_stock", "left": 2 }`), and the
  message is looked up in the user's language by whoever renders it. When the
  server renders text itself (emails, server-rendered pages), it uses the
  catalogue too.
- Messages say what happened and what to do: "The code has expired. Request a
  new one." Not "Error 1043" and not "Something went wrong" when the cause is
  known.
- Messages for developers (logs, exceptions) stay in English.

## Avoid

`200` with `success: false`; a different error shape per endpoint; the stack
trace in the response; `catch (e) { console.log(e) }` and carry on; an error
message built from an exception's text and sent to the user; "Something went
wrong" for a failure whose cause is known; a sign-in that says which half was
wrong.
