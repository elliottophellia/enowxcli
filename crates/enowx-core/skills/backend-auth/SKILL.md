---
name: backend-auth
description: "Authentication and authorisation on the server: sign-in with a maintained library, sessions and cookies, tokens for other clients, password storage, reset and verification tokens, permissions and ownership on every request, tenants, API keys. Read before building or changing sign-in, accounts or who may do what."
---

# Who is calling, and what they may do

One part of the backend; the whole is in the `backend` skill. Most breaches
in generated backends are not clever: a password stored with a fast hash, a
token in `localStorage`, an endpoint that loads `/orders/42` for whoever asks.

## 1. Use what exists

- The framework's own auth, or a maintained library, never hand-written
  crypto or session handling: Auth.js or Better Auth for Next.js and Node,
  Laravel's Sanctum, Breeze or Fortify, Django's auth, `tower-sessions` or
  `axum-login` for axum, `scs` for Go. Keep the one the project uses.
- Sign-in with Google, GitHub and the like through the library's OAuth or
  OpenID Connect support, with `state` and PKCE; never by parsing tokens by
  hand.

## 2. Sessions for the web

- A browser app on the same site uses a server session: a random id of at
  least 128 bits in a cookie with `HttpOnly`, `Secure`, `SameSite=Lax` and
  `Path=/`; the session itself in the database or Redis, or in an encrypted,
  signed cookie the library manages.
- A new session id at sign-in (so a planted one is useless), and the session
  deleted at sign-out, on password change and on reset. An idle timeout and
  an absolute lifetime the product suits: days for a shop, minutes to hours
  for an admin or money.
- State-changing requests from a browser with cookie auth are protected
  against CSRF: `SameSite=Lax` plus the framework's CSRF token for forms, or a
  check that the `Origin` header is the app's own. CORS is not a CSRF
  defence.

## 3. Tokens for other clients

- Mobile apps, CLIs and other services use tokens: a short-lived access
  token (5 to 15 minutes) and a refresh token that rotates on each use, with
  reuse of an old one revoking the family. Verify signature, algorithm (pinned,
  never `none`), `exp`, `iss` and `aud` on every request.
- A web app does not keep tokens in `localStorage` or `sessionStorage`, where
  any injected script reads them; it uses the session cookie.
- API keys for machine callers: 32 random bytes with a readable prefix
  (`sk_live_`), shown once, stored as a hash, with scopes, a last-used time,
  and a way to revoke.

## 4. Passwords

- Stored with argon2id (the library's defaults) or bcrypt with a cost of 12
  or more; never MD5, SHA-1, SHA-256 or any fast hash, never reversible
  encryption, never plain.
- A minimum length (at least 8; 12 or more when the password is the only
  factor), at least 64 characters allowed, no rules about symbols or case, and
  known-breached passwords refused when a list is available.
- Compared in constant time by the library. The same answer and the same
  timing for an unknown email and a wrong password.
- Sign-in, sign-up and reset are rate-limited per account and per IP, with a
  growing delay after failures (`backend-security`).

## 5. Reset, verification and invitations

- A token of 32 random bytes, stored as its hash, used once, expiring (30 to
  60 minutes for a password reset, a day for email verification, a week for
  an invitation), and sent only to the address on the account.
- "If that address has an account, we sent a link" whether or not it has one.
- A reset ends every other session of the account.

## 6. Authorisation on every request

- Deny by default: a route with no rule is closed. Signing in says who
  someone is; each action still checks what they may do.
- The check is on the server, in the service or a policy, for every request;
  hiding a button is not a permission.
- Ownership, not just a role: a query for a record is scoped to what the
  caller may see (`WHERE id = $1 AND owner_id = $user`, or the tenant), so
  changing the id in the URL returns `404`, not someone else's order. This
  goes for every read, update, delete, download and nested route.
- Roles and permissions live in one place (a policy module, gates, a
  permissions table), named for actions (`orders.refund`), not scattered
  `if user.role == "admin"` checks.
- Several organisations in one database: every tenant table has the tenant
  id, every query filters by it (through a scoped repository, a global scope,
  or row-level security in PostgreSQL), and a test proves one tenant cannot
  read another's data.

## 7. Records and extra factors

- Sign-ins, failures, password and email changes, role changes and admin
  actions are recorded with who, when and from where, without the secret.
- Two-factor sign-in (TOTP through a library, recovery codes stored hashed)
  when the product handles money, health or admin power, or the user asks.

## Avoid

Passwords with a fast hash; your own JWT or session code; tokens in
`localStorage`; a session id kept after sign-in; `/api/orders/:id` with no
owner check; `if (isAdmin)` copied into twenty handlers; a reset link that
works twice or never expires; "no account with that email"; secrets or
tokens in logs.
