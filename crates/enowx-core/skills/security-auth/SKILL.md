---
name: security-auth
description: "Reviewing authentication and authorisation: password storage, sessions and cookies, JWT pitfalls, OAuth and OpenID Connect flows, MFA and passkeys, account enumeration, brute force and rate limits, password reset and verification tokens, API keys, and authorisation checked on every route and every object, including between tenants. Read before reviewing sign-in, sessions, permissions or tokens."
---

# Reviewing authentication and authorisation

The naive auth review sees bcrypt and stops. It misses the JWT verified
without a pinned algorithm, the session that survives a password reset, the
reset link that works twice, the tenant id read from a header, and the admin
route that trusts a role the client wrote. This is what to check in sign-in,
sessions, tokens and permissions, where it hides in code, and how bad each
gap is. The rules to build by are in `backend-auth`; method and report format
in `security`.

## 1. Map the auth surface

- **Where identity is established**: sign-in and sign-up, OAuth and SSO
  callbacks, magic links, API key and service token middleware, "remember
  me", impersonation.
- **Where it is checked**: the middleware, its order, the public list, the
  policy layer. A hosted provider (Auth0, Clerk, Cognito, Supabase Auth,
  Firebase Auth, Keycloak) takes over password storage and MFA; sessions,
  token checks and authorisation stay in the app.
- **Which library**: Auth.js, Better Auth, Passport, Django auth and allauth,
  Laravel Sanctum or Fortify, Devise, Spring Security. Hand-written auth is
  read line by line; Lucia is deprecated.

```sh
rg -n -i 'bcrypt|argon2|scrypt|pbkdf2|password_hash|hashpw|hashlib\.(md5|sha1|sha256)|createHash\(|md5\(|sha1\(' .
rg -n -i 'jwt\.(sign|verify|decode)|jsonwebtoken|jwtVerify|decodeJwt|ParseWithClaims|ParseUnverified|algorithms' .
rg -n -i 'httpOnly|sameSite|SESSION_COOKIE|cookie_secure|session\.regenerate|regenerate_id|cycle_key|reset_session' .
rg -l -i 'reset|forgot|magic.?link|verify.?email|otp|totp|recovery.?code|api.?key' .
```

## 2. Passwords

- **Storage** (CWE-916, 256, 257): argon2id (OWASP minimum 19 MiB memory,
  2 iterations, 1 lane), scrypt (N 2^17, r 8, p 1), bcrypt at cost 12 or
  more, PBKDF2-HMAC-SHA256 at 600,000 iterations where FIPS demands it. A
  fast hash (MD5, SHA-1, SHA-256, salted or not), reversible encryption or
  plain text is high: it matters exactly when the database leaks.
- bcrypt reads only the first 72 bytes; a pre-hash must be encoded (base64),
  since raw digest bytes can contain zeros that end the input early.
- Verified by the library in constant time; `==` on hashes is a finding.
  Hashes rehashed at sign-in when parameters are old (`password_needs_rehash`,
  argon2's `check_needs_rehash`).
- **Policy** (CWE-521): at least 8 characters (NIST SP 800-63B revision 4
  asks for 15 when the password is the only factor), at least 64 allowed,
  every character allowed, no composition rules, no forced rotation, and
  breached passwords refused (the Pwned Passwords range API receives only
  the first five characters of the SHA-1 hash).
- Changing the password asks for the current one (CWE-620) and ends other
  sessions. A password in a URL, a log or an email is a finding.

## 3. Sessions and cookies

- Session ids of 128 bits or more from a CSPRNG (frameworks do this), never a
  user id, timestamp or hash of them.
- **Fixation** (CWE-384): a new id at sign-in and on privilege change:
  `req.session.regenerate()`, Django `login()`, Laravel
  `session()->regenerate()`, Rails `reset_session`, PHP
  `session_regenerate_id(true)`; Spring Security does it by default.
- **Expiry** (CWE-613): idle and absolute limits enforced on the server
  (OWASP suggests 2 to 5 minutes idle for high-value applications, 15 to 30
  for low-risk ones); sign-out, password change and reset end the session
  on the server. A signed cookie or JWT still valid after sign-out is
  medium; "sign out everywhere" exists.
- **Cookies** (CWE-1004, 614, 1275): `HttpOnly`, `Secure`, `SameSite=Lax`
  (`Strict` for admin), `Path=/`, no `Domain` unless needed (a parent-domain
  cookie reaches every subdomain); the `__Host-` prefix stops a sibling
  subdomain overwriting it.
- Signed is not encrypted: Flask's default session and other signed cookies
  are readable by the user. No secrets or personal data inside.
- Weak session secrets (`keyboard cat` from the express-session docs,
  `changeme`, `django-insecure-` in production, a committed
  `secret_key_base`) let anyone mint sessions: critical when exploitable.

```sh
rg -n -i 'httpOnly:\s*false|secure:\s*false|sameSite:\s*["\x27]?none|SESSION_COOKIE_(SECURE|HTTPONLY)\s*=\s*False|keyboard cat' .
```

## 4. JWT

- **Decoded, not verified** (CWE-347): jsonwebtoken `decode`, jose
  `decodeJwt`, golang-jwt `ParseUnverified`, PyJWT with
  `verify_signature` off, `jwt-decode` on a server. Claims from an
  unverified token are the client's word: critical when they decide access.
- **Algorithm not pinned**: a verifier that follows the header accepts
  `none`, or checks an HMAC keyed with the RSA public key (algorithm
  confusion). Pin it: `algorithms: ["RS256"]`, PyJWT's required `algorithms`,
  golang-jwt `WithValidMethods`.
- **Keys from the token**: `jku`, `x5u` or `jwk` headers trusted; `kid` put
  in a file path or query.
- **Claims**: `exp` required, `iss` and `aud` checked (a token issued to
  another service or client accepted is a confused deputy), a minute of
  clock skew at most.
- **Lifetimes**: access tokens 5 to 15 minutes; refresh tokens kept on the
  server, rotated on use, reuse revoking the family. A 30-day access token
  with no revocation is a finding; so are roles in a long-lived token that
  outlive a demotion.
- HMAC secrets of 32 random bytes or more: a short or dictionary secret is
  guessed offline from any one token (`your-256-bit-secret` is the jwt.io
  example). The payload is readable: no secrets or needless personal data.
- In a browser, not in `localStorage` (any XSS reads it): the session cookie
  or a backend-for-frontend.

```sh
rg -n 'jwt\.decode\(|decodeJwt\(|ParseUnverified|verify_signature["\x27]?\s*:\s*False|ignoreExpiration|(alg|algorithms?)\W{0,6}\[?\s*["\x27]none["\x27]|\bjku\b|\bx5u\b' .
```

## 5. OAuth, OpenID Connect and SAML

- Authorization code with PKCE (S256) for every client, as RFC 9700 (the
  OAuth 2.0 security best practice, 2025) recommends; the implicit flow and
  the password grant are findings.
- `state` per request, bound to the session and checked on the callback;
  without it an attacker can sign a victim into the attacker's account or
  link the attacker's identity to the victim's (high). `nonce` checked in
  the ID token.
- Redirect URIs matched exactly at the provider; the app's own `returnTo`
  validated (`security-web` section 7).
- ID tokens checked by the library: signature against the provider's JWKS,
  `iss`, `aud` equal to the client id, `exp`, `nonce`.
- Users keyed by `iss` plus `sub`, never by email alone. Linking by email
  only when the provider asserts `email_verified` and owns the domain;
  keying Microsoft Entra multi-tenant sign-ins on the mutable `email` claim
  (nOAuth) gives accounts away.
- A provider's access token proves identity to your API only when issued for
  it (the audience); client secrets never in a mobile app or SPA; refresh
  tokens encrypted at rest; scopes minimal.
- SAML: a maintained library at a current version (ruby-saml and others had
  signature bypasses in 2024 and 2025), signed assertions required,
  audience, recipient and `NotOnOrAfter` checked, assertion ids cached
  against replay.

## 6. MFA and passkeys

- TOTP (RFC 6238): 30-second steps, one step of drift, attempts limited per
  account, a used code refused within its window, the shared secret
  encrypted at rest (it cannot be hashed).
- SMS and email codes are weaker (SIM swap, mailbox takeover): a fallback,
  not the factor for admins.
- WebAuthn and passkeys through a library (`@simplewebauthn/server`,
  `webauthn` for Python, `go-webauthn`, `webauthn-rs`): a random, single-use,
  short-lived challenge; origin and RP ID checked; user verification
  required when the passkey is the only factor.
- Recovery codes: 8 to 10, random, hashed, single use, replaced as a set.
- **Bypasses**: the second step reachable without the first (a flag the
  client holds); API tokens or OAuth sign-in skipping MFA; a reset that turns
  MFA off; "remember this device" tokens that never expire; MFA enrolled or
  removed without re-authentication.
- Step-up (a recent sign-in or a fresh factor) for email and password
  changes, new API keys, payouts, new MFA devices, account deletion.

## 7. Enumeration and brute force

- **Enumeration** (CWE-204): the same status, message and timing for known
  and unknown accounts on sign-in, reset, sign-up and resend-verification;
  an unknown user still costs one dummy hash. Low alone; medium when
  membership is itself sensitive (a clinic's patient portal).
- **Brute force** (CWE-307): limits per account and per IP (and per /24 or
  /64) with growing delays on sign-in, code checks, reset, sign-up and
  anything that sends email or SMS. A six-digit code needs a cap per code
  (about 5 tries) and a short life (5 to 10 minutes).
- Lockout that is temporary and cannot be used to lock owners out for good;
  a CAPTCHA or delay after repeated failures; the owner notified.
- Credential stuffing beats per-IP limits: breached-password checks, MFA and
  bot signals matter more.
- Limits keyed on spoofable headers, or counted per request while GraphQL
  batches many attempts into one (`security-web` section 13).

## 8. Reset, verification and magic links

- 32 random bytes from a CSPRNG, stored as a SHA-256 hash, single use,
  bound to one purpose, expiring (reset 15 to 60 minutes, magic link 10 to
  15, email verification a day), and void once used, reissued, or the
  password changes (CWE-640).
- Sent only to the address on the account, never to one in the request (a
  second field, an array); the link built from the configured base URL,
  never the `Host` header, which would send the token to another domain.
- Kept out of logs, analytics and the `Referer` sent to third-party scripts
  (`Referrer-Policy: no-referrer` on the page that holds it).
- After a reset: other sessions end, MFA stays, the owner is told. An email
  change is confirmed from the new address and announced to the old;
  `emailVerified` is never settable by the client.

```sh
rg -n -i 'Math\.random|uuid\.?v1|Date\.now\(\)|random\.(randint|choice)|mt_rand|uniqid' .
rg -n 'req\.(headers\.host|hostname)|request\.get_host\(\)|request\.host\b|\$_SERVER\[.HTTP_HOST' .
```

## 9. API keys and service credentials

- 32 random bytes with a prefix naming product and environment
  (`sk_live_`), shown once, stored hashed (SHA-256 suffices for random keys),
  looked up by hash, scoped, revocable, expiring where possible, with a
  last-used time.
- In a header, never the query string (URLs end up in logs). A user's key
  carries that user's permissions and dies with their membership.
- Service to service: workload identity, mTLS or short-lived signed tokens
  with an audience, not one static key shared by every service.

## 10. Authorisation

- **Deny by default**: auth applied to the whole router with an explicit
  public list. Check the order (a route registered before the middleware),
  catch-alls, and methods (the `GET` guarded, the `POST` to the same path
  not).
- **One layer**: policies or gates (Laravel policies, Pundit, Django and DRF
  `permission_classes`, Spring `@PreAuthorize`, Casbin, Oso, Cerbos,
  OpenFGA or SpiceDB for relationships). Scattered `role == "admin"` checks
  are where one is missing.
- **Every object**: reads, updates, deletes, downloads, exports, search
  results, nested routes, bulk actions, socket subscriptions, GraphQL fields
  and `node` lookups (`security-web` section 1).
- **Tenants**: the tenant from the session's membership, never the body, a
  header or the subdomain alone; every query, cache key, search index, file
  key and job scoped by it.
- **Row-level security** in PostgreSQL: `ENABLE ROW LEVEL SECURITY` and
  `FORCE ROW LEVEL SECURITY` (without `FORCE` the table owner skips it;
  superusers and `BYPASSRLS` roles always do, so the app never connects as
  one); the tenant set with `SET LOCAL` inside a transaction, or a pooled
  connection carries it into the next request.
- **Supabase and Firebase**: the public key plus a table without RLS, or
  rules like `allow read, write: if true` or test mode still in force,
  exposes everything; the `service_role` or secret key never ships to a
  client.
- Roles from the server's records, not a claim the client sent, effective at
  once when changed; admin routes apart, with MFA and an audit trail;
  impersonation logged and visible.
- Tests prove that another user or tenant gets `404` or `403` for each
  resource. Their absence is a note; the gap they would catch is the finding.

```sh
rg -n -i 'isAdmin|role\s*===?\s*["\x27]|hasRole\(|@PreAuthorize|permission_classes|authorize\(|Gate::|->can\(' .
rg -n -i 'row level security|create policy|set_config\(|set local|service_role|if true;' .
```

## 11. Rating auth findings

| Finding | Usual rating |
|---|---|
| Unverified or `none` tokens accepted; an auth bypass on a public route; a mintable session secret | Critical |
| Cross-tenant reads or writes; a public key with RLS off on private tables | Critical or high |
| IDOR on private data; a reset token reusable or never expiring; an MFA bypass; account-linking CSRF | High |
| Sessions surviving a reset or sign-out; codes with no attempt limit | Medium or high |
| No sign-in rate limit; session fixation; tokens in `localStorage` | Medium |
| Enumeration; no `__Host-` prefix; a weak policy where MFA is enforced | Low |

## Check it

- Walk each flow in the code: sign-up, sign-in, sign-out, reset, email
  change, MFA enrol and removal, invitation, role change, account deletion.
  For each: what proves identity, what is issued, what ends, what is logged.
- For each role, the routes it reaches match what the product intends; for
  each tenant-scoped table, every query path filters by tenant.
- Run the project's auth tests and note which forbidden paths have none.

## Avoid

Stopping at the hash function; trusting a decoded but unverified token; an
algorithm taken from the token header; a session that survives a reset; a
reset link built from the `Host` header; users matched by email across
providers; a tenant id from the request; `if isAdmin` copied into handlers;
a public key over tables without RLS; calling enumeration critical.
