---
name: security
description: "Auditing software for security problems: scoping, a light threat model, tracing input to dangerous sinks, confirming reachability, rating severity by impact and likelihood, reporting each finding with its class, place, impact and fix, handling secrets found along the way, and not inventing issues. Read before any security review or audit."
---

# Security audits

A generated audit reads like a poster: "sanitise all inputs", "use HTTPS",
"consider rate limiting", findings with no file or line, a critical for a
theoretical issue behind an admin login, a working exploit pasted in as
proof, an API key quoted in full, and not a word about the endpoint that
hands any customer's invoice to whoever changes the id. This is the method a
practised reviewer follows instead: scope, a light threat model, input traced
to sinks, reachability confirmed, severity from impact and likelihood, and a
report in which every finding has a class, a place, an impact, a fix and a
confidence.

## 1. Scope

- **In and out**: the repositories, services, environments and branches under
  review, and the commit (`git rev-parse --short HEAD`). A change review
  (`git diff main...HEAD`) covers the change and whatever it touches.
- **The stack**: languages and framework versions from the lockfiles (not
  from memory), the database, the auth library, how it is deployed
  (Dockerfile, compose, Kubernetes, Terraform, a platform) and where config
  and secrets come from.
- **The people**: who uses it (visitors, customers, organisations sharing one
  database, staff, admins) and who would attack it.
- **The rules**: read and run commands; never edit, deploy, send requests to
  production or to third-party services, brute-force anything, or run the
  project's destructive scripts. When only a live test would settle a
  question, say so in the report instead of running it.

```sh
git rev-parse --short HEAD && git log --oneline -5
ls -a . .github/workflows 2>/dev/null
cat package.json pyproject.toml go.mod Cargo.toml composer.json Gemfile 2>/dev/null | head -150
```

## 2. A light threat model

Ten minutes, written at the top of the report, so the review looks where the
damage would be.

- **Assets**: personal and payment data, secrets, money (balances, credits,
  refunds, coupons), accounts (admins above all), infrastructure (servers,
  cloud accounts, CI that can deploy).
- **Entry points**: HTTP routes and server actions, GraphQL resolvers,
  WebSockets, webhooks, uploads and imports, queue consumers, scheduled jobs
  that read outside data, CLIs, admin panels, OAuth callbacks, inbound email,
  and LLM features whose output reaches tools or pages.
- **Trust boundaries**: browser to server, server to database, service to
  service, tenant to tenant, user to admin, CI to production, provider to
  webhook.
- **Attackers**: the anonymous internet; a signed-in user (the most common
  and the most underestimated, since sign-up is often open); another tenant;
  an insider with partial access; a compromised dependency or CI action;
  someone holding a stolen token.

| Threat | Ask |
|---|---|
| Spoofing | Can someone act as another user or service (forged tokens, unsigned webhooks, trusted headers)? |
| Tampering | Can a client change what it must not (price, role, owner id, signed data, stored files)? |
| Repudiation | Are sensitive actions recorded with who, when and from where? |
| Information disclosure | Can data reach the wrong person (other tenants, logs, errors, client bundles)? |
| Denial of service | Can one request cost minutes or gigabytes (no limits, heavy regexes, unbounded queries)? |
| Elevation of privilege | Can a user become an admin, or input become code on the server? |

## 3. Map the routes, then trace input to sinks

List every entry point with who may call it, then follow the input.

```sh
rg -n '\b(app|router|api|server|fastify)\.(get|post|put|patch|delete|all|use|route)\(' -t js -t ts .
rg -n '@\w+\.(get|post|put|patch|delete|route)\(|\b(re_)?path\(' -t py .
rg -n 'HandleFunc\(|\.(Handle|GET|POST|PUT|PATCH|DELETE|Get|Post|Put|Patch|Delete)\(' -t go .
rg -n 'Route::|@(Get|Post|Put|Patch|Delete|Request)Mapping|\.route\("' -t php -t java -t kotlin -t rust .
rg -l '^\s*["\x27]use server["\x27]' -t ts -t js .
```

Also `app/**/route.ts` and `pages/api/**` in Next.js, `config/routes.rb`,
`urls.py`, `routes/*.php`. A server action is a public POST endpoint: each
one checks the session itself.

- **Sources**: body, query, path parameters, headers (including `Host` and
  `X-Forwarded-For`), cookies, file names and contents, webhook and queue
  payloads, rows other users wrote (stored input), third-party responses,
  model output.
- **Sinks**: SQL and NoSQL queries, shell, file paths, templates, HTML,
  redirects, outbound requests, deserialisers and XML parsers, `eval`,
  regexes, object merges, headers and logs.
- Work sink-first on a large codebase (grep the sinks, walk the callers back
  to a source); source-first when reviewing one route or one change.
- On every route: who may call it. On every record it loads: whether it
  belongs to the caller (`security-auth`).
- Then the parts that cut across routes: secrets and config
  (`security-secrets`), headers, CORS and cookies (`security-web` section
  12), dependencies and CI (`security-supply-chain`), deployment
  (`security-infra`).

A first sweep; the full patterns per class are in `security-web`:

```sh
rg -n 'child_process|shell=True|os\.system|popen\(|eval\(|new Function|pickle\.loads?|yaml\.load\(|unserialize\(|readObject\(' .
rg -n 'innerHTML|dangerouslySetInnerHTML|v-html|\{@html|\|\s*safe\b|mark_safe|html_safe|\{!!' .
rg -n 'queryRawUnsafe|\.raw\(|whereRaw|execute\(\s*f["\x27]|fmt\.Sprintf\(\s*"\s*(?i:select|insert|update|delete)\b' .
rg -n 'redirect\(|sendFile|send_file|createReadStream|fetch\(|axios|requests\.(get|post)|http\.Get|urlopen' .
```

## 4. Tools give leads, reading gives findings

Check what is installed (`command -v semgrep gitleaks trivy osv-scanner`),
run what fits, and report what was not run. Never install tools into the
project.

| Area | Tool |
|---|---|
| Code patterns | `semgrep scan --config p/security-audit --config p/secrets --metrics off` (registry rules need network) |
| Per language | `bandit -r . -ll` (Python), `gosec ./...` (Go), `brakeman -q` (Rails), `psalm --taint-analysis` (PHP) |
| Data flow, when set up | CodeQL: `codeql database create`, then `codeql database analyze` |
| Secrets | gitleaks, trufflehog (`security-secrets`) |
| Dependencies | `npm audit`, `pip-audit`, `govulncheck ./...`, `cargo audit`, `osv-scanner` (`security-supply-chain`) |
| Config and pipelines | `trivy config .`, `checkov -d .`, `hadolint Dockerfile`, `zizmor .github/workflows` (`security-infra`) |

- Every hit is read in context before it becomes a finding: most are tests,
  dead code, constant input or already escaped.
- A clean scan proves little. No scanner knows that invoice 42 belongs to
  someone else: authorisation and business logic are read by hand.

## 5. Confirm reachability

For each lead, answer in order; stop when an answer ends it.

1. **Registered and deployed**: mounted on a route in the production build,
   not dead code, a test helper, or behind a flag that is off.
2. **Who reaches it**: anonymous, any account, a role, or an internal network
   only (internal is often reachable through SSRF or a proxy that forwards
   everything).
3. **The input arrives as the attacker sent it**: no schema that rejects it,
   no type that makes it harmless (an integer path parameter cannot carry
   SQL), no escaping or parameterisation on the way.
4. **Preconditions**: a production setting, a victim who clicks (XSS, CSRF),
   a race window, a second weakness.
5. **Effect**: what the sink does with it here, in this code.

Confirm by reading the path end to end, by the project's own tests, or by a
local run in a scratch environment where the harness allows it; never
against production or someone else's service. Then label it:

- **Confirmed**: every step from source to sink read, or shown locally.
- **Likely**: traced, but one link depends on something not visible (a
  deploy setting, a proxy rule).
- **Suspected**: a pattern and a plausible path, not traced. Listed apart,
  as questions.

## 6. Rate severity

Impact times likelihood, for this system: what an attacker gains, and how
easily the path is reached.

| Severity | Impact and reach | Examples |
|---|---|---|
| Critical | Code execution, all data, admin or cloud takeover, money moved at will; anonymous or any account, little effort | SQL injection in a public search; an unauthenticated admin API; tokens accepted without a verified signature; a live production cloud key in a public repository |
| High | Other users' data or accounts, real money; needs an account or one victim action | IDOR across tenants on invoices; stored XSS in a page staff open; SSRF reaching cloud metadata; a reset token that never expires |
| Medium | Limited data or integrity, or conditions an attacker can arrange | CSRF on a settings change; reflected XSS under a partial CSP; no rate limit on sign-in; an open redirect in the sign-in flow |
| Low | Small disclosure, or a missing defence with no bug to exploit through it | Account enumeration; missing security headers; framework versions in responses |
| Informational | No direct risk; hygiene | An outdated package whose flaw is not reachable; a good practice absent |

- Raise it for: no authentication needed, every tenant affected, sensitive
  data (health, payments, identity documents), a public repository, a chain
  (an open redirect in an OAuth flow can carry codes away; self-XSS plus
  login CSRF runs script in a victim's browser).
- Lower it for: admin only (never to zero: admins get phished, insiders
  exist), preconditions an attacker cannot arrange, a control that actually
  blocks the path. A WAF rule or "nobody knows the URL" is not one.
- An advisory's CVSS rates the library's worst case, not this use of it:
  re-rate by reachability (`security-supply-chain`). When the owner wants
  CVSS, add a v4.0 vector beside the rating.

## 7. Report each finding

One finding per root cause, with every location under it:

```markdown
### [High] Invoices readable across organisations (IDOR)
- Location: src/invoices/routes.ts:42, src/invoices/repo.ts:88
- Class: broken access control, CWE-639
- Impact: any signed-in user can download any organisation's invoices
  (names, addresses, amounts) by changing the id in the URL.
- Reach: any account; sign-up is open; ids are sequential.
- Evidence: the route checks the session but not ownership; the query
  filters by id alone (repo.ts:88).
- Fix: scope the query by the caller's organisation and return 404
  otherwise; add a test that a second organisation gets 404
  (`backend-auth`, section 6).
- Confidence: confirmed by reading; not run against a live instance.
```

What an attacker could do is said in words. When the owner needs a proof,
describe the test that would show it ("a request for organisation B's
invoice with organisation A's session returns 200"), never a payload,
request or script.

The report, in order: a summary (counts by severity, the three risks that
matter most in a sentence each); scope, threat model, method, tools run with
versions and tools not run; findings by severity, confirmed before likely;
suspicions, each with the question that would settle it; what was not
reviewed; hardening suggestions, labelled as such and short.

## 8. Secrets found along the way

- Never print one: not in the report, the todo list, a command line or a
  quoted excerpt. Search with output that hides them: `rg -l` (files),
  `rg -c` (counts), `rg -n -r '[REDACTED]' 'pattern' .` (line numbers with the
  match replaced). A `.env` file's names only: `cut -d= -f1 .env`.
- Report the path and line (or the commit and path when it is only in
  history), the kind of secret and its provider, whether it looks live (a
  live prefix, recent commits) or test, and that it must be revoked and
  rotated. At most the provider's prefix (`sk_live_`) appears.
- Never try a found key against its service to see whether it works unless
  the owner has said so: using a found credential is using it.
- Formats, tools and the response order: `security-secrets`.

## 9. Discipline

- No finding from a pattern alone. `exec(` in the code is a lead; a finding
  names who controls what reaches it.
- No generic advice as findings. "Consider MFA" is a hardening suggestion
  unless the context (admin power, money) makes its absence a risk you can
  state.
- Say what was not reviewed (infrastructure outside the repository, the
  mobile app, third-party settings, areas skipped for time) and what could
  not be run.
- Note a good practice only when it helps calibrate ("every query goes
  through the ORM with parameters; the raw query in reports.ts:31 is the
  exception").
- "No issues found" comes with its scope and method, never as "secure".
- Never a working exploit: the class and the shape of the input in words
  ("a file name containing parent-directory segments"), not a payload.

## 10. Which skill for which area

| Area | Skill |
|---|---|
| Access control, injection, XSS, CSRF, SSRF, uploads, redirects, races, misconfiguration | `security-web` |
| Passwords, sessions, JWT, OAuth, MFA, reset tokens, API keys, permissions, tenants | `security-auth` |
| Leaked keys, scanners, reporting and rotating, secret storage | `security-secrets` |
| Dependencies, lockfiles, install scripts, licences, SBOMs, CI pipelines | `security-supply-chain` |
| Cloud IAM, exposure, containers, Kubernetes, TLS, logging, backups | `security-infra` |
| Encryption, signatures, hashing, randomness, keys, certificate checks | `security-crypto` |
| Personal data, logs, retention, consent, rights, GDPR and similar laws | `security-privacy` |
| Fixes on the server | `backend-security`, `backend-auth` |
| Fixes in the browser | `frontend-security` |
| Fixes in pipelines and infrastructure | `devops-security` |

## Check it

- Every location exists at the reviewed commit: open it again
  (`sed -n '80,95p' path`) and check the line numbers.
- Every route in the map has an answer for authentication and authorisation,
  including webhooks, uploads, server actions and admin routes.
- Each severity has a sentence saying why: the gain and the reach.
- Confirmed and suspected findings are in separate lists.
- Read your report for secret formats (`AKIA`, `sk_live_`, `ghp_`,
  `-----BEGIN`, long base64 strings) and for anything that reads as a
  ready-to-run attack, before sending it.
- The report names the tools run and their versions, the ones not run, and
  what was out of scope.

## Avoid

Generic advice dressed as findings; a finding with no path and line; a
critical for a theoretical issue behind an admin login; a pattern match
reported as a vulnerability; scanner output pasted as the report; a working
exploit or payload; a secret printed past its prefix; a found key tried
against its service; the IDOR missed while headers are listed; "no
vulnerabilities" without a scope; confirmed issues and hunches in one list.
