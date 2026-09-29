---
name: backend-auth
description: "Authentication and authorisation on the server: a maintained library per stack, sessions or tokens by client type, cookies, rotation and remember-me, session lists with revoke, password storage, passkeys and magic links, social sign-in and account linking, multi-factor with recovery codes, reset and verification tokens, permission models (RBAC, ABAC, relationships) and where checks live, ownership on every request, tenants, impersonation, API keys and machine clients. Read before building or changing sign-in, accounts or who may do what."
---

# Who is calling, and what they may do

One part of the backend; the whole is in the `backend` skill, and reviewing
auth someone else built is `security-auth`. Most breaches in generated
backends are not clever: a password stored with a fast hash, a token in
`localStorage`, an endpoint that loads `/orders/42` for whoever asks, an
account taken over by a social sign-in matched on an unverified email.

## 1. Use what exists

The framework's own auth, or a maintained library, never hand-written crypto
or session handling. Keep the one the project uses; with none:

| Stack | Use |
|---|---|
| Next.js, Node | Better Auth (sessions, OAuth, passkeys, 2FA, magic links, organisations as plugins); Auth.js where the project has it. Lucia is now a guide rather than a library; Better Auth or the Oslo and Arctic packages it teaches with replace it |
| Django | Its auth, with django-allauth for social sign-in, MFA and passkeys |
| FastAPI | Security helpers only: an identity provider, or Authlib for OpenID Connect with server-side sessions |
| Laravel | A starter kit (Breeze in older apps), Sanctum for SPA cookies and API tokens, Fortify for headless sign-in, 2FA and reset, Socialite for OAuth |
| Rails | The Rails 8 generator (`bin/rails generate authentication`); Devise when its modules are wanted; OmniAuth for social sign-in |
| Spring | Spring Security: form login, OAuth 2 client and resource server, passkeys since 6.4 |
| .NET | ASP.NET Core Identity; the JWT bearer handler for APIs |
| Go | `scs` for sessions, `golang.org/x/oauth2` with `coreos/go-oidc`, `go-webauthn/webauthn` |
| Rust | `tower-sessions`, `axum-login`, `openidconnect`, `webauthn-rs` |

- A hosted provider (Clerk, Auth0, WorkOS, Supabase Auth; Keycloak or
  Zitadel self-hosted) when customers need enterprise SSO (SAML, their own
  OIDC) and SCIM, or a small team wants little to run. Your tables still
  hold the app's data, keyed by the provider's user id.
- Sign-in with Google, GitHub and the like through the library's OAuth or
  OpenID Connect support, with `state` and PKCE; never by parsing tokens by
  hand.

## 2. Sessions or tokens, by client

| Client | Use |
|---|---|
| Browser app on the same site as the API | A server session in an `HttpOnly` cookie |
| Browser app on another site | A backend-for-frontend on the app's site holding the session, or the API moved under the same site (`app.example.com` and `api.example.com` are one site) |
| Mobile app | A short access token and a rotating refresh token in the Keychain or Keystore; third-party sign-in in the system browser with PKCE |
| CLI | The device authorisation grant (RFC 8628), or a personal API key |
| Another company's server | An API key, or OAuth client credentials |
| A third-party app acting for your users | OAuth 2 code flow with PKCE, scopes and consent |
| Your own services | Workload identity, mTLS, or short-lived tokens with an audience |

Sessions are the default: revoked at once, nothing for a script to steal, no
refresh logic. A signed token (JWT) earns its place when many services check
it without a lookup, and then it lives minutes, not days.

## 3. Sessions for the web

- A browser app on the same site uses a server session: a random id of at
  least 128 bits in a cookie with `HttpOnly`, `Secure`, `SameSite=Lax` and
  `Path=/`; the session itself in the database or Redis, or in an encrypted,
  signed cookie the library manages. Named `__Host-session`, the prefix
  forces `Secure`, `Path=/` and no `Domain`, so no subdomain can set it.
- Stored by the SHA-256 of its id, with the user, created and last-seen
  times, the IP and user agent at sign-in, and how it was authenticated
  (password, passkey, MFA done).
- A new session id at sign-in (so a planted one is useless) and at every
  change of privilege: MFA completed, a role changed, impersonation started
  or ended. The old id is deleted, not left valid.
- The session deleted at sign-out, on password change and on reset. An idle
  timeout and an absolute lifetime the product suits: days for a shop,
  minutes to hours for an admin or money.
- "Remember me" is a longer absolute lifetime for that session (30 days),
  not a second credential. Where a separate token is unavoidable: a
  selector and a validator, the validator hashed and replaced on each use,
  and a known selector with a wrong validator ends every session of the user.
- A sessions page: device and browser, rough place, last active, "this
  device", end any one, and "sign out everywhere else". A sign-in from a new
  device sends an email.
- State-changing requests from a browser with cookie auth are protected
  against CSRF: `SameSite=Lax` plus the framework's CSRF token for forms, or a
  check that the `Origin` header is the app's own. CORS is not a CSRF
  defence (`backend-security`).

## 4. Tokens, keys and machine clients

- Mobile apps, CLIs and other services use tokens: a short-lived access
  token (5 to 15 minutes) and a refresh token that rotates on each use, with
  reuse of an old one revoking the family. Verify signature, algorithm (pinned,
  never `none`), `exp`, `iss` and `aud` on every request.
- A web app does not keep tokens in `localStorage` or `sessionStorage`, where
  any injected script reads them; it uses the session cookie.
- API keys for machine callers: 32 random bytes with a readable prefix
  (`sk_live_`), shown once, stored as a hash, with scopes, a last-used time,
  and a way to revoke. SHA-256 suffices for a random key (slow hashes are
  for passwords), so it is looked up by hash. The interface shows the prefix
  and last four characters; two keys can be live at once for rotation; each
  has its own rate limit and dies with its owner's membership.
- Service to service: the client credentials grant, the token cached until
  shortly before `exp`, its audience the API called; inside one platform,
  workload identity or mTLS rather than one static secret for all.

## 5. Passwords

- Stored with argon2id (the library's defaults, at least 19 MiB, 2
  iterations, 1 lane) or bcrypt with a cost of 12 or more; never MD5, SHA-1,
  SHA-256 or any fast hash, never reversible encryption, never plain.
  Rehashed at sign-in when the parameters are old.
- A minimum of 15 characters when the password is the only factor and 8 with
  a second (NIST SP 800-63B revision 4), at least 64 characters allowed, no
  rules about symbols or case, no forced rotation, and known-breached
  passwords refused (the Pwned Passwords range API sees only the first five
  characters of the SHA-1 hash).
- Compared in constant time by the library. The same answer and the same
  timing for an unknown email and a wrong password: an unknown user still
  costs one dummy hash.
- Sign-in, sign-up and reset are rate-limited per account and per IP, with a
  growing delay after failures (`backend-security`).
- Changing the password asks for the current one and ends other sessions.

## 6. Passkeys and magic links

- Passkeys (WebAuthn) through a library: `@simplewebauthn/server`,
  `webauthn` for Python, `go-webauthn`, `webauthn-rs`, Spring Security, or
  the auth library's plugin. Offered beside the existing way in: added from
  settings after a sign-in, then used through autofill
  (`mediation: "conditional"`, and `autocomplete="username webauthn"` on the
  username field).
- A random challenge of at least 16 bytes, kept in the session, accepted
  once within about 5 minutes; the origin and the RP ID (the registrable
  domain, which covers its subdomains) checked. Discoverable credentials
  (`residentKey: "required"`) for sign-in without a username; user
  verification required when the passkey is the only factor.
- Stored per credential: id, public key, sign counter, transports, backup
  flags, a name the user chose, created and last used. Several per account;
  removing one needs a recent sign-in, and never removes the last way in.
- Magic links: 32 random bytes, stored hashed, single use, 10 to 15 minutes.
  The link opens a page whose button posts the token: mail scanners open
  links on arrival, and a `GET` that signs in or spends the token is used up
  before the person clicks. A six-digit code beside it serves someone reading
  mail on another device (10 minutes, about 5 tries per code).

## 7. Social sign-in and linking accounts

- The code flow with PKCE, `state`, and `nonce` for OpenID Connect; the ID
  token checked by the library.
- An identity is the provider's issuer and subject (`sub`), in a table of
  its own (`user_identities`: provider, subject, user id, email at link
  time, unique on provider and subject). Never the email alone.
- A known identity signs in its user. A new identity whose email matches an
  account is linked only when the provider asserts `email_verified` and is
  authoritative for that domain (Google for Gmail and Workspace); otherwise
  the person signs in the old way first and links from settings.
- Microsoft Entra: key on `oid` with `tid`, never the `email` claim, which a
  tenant can set to anything (the nOAuth takeover).
- An identity is unlinked only when another way in remains. Provider tokens
  are kept only when you call the provider later: encrypted, minimal scopes.

## 8. Multi-factor and step-up

- Required for admins and for accounts that move money or hold health data;
  offered to everyone.
- TOTP (RFC 6238) through a library (otplib, pyotp, pquerna/otp, totp-rs,
  Fortify, devise-two-factor): 30-second steps, one step of drift, a used
  code refused within its window, attempts limited per account, the secret
  encrypted at rest (it cannot be hashed), enrolment confirmed with a code.
- Passkeys are the strong factor; SMS and email codes a fallback only (SIM
  swap, mailbox takeover).
- Recovery codes: 8 to 10, random, at least 10 characters, shown once,
  stored hashed, single use, replaced as a set; using one emails the owner.
- The second step is bound to the first on the server (a pending session,
  not a flag the client holds); API tokens and social sign-in do not skip
  it; a password reset does not turn it off.
- Step-up: changing the email, password or MFA, new API keys, payouts and
  deleting the account need a sign-in or fresh factor from the last 5 to 15
  minutes.

## 9. Reset, verification and invitations

- A token of 32 random bytes, stored as its hash, used once, expiring (30 to
  60 minutes for a password reset, a day for email verification, a week for
  an invitation), and sent only to the address on the account.
- "If that address has an account, we sent a link" whether or not it has one.
- A reset ends every other session of the account.
- The link is built from the configured base URL, never the request's
  `Host` header, which would send the token to another domain.
- An email change is confirmed from the new address and announced to the
  old one, with a way to undo it.

## 10. Authorisation on every request

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

| Model | Fits | Tools |
|---|---|---|
| RBAC: roles grant named permissions | Most apps; start here | A permission map in code, Laravel gates, Spring `@PreAuthorize`, Pundit |
| ABAC: rules on attributes (owner, status, amount, time) | "Editors may change drafts they own"; limits on amounts | Policy functions, CASL, Casbin, Cerbos, Cedar, OPA |
| ReBAC: permission from relationships (a team that owns the folder holding the file) | Sharing, nested groups, hierarchies | OpenFGA, SpiceDB, Permify |

One policy per resource, called by the service; lists use the same rule as a
query scope (`visibleTo(user)`), since a page of 25 cannot be checked row by
row after it is loaded:

```ts
export function can(user: User, action: "view" | "refund", order: Order): boolean {
  if (order.tenantId !== user.tenantId) return false;
  if (action === "view") return order.customerId === user.id || user.perms.has("orders.view_all");
  return user.perms.has("orders.refund") && order.status === "paid";
}
```

## 11. Tenants

- Several organisations in one database: every tenant table has the tenant
  id, every query filters by it (through a scoped repository, a global scope,
  or row-level security in PostgreSQL), and a test proves one tenant cannot
  read another's data.
- The tenant comes from the signed-in membership (someone in several picks
  one, checked against their memberships on each request), never from a
  header, the body or the subdomain alone.
- Unique keys include it (`(tenant_id, email)`), and so do foreign keys
  (`(tenant_id, customer_id)` referencing `customers (tenant_id, id)`), so a
  row cannot point into another tenant.
- Row-level security as a second wall: `ENABLE` and `FORCE ROW LEVEL
  SECURITY`, policies on `current_setting('app.tenant_id')`, set in each
  transaction with `set_config('app.tenant_id', $1, true)`, and an app role
  that neither owns the tables nor has `BYPASSRLS`.
- Cache keys, search indexes, file paths, job payloads and logs carry it.

## 12. Impersonation and records

- Support acting as a user: a permission of its own, a typed reason, a new
  session holding the admin (actor) and the user (subject), an hour at most,
  a banner throughout, and an explicit end back to the admin's own session.
  No password, email or MFA changes, full payment details or account
  deletion while impersonating.
- Sign-ins, failures, password and email changes, role changes, admin
  actions and every impersonated action are recorded with who (actor and
  subject), when and from where, without the secret.

## Check it

- Tests with two users and two tenants: another user's or tenant's record is
  `404`; a user without the permission gets `403` (or `404`); no list holds
  their rows.
- `curl -si` on sign-in shows
  `Set-Cookie: __Host-session=…; Path=/; Secure; HttpOnly; SameSite=Lax`;
  the id changes at sign-in and the old one is refused after sign-out.
- An unknown email and a wrong password give the same status, body and
  about the same time; a reset token works once, not after it expires, and
  the reset ended the other sessions.

## Avoid

Passwords with a fast hash; your own JWT or session code; tokens in
`localStorage`; a session id kept after sign-in; `/api/orders/:id` with no
owner check; `if (isAdmin)` copied into twenty handlers; a reset link that
works twice or never expires; "no account with that email"; secrets or
tokens in logs; accounts linked on an unverified email; a magic link spent
by a `GET`; MFA skipped by another sign-in path; the tenant taken from a
header; impersonation without an audit trail.
