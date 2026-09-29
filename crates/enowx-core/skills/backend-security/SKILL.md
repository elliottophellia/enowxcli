---
name: backend-security
description: "Security basics for a backend reachable from the internet: a checklist for every route, injection of every kind, SSRF with checks after DNS resolution, paths and uploads, secrets management, security headers with values, CORS and CSRF, rate limits and brute-force protection in a shared store, admin and debug endpoints, what never goes in a log, dependency scanning in CI. Read before shipping an endpoint that takes user input, files or URLs, or before a security pass."
---

# Security basics

One part of the backend; the whole is in the `backend` skill, sign-in and
permissions in `backend-auth`. Every input is hostile until validated
(`backend-api`), and every rule here is checked on the server. Reviewing or
auditing code for weaknesses is `security`, `security-web` and
`security-auth`; this is what to build in from the start.

## 1. Every route, the same questions

| Question | The answer in code |
|---|---|
| Who is calling? | Authentication on the whole router, with an explicit list of public routes |
| May they touch this record? | A query scoped to the caller or tenant, `404` otherwise (`backend-auth`) |
| Is the input what we expect? | A schema with types, sizes and allowed values; unknown fields dropped (`backend-api`) |
| Is the output encoded for where it goes? | The JSON serialiser, escaping templates; CSV cells starting with `=`, `+`, `-`, `@` prefixed with `'` |
| How often may they call? | A rate limit per account, key or IP (section 7) |
| What does a failure or a log reveal? | The problem format and ids, nothing internal (`backend-errors`, section 9) |

## 2. Injection

- SQL: parameters only (`backend-data`); column names for sort and filter
  from an allowed list.
- Shell: never build a command line from input. Call the program with an
  argument array (`execFile`, `subprocess.run([...])`, `exec.Command`), and
  better, a library instead of a program. A value that could start with `-`
  goes after `--`, so it cannot become an option.
- HTML: rendered by a template engine or framework that escapes by default;
  user HTML (rich text) cleaned with a sanitiser (DOMPurify, nh3, ammonia,
  bluemonday) against an allowed list; bleach is deprecated.
- No `eval`, no deserialising untrusted data into objects (`pickle`,
  unsafe `yaml.load`, PHP `unserialize`), no templates built from user text;
  XML parsed with external entities off (defusedxml in Python).
- Regular expressions on user input: bounded input length, and no nested
  quantifiers that can backtrack for minutes.
- JSON merged into objects in JavaScript only after a schema has dropped
  `__proto__` and `constructor`, or into a `Map`.
- A redirect to a URL from the request (`?next=`) only to a path on your own
  site: it starts with `/` and not `//` or `/\`.

## 3. Requests the server makes for a user (SSRF)

When a user supplies a URL the server fetches (a link preview, an import, a
webhook address):

- A list of allowed hosts when the feature permits one; then nothing else
  can be reached.
- Otherwise allow only `https` (and `http` if needed), resolve the host and
  refuse private, loopback and link-local addresses (`10.0.0.0/8`,
  `172.16.0.0/12`, `192.168.0.0/16`, `127.0.0.0/8`, `169.254.0.0/16` with the
  cloud metadata address, `::1`, `fc00::/7`), and check again after every
  redirect or turn redirects off. Also `0.0.0.0/8`, `100.64.0.0/10`,
  `fe80::/10` and IPv4 inside IPv6 (`::ffff:127.0.0.1`); only ports 80, 443.
- Check the address actually connected to, not an earlier lookup: a second
  lookup can answer differently (DNS rebinding). A Go dialer's `Control`
  sees the resolved IP; in Node, request-filtering-agent; anywhere, an
  egress proxy that enforces it (Smokescreen) or a network policy.
- A timeout, a size limit on the response, and no credentials sent. On AWS,
  IMDSv2 with a hop limit of 1 keeps instance credentials out of reach.

```go
var extra = []netip.Prefix{
	netip.MustParsePrefix("0.0.0.0/8"),
	netip.MustParsePrefix("100.64.0.0/10"),
}

func publicOnly(_, address string, _ syscall.RawConn) error {
	host, _, err := net.SplitHostPort(address)
	if err != nil {
		return err
	}
	ip, err := netip.ParseAddr(host)
	if err != nil {
		return err
	}
	ip = ip.Unmap()
	inExtra := slices.ContainsFunc(extra, func(p netip.Prefix) bool { return p.Contains(ip) })
	if !ip.IsGlobalUnicast() || ip.IsPrivate() || inExtra {
		return fmt.Errorf("ssrf: %s is not a public address", ip)
	}
	return nil
}

var fetchClient = &http.Client{
	Timeout:       10 * time.Second,
	Transport:     &http.Transport{DialContext: (&net.Dialer{Control: publicOnly}).DialContext},
	CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse },
}
```

## 4. Files and paths

- Never put a user's file name or path into a file system path: generate
  names (a UUID), and when a path must be joined, resolve it and check it is
  still inside the base directory.
- Uploads: a size limit enforced while reading, the type checked from the
  content (magic bytes) against an allowed list, images re-encoded when they
  are shown to others, stored outside the web root or in object storage, never
  executed, and served with `Content-Disposition: attachment` unless they are
  images you produced.
- Archives: every entry's path checked the same way (zip slip), and the
  unpacked size and entry count capped (zip bombs). Presigned uploads,
  scanning and serving are in `backend-files`.

## 5. Secrets

- In the environment or the platform's secret store, read at start
  (`backend-observability`). Never in the repository, a Docker image, a test
  fixture, a client bundle (`NEXT_PUBLIC_` and `VITE_` variables are public),
  a URL, or an error message.
- `.env` in `.gitignore`, `.env.example` committed with every name and no
  real value. A secret that was committed is rotated, not just deleted.
- Random values for tokens and ids from the platform's secure generator
  (`crypto.randomBytes`, `secrets`, `crypto/rand`, `rand::rngs::OsRng`),
  never `Math.random`.
- In production they come from a secret manager (AWS Secrets Manager, GCP
  Secret Manager, Azure Key Vault, Vault or OpenBao, Doppler, Infisical),
  injected at start; into Kubernetes through the External Secrets Operator;
  in git only encrypted (SOPS).
- One secret per service and environment, with an owner and a rotation
  date; signing keys and webhook secrets accept old and new during a change.
- Never a command-line argument (visible in `ps`), printed at start, or
  shown by a debug endpoint; image builds use build secrets
  (`RUN --mount=type=secret`, `devops-containers`). gitleaks or trufflehog
  in CI and the host's push protection catch what slips.

## 6. Headers, CORS and CSRF

- HTTPS only, with `Strict-Transport-Security` in production.
- `X-Content-Type-Options: nosniff`, a `Referrer-Policy`, and for HTML a
  `Content-Security-Policy` with `frame-ancestors` (the framework's helmet or
  middleware sets these). One middleware, on every response, errors too:

| Header | Value |
|---|---|
| `Strict-Transport-Security` | `max-age=63072000; includeSubDomains`; `preload` only once every subdomain is HTTPS for good |
| `Content-Security-Policy`, HTML | `default-src 'self'; script-src 'self' 'nonce-…'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'` |
| `Content-Security-Policy`, JSON | `default-src 'none'; frame-ancestors 'none'` |
| `X-Content-Type-Options` | `nosniff` |
| `Referrer-Policy` | `strict-origin-when-cross-origin`; `no-referrer` on pages holding tokens |
| `Cross-Origin-Opener-Policy` | `same-origin` |
| `Cross-Origin-Resource-Policy` | `same-origin` for APIs not meant to be embedded elsewhere |
| `Permissions-Policy` | `camera=(), microphone=(), geolocation=()`, opened per feature |
| `X-Powered-By`, a detailed `Server` | Removed |

- The middleware: helmet or `@fastify/helmet`, Hono's `secureHeaders()`,
  Django's `SecurityMiddleware` settings, Spring Security's defaults, Rails's
  defaults with `content_security_policy`,
  NetEscapades.AspNetCore.SecurityHeaders; in Go or axum, a few lines of
  your own (tower-http's `SetResponseHeaderLayer`).
- CORS lists the origins allowed, the methods and headers used, and allows
  credentials only for those origins. Never `*` with credentials, never an
  origin echoed back unchecked. Origins come from config and match exactly:
  no suffix test or regex that `evil-example.com` passes, never `null`. CORS
  only relaxes the browser; it protects nothing from curl.
- Cookies: `HttpOnly`, `Secure`, `SameSite` (`backend-auth`).
- CSRF with cookie sessions: `SameSite=Lax` stops cross-site form posts, not
  those from a sibling subdomain. Keep the framework's CSRF token (Django,
  Rails, Laravel and Spring have it on), or refuse unsafe requests whose
  `Sec-Fetch-Site` is `cross-site` or whose `Origin` is not yours (Go 1.25's
  `http.CrossOriginProtection` does this). `GET` never changes anything.

## 7. Rate limits and brute force

- Rate limits per user and per IP, strict on sign-in, sign-up, password reset
  and verification codes (about five a minute per account), on anything that
  sends email or SMS, and on expensive endpoints (search, exports, AI
  calls); a `429` with `Retry-After` when hit.
- Counters in a shared store (Redis or Valkey), never in process memory
  behind a load balancer, where each instance counts its own share. Atomic
  token bucket or sliding window: rate-limiter-flexible or
  `@upstash/ratelimit` (Node), `limits` or slowapi (Python),
  `go-redis/redis_rate`, Bucket4j, Laravel's `RateLimiter::for`, Rails's
  `rate_limit` or Rack::Attack. ASP.NET Core's limiter and Go's
  `x/time/rate` count per instance.
- Keyed by account (one email tried from many IPs), IP (many emails from one)
  and API key or tenant. The client IP comes from `X-Forwarded-For` only as
  set by a proxy you trust and have configured; otherwise it is whatever the
  attacker typed.

| What | Starting point |
|---|---|
| Sign-in | 5 a minute per account, 20 per IP, a growing delay after failures |
| Password reset, verification email | 3 an hour per account, 10 per IP |
| A six-digit code | 5 tries, then a new code |
| Sign-up | 10 an hour per IP; a CAPTCHA (Turnstile, hCaptcha) when abused |

- Repeated failures on one account bring a delay or a CAPTCHA and an email
  to the owner; a lock is temporary, never one an attacker can use to shut
  the owner out.
- Size limits on bodies, uploads, arrays and pages; time limits on queries
  and outbound calls. Without them one request can take the server down.
  Header and body read timeouts (Node's `headersTimeout`, `requestTimeout`,
  or the proxy's) stop slow clients holding connections open.

## 8. Admin and debug endpoints

- Admin lives apart (`/admin` or `admin.example.com`): its own permission,
  MFA, shorter sessions, CSRF protection, every action audited
  (`backend-auth`); better still, reachable only through an access proxy or
  VPN (Cloudflare Access, Tailscale).
- Consoles and dashboards (Django admin, Filament, Horizon, Telescope,
  Sidekiq's web UI, Bull Board, Swagger UI's try-it) sit behind that sign-in
  or are off in production.
- Off in production: `DEBUG=True`, `APP_DEBUG`, Spring Boot Actuator beyond
  `health` and `info`, `phpinfo()`, pprof on a public port, error pages with
  stacks. Metrics endpoints are not public.

## 9. Logs and data

- Never logged: passwords, tokens, keys, session ids, card numbers, full
  personal records. Log ids instead (`user_id=812`), and mask what must be
  seen (`****4242`).
- Redaction set in the logger, so a new line cannot forget it: pino's
  `redact` (`req.headers.authorization`, `req.headers.cookie`,
  `*.password`), a structlog processor, slog's `ReplaceAttr`, Serilog's
  destructuring policies. Bodies of sign-in, payment and upload requests
  are not logged at all.
- URLs land in proxy and access logs: no keys or session tokens in them; a
  one-time token in a link is single use and short-lived.
- Personal data kept only as long as it is needed, and deletable on request
  where the law asks (GDPR, Indonesia's UU PDP). IP addresses count: access
  logs are kept for a set period (30 to 90 days), not forever.
- Admin and destructive actions recorded with who and when.

## 10. Dependencies

- The lock file committed; versions pinned by it; new packages chosen for
  maintenance and size (`code` skill).
- The audit tool run before shipping (`npm audit`, `pnpm audit`,
  `pip-audit`, `govulncheck`, `cargo audit`, `composer audit`), and a known
  critical issue fixed or reported.
- In CI on every pull request, failing on high and critical: that audit (or
  OSV-Scanner across languages, `bundler-audit`,
  `dotnet list package --vulnerable`), Trivy on the image, a secret scan;
  Dependabot or Renovate opening the updates (`devops-security`).
- Installs from the lock file only (`npm ci`, `pnpm install
  --frozen-lockfile`, `uv sync --locked`), install scripts off where the tool
  allows (pnpm 10 runs none it was not told to), and new releases taken
  after a few days' wait (`minimumReleaseAge` in Renovate or pnpm), since a
  hijacked package is usually caught within days.

## Check it

```sh
curl -sI https://staging.example.com/ | grep -iE 'strict-transport|content-security|nosniff|referrer'
for i in $(seq 8); do curl -s -o /dev/null -w '%{http_code} ' -X POST localhost:3000/api/sign-in \
  -H 'content-type: application/json' -d '{"email":"a@example.com","password":"wrong"}'; done
```

- The headers are on pages and on error responses; the loop ends in `429`
  with `Retry-After`.
- Signed in as a second user, each new route with the first user's ids
  answers `404`.
- A URL-fetching feature refuses `http://169.254.169.254/`,
  `http://localhost`, `http://[::1]` and a host that resolves to `10.0.0.1`.
- The audit and the secret scan are clean, or each finding is reported;
  `rg -n 'eval\(|shell=True|yaml\.load\(|pickle\.loads|Math\.random' src`
  shows nothing new that touches user input.

## Avoid

Input in SQL, shell or templates; a user URL fetched without checking where
it points; a user's file name on disk; uploads served back as HTML; secrets in
the repository or the client bundle; `Math.random` for a token; CORS `*` with
cookies; no rate limit on sign-in; a password or token in a log; rate limits
counted in memory per instance; the client IP read from a header anyone can
send; an admin panel open to the internet; debug mode in production; a
dependency update installed the hour it was published.
