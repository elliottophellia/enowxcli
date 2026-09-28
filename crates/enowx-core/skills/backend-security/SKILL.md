---
name: backend-security
description: "Security basics for a backend reachable from the internet: injection of every kind, SSRF, path traversal, uploads, secrets, security headers, CORS, rate limits, dependencies, what never goes in a log. Read before shipping an endpoint that takes user input, files or URLs, or before a security pass."
---

# Security basics

One part of the backend; the whole is in the `backend` skill, sign-in and
permissions in `backend-auth`. Every input is hostile until validated
(`backend-api`), and every rule here is checked on the server.

## 1. Injection

- SQL: parameters only (`backend-data`); column names for sort and filter
  from an allowed list.
- Shell: never build a command line from input. Call the program with an
  argument array (`execFile`, `subprocess.run([...])`, `exec.Command`), and
  better, a library instead of a program.
- HTML: rendered by a template engine or framework that escapes by default;
  user HTML (rich text) cleaned with a sanitiser (DOMPurify, bleach,
  ammonia) against an allowed list.
- No `eval`, no deserialising untrusted data into objects (`pickle`,
  unsafe `yaml.load`, PHP `unserialize`), no templates built from user text.
- Regular expressions on user input: bounded input length, and no nested
  quantifiers that can backtrack for minutes.

## 2. Requests the server makes for a user (SSRF)

When a user supplies a URL the server fetches (a link preview, an import, a
webhook address):

- Allow only `https` (and `http` if needed), resolve the host and refuse
  private, loopback and link-local addresses (`10.0.0.0/8`, `172.16.0.0/12`,
  `192.168.0.0/16`, `127.0.0.0/8`, `169.254.0.0/16` with the cloud metadata
  address, `::1`, `fc00::/7`), and check again after every redirect or turn
  redirects off.
- A timeout, a size limit on the response, and no credentials sent.

## 3. Files and paths

- Never put a user's file name or path into a file system path: generate
  names (a UUID), and when a path must be joined, resolve it and check it is
  still inside the base directory.
- Uploads: a size limit enforced while reading, the type checked from the
  content (magic bytes) against an allowed list, images re-encoded when they
  are shown to others, stored outside the web root or in object storage, never
  executed, and served with `Content-Disposition: attachment` unless they are
  images you produced.

## 4. Secrets

- In the environment or the platform's secret store, read at start
  (`backend-observability`). Never in the repository, a Docker image, a test
  fixture, a client bundle (`NEXT_PUBLIC_` and `VITE_` variables are public),
  a URL, or an error message.
- `.env` in `.gitignore`, `.env.example` committed with every name and no
  real value. A secret that was committed is rotated, not just deleted.
- Random values for tokens and ids from the platform's secure generator
  (`crypto.randomBytes`, `secrets`, `crypto/rand`, `rand::rngs::OsRng`),
  never `Math.random`.

## 5. Headers and CORS

- HTTPS only, with `Strict-Transport-Security` in production.
- `X-Content-Type-Options: nosniff`, a `Referrer-Policy`, and for HTML a
  `Content-Security-Policy` with `frame-ancestors` (the framework's helmet or
  middleware sets these).
- CORS lists the origins allowed, the methods and headers used, and allows
  credentials only for those origins. Never `*` with credentials, never an
  origin echoed back unchecked.
- Cookies: `HttpOnly`, `Secure`, `SameSite` (`backend-auth`).

## 6. Limits

- Rate limits per user and per IP, strict on sign-in, sign-up, password reset
  and verification codes (about five a minute per account), on anything that
  sends email or SMS, and on expensive endpoints (search, exports, AI
  calls); a `429` with `Retry-After` when hit.
- Size limits on bodies, uploads, arrays and pages; time limits on queries
  and outbound calls. Without them one request can take the server down.

## 7. Logs and data

- Never logged: passwords, tokens, keys, session ids, card numbers, full
  personal records. Log ids instead (`user_id=812`), and mask what must be
  seen (`****4242`).
- Personal data kept only as long as it is needed, and deletable on request
  where the law asks (GDPR, Indonesia's UU PDP).
- Admin and destructive actions recorded with who and when.

## 8. Dependencies

- The lock file committed; versions pinned by it; new packages chosen for
  maintenance and size (`code` skill).
- The audit tool run before shipping (`npm audit`, `pnpm audit`,
  `pip-audit`, `govulncheck`, `cargo audit`, `composer audit`), and a known
  critical issue fixed or reported.

## Avoid

Input in SQL, shell or templates; a user URL fetched without checking where
it points; a user's file name on disk; uploads served back as HTML; secrets in
the repository or the client bundle; `Math.random` for a token; CORS `*` with
cookies; no rate limit on sign-in; a password or token in a log.
