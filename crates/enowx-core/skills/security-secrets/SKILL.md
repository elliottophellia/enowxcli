---
name: security-secrets
description: "Finding and handling secrets: where they leak (committed env files, config, history, logs, client bundles, CI output, images), scanning with gitleaks or trufflehog, reporting a found secret without printing it, rotation first then history cleanup, storing secrets in a manager, least privilege and scanning in CI and pre-commit. Read before auditing a repository or pipeline for secrets, or when one is found."
---

# Secrets in code, history and pipelines

The naive version greps for "password", pastes the matching line into the
report (leaking the key a second time), recommends deleting the file, and
moves on while the key stays live and in git history for good. This skill is
where secrets actually leak, the tools and patterns that find them, how to
report one without printing it, and the order of the response: rotate first,
clean up after. Storing secrets well at build and run time is also in
`devops-security` and `backend-security` section 4.

## 1. Where they leak

- **Files in the repository**: `.env`, `.env.local`, `.env.production`;
  `settings.py`, `application.yml`, `appsettings.json`, `wp-config.php`,
  `database.yml`; Rails `master.key` beside `credentials.yml.enc` (the key
  committed defeats the encryption); `docker-compose*.yml` `environment:`
  blocks; Kubernetes `Secret` manifests (base64 is not encryption); Helm
  values; `*.tfvars` and `terraform.tfstate` (state holds generated passwords
  and keys in plain text); `.npmrc` (`_authToken`), `.pypirc`, `.netrc`,
  `.git-credentials`, Docker `config.json`; keys and keystores (`*.pem`,
  `*.key`, `*.p12`, `*.pfx`, `*.jks`, `id_rsa`); cloud service account JSON;
  kubeconfigs; test fixtures, seeds and SQL dumps; notebooks (outputs keep
  what was printed); Postman environments and HAR files (headers and
  cookies); editor settings; AI tool configs with inline tokens
  (`.mcp.json`, `.cursor/mcp.json`).
- **Git history**: a deleted file is still in every clone, on every branch,
  tag and stash that has it, and in forks and pull request refs on the host.
- **Client code**: variables with public prefixes are compiled into the
  bundle (`NEXT_PUBLIC_`, `VITE_`, `REACT_APP_`, `EXPO_PUBLIC_`,
  `NUXT_PUBLIC_`, SvelteKit `$env/static/public`); source maps; mobile apps
  (anything in the binary can be extracted).
- **Build and run output**: CI logs (`set -x`, `env`, `printenv`, debug
  flags, a failing command echoing its arguments), artefacts and caches that
  picked up `.env` or `~/.npmrc`, container images (`ENV`, `ARG` values in
  `docker history`, files deleted in a later layer still present in an
  earlier one), application logs and error trackers (headers, bodies,
  connection strings in exceptions).

## 2. Scanning

Run what is installed; say what was not. Each command below keeps values out
of its output.

```sh
gitleaks git --redact -v .          # every commit on every branch (gitleaks detect before 8.19)
gitleaks dir --redact -v .          # the files on disk, tracked or not
trivy fs --scanners secret .        # masks what it finds
trufflehog git file://. --no-verification --no-update --json 2>/dev/null \
  | jq -c '{detector: .DetectorName, verified: .Verified, at: (.SourceMetadata.Data | (.Git // .Filesystem // .Docker) | del(.email))}'
```

- trufflehog prints raw values, in plain and in `--json` output alike: run it
  only through a filter that drops them, like the `jq` above (the same filter
  serves `trufflehog filesystem .` and `trufflehog docker --image name:tag`).
- It also verifies what it finds by calling the provider with it, unless
  `--no-verification` is given. That is using a found credential: only with
  the owner's permission.
- False positives are recorded, not argued with each run: gitleaks takes
  fingerprints in `.gitleaksignore` and `gitleaks:allow` comments;
  `detect-secrets` keeps a reviewed baseline file.
- Without the tools, `git` and `rg` still answer the key questions, without
  printing values:

```sh
git ls-files | rg -i '(^|/)\.env|\.pem$|\.key$|\.p12$|\.pfx$|\.jks$|id_rsa|tfstate|credentials|service.?account|kubeconfig'
git log --all --full-history --oneline --name-status -- '*.env' '.env*' '*.pem' '*.tfstate'
git log --all -S 'sk_live_' --oneline --name-only
git rev-list --all | xargs git grep -l -I -E 'AKIA[0-9A-Z]{16}|-----BEGIN [A-Z ]*PRIVATE KEY'
```

The last one walks every commit and is slow on long histories; it prints
`commit:path` only. `git log -p -S 'string'` shows the diffs that added or
removed a string, values included: read it through the redacting `rg -r`
of section 3, never raw.

## 3. Formats

A sweep of the working tree with every match replaced in the output:

```sh
rg -n --hidden -g '!.git' -r '[REDACTED]' '(AKIA|ASIA)[0-9A-Z]{16}|gh[pousr]_[A-Za-z0-9]{36}|github_pat_\w{20,}|glpat-[\w-]{20,}|xox[abprs]-[\w-]{10,}|xapp-[\w-]{10,}|(sk|rk)_live_\w{20,}|whsec_\w{20,}|AIza[\w-]{35}|GOCSPX-[\w-]{20,}|sk-(proj|svcacct|admin)-[\w-]{20,}|sk-ant-[\w-]{20,}|SG\.[\w-]{22}\.[\w-]{43}|npm_\w{36}|pypi-[\w-]{50,}|hf_\w{30,}|sb_secret_\w{20,}|dop_v1_[a-f0-9]{64}|shpat_[a-fA-F0-9]{32}|-----BEGIN [A-Z ]*PRIVATE KEY' .
rg -n --hidden -g '!.git' -r '$1:[REDACTED]@' '(\w+://[^:@\s/]+):[^@\s/]+@' .
rg -n --hidden -g '!.git' -i -r '$1[REDACTED]' '((api[_-]?key|secret|token|passw(or)?d)\w*\s*[:=]\s*["\x27])[^"\x27\s]{12,}' .
```

`rg` skips ignored files: add `--no-ignore` to check build output
(`.next/static`, `dist`, `build`) or what a Docker build context would copy.

| Prefix or marker | What it is | Treat as |
|---|---|---|
| `AKIA`, `ASIA` | AWS access key id (long-term, temporary); the secret key sits nearby | Secret pair |
| `ghp_`, `gho_`, `ghu_`, `ghs_`, `ghr_`, `github_pat_` | GitHub tokens | Secret |
| `glpat-` | GitLab personal access token | Secret |
| `xoxb-`, `xoxp-`, `xapp-`, `hooks.slack.com/services/` | Slack tokens and webhooks | Secret |
| `sk_live_`, `rk_live_`, `whsec_` | Stripe secret, restricted and webhook keys | Secret |
| `sk_test_` | Stripe test key | Low, still revoke |
| `sk-proj-`, `sk-ant-`, `hf_` | OpenAI, Anthropic, Hugging Face keys (spend money) | Secret |
| `GOCSPX-`, `"type": "service_account"` | Google OAuth client secret, service account key | Secret |
| `SG.`, `npm_`, `pypi-`, `dop_v1_`, `shpat_`, `sb_secret_` | SendGrid, npm, PyPI, DigitalOcean, Shopify, Supabase secret | Secret |
| `-----BEGIN ... PRIVATE KEY` | TLS, SSH, signing or service keys | Secret |
| `pk_live_`, `AIza` (browser), Firebase web config, Supabase anon or `sb_publishable_` | Public by design | Not a leak; check restrictions and rules |

A JWT (`eyJ...`) may be a test token or a Supabase key: its payload's `role`
claim tells `anon` (public) from `service_role` (secret) without printing it.

## 4. Reporting one

- Never the value, in any form: not in the report, the todo list, a later
  command line (shell history keeps it) or a quoted excerpt. Quote code with
  the value replaced (`const key = "sk_live_[REDACTED]"`). At most the
  provider's prefix appears; the last four characters only when two keys
  must be told apart.
- Never `cat .env`, `env` or `printenv` during an audit; `cut -d= -f1 .env`
  lists the names.
- Say: path and line (or commit, date and path when only in history), the
  kind and provider, live or test by prefix, what it can do if known (an
  AWS key in a deploy script likely deploys), who can read it (public
  repository, collaborators, CI logs, image registry), since when, and that
  it must be revoked and rotated.
- "Looks live" (live prefix, recent commit) is different from "verified":
  verification is for the owner, or with their permission.

```markdown
### [Critical] Live Stripe secret key committed
- Location: src/payments/config.ts:12, present since commit 3f2a9c1
- Kind: Stripe live secret key (sk_live_ prefix), full API access
- Exposure: public repository; also on two other branches
- Action: roll the key in Stripe now, load it from the platform's secret
  store, review Stripe logs since that commit, then decide on history cleanup
- Confidence: present in code; not tested against Stripe
```

**Rating**: critical for a live, broadly scoped credential (cloud admin,
payment secret, database owner, session or token signing key) in a public
repository, image or client bundle; high for the same in a private
repository or CI logs (every clone, runner and collaborator has it, and
repositories go public by mistake) or for a narrower live key in public;
medium for test keys and for secrets in files a pushed image would include;
informational for publishable keys, with their restrictions checked.

## 5. The response, in order

1. **Revoke and rotate** at the provider now: issue the new secret, deploy
   it, then revoke the old. Deleting it from the code does not un-leak it;
   bots scrape public repositories within minutes.
2. **Look for misuse** since the first commit: CloudTrail for an AWS key id
   (the id is an identifier, the secret key is the secret), the provider's
   logs and audit trail, unexpected spend.
3. **Remove it from the code**: read from the environment or a secret
   manager; ignore the file.
4. **Clean history** only when it matters (a public repository, a policy):
   `git filter-repo --invert-paths --path .env` or
   `git filter-repo --replace-text replacements.txt` (BFG does the same),
   then force-push every branch and tag, have everyone re-clone, and ask the
   host to purge cached views and pull request refs. Forks and old clones
   keep it; rotation is what makes it safe.
5. **Prevent the next one** (section 7).

Signing keys are a special case: rotating a session or JWT secret signs
everyone out unless keys carry an id and the old one verifies for a short
overlap; a leaked CA or code-signing key needs revocation, not only a new
key.

## 6. Where secrets live

- A secret manager or the platform's encrypted environment: AWS Secrets
  Manager or SSM Parameter Store, GCP Secret Manager, Azure Key Vault,
  HashiCorp Vault or OpenBao, Doppler, Infisical, 1Password, or the host's
  own (Vercel, Fly, Render, Coolify). In Kubernetes, the External Secrets
  Operator, Sealed Secrets or SOPS; encrypted files in git with SOPS and
  age or a KMS.
- Injected at run time, never baked into an image; separate per environment;
  readable only by the service that uses it; access audited.
- Short-lived where the platform allows: OIDC from CI to the cloud, IAM
  roles and workload identity instead of keys, Vault dynamic database
  credentials.
- In the application: read once at start (`backend-observability`), never
  logged (redaction in the logger: pino `redact`, structlog processors),
  never sent to the client or put in an error message.
- Client and mobile code holds no secret at all: a server route calls the
  third party. Obfuscating a key in an app does not hide it.

## 7. Prevention

- `.gitignore` with `.env*` and `!.env.example`; `.env.example` committed
  with every name and no real value; `.dockerignore` with `.env*` and
  `.git`; `*.tfstate*` ignored and state in an encrypted remote backend.
- A pre-commit hook: the `gitleaks` hook from
  `https://github.com/gitleaks/gitleaks`, pinned to a release tag
  (`pre-commit autoupdate`), or `gitleaks git --pre-commit --staged` in a
  plain git hook.
- CI scanning on every push and pull request, and the host's secret scanning
  with push protection (free on public GitHub repositories; GitHub Secret
  Protection for private ones; GitLab has its own).
- CI hygiene: secrets only in protected environments, never given to
  workflows from forks; passed as environment variables, not command-line
  arguments (visible to other processes); no `set -x` in steps that use
  them; masking is not a guarantee (a base64 of a secret is not masked).
- Least privilege per key (read-only where reads suffice), expiry dates on
  personal tokens (fine-grained GitHub tokens), one key per service so a
  leak is revoked without an outage elsewhere.
- Docker builds take secrets through BuildKit mounts, never `ARG` or `ENV`:

```dockerfile
# docker build --secret id=npmrc,src=$HOME/.npmrc .
RUN --mount=type=secret,id=npmrc,target=/root/.npmrc npm ci
```

## Check it

- gitleaks (or the git commands above) ran over the full history, not only
  the working tree, and the output you kept is redacted.
- `git check-ignore -v .env` names the rule that ignores it, and
  `git ls-files` lists no env, key or state file.
- The build output and, where one exists, the image were searched:
  `rg --no-ignore` on `dist` or `.next/static`, and
  `trivy image --scanners secret name:tag`.
- Your report and notes contain no secret value: search them for the
  prefixes in section 3.

## Avoid

Printing, pasting or echoing a found secret; testing it against its service
without permission; recommending deletion from the code as the fix; history
rewriting before rotation; publishable keys reported as leaks, or treated as
safe without checking their restrictions; secrets in `ARG`, `ENV`, a URL,
a client bundle or a log; one shared key for every service; a scan of the
working tree alone.
