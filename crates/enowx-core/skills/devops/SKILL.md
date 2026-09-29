---
name: devops
description: "Building and running software: pipelines, containers, deployments, infrastructure as code, observability, security and networking. How to read what a project already has, the principles (reproducible, automated, observable, reversible, least privilege), the defaults, and which skill holds each. Read before any CI, container, deploy or infrastructure work."
---

# Building and running software

The generated version is easy to spot: a Dockerfile that copies the whole
repository into an image and runs it as root; a CI job that installs
everything uncached on every push and takes twenty minutes; a `.env` with live
keys committed; `latest` tags everywhere, so nobody knows what is running; a
deploy that swaps containers with no health check and no way back; logs that
exist only in `docker logs` on one server; Postgres and an admin panel on a
public port. This skill is how to read what a project already runs, the
principles every change keeps, the safety rules for an agent, the defaults
when there is nothing, and which `devops-*` skill holds each part.

## 1. Read what is there before changing it

| Look for | It tells you |
|---|---|
| `Dockerfile*`, `compose*.yaml`, `.dockerignore` | How it is built and run, the base images, the user it runs as |
| `.github/workflows/`, `.gitlab-ci.yml`, `.circleci/`, `Jenkinsfile` | What is checked before merge, which secrets CI holds, what deploys and when |
| `fly.toml`, `vercel.json`, `netlify.toml`, `render.yaml`, `railway.json`, `wrangler.jsonc`, `Procfile`, `config/deploy.yml` (Kamal) | The hosting platform and its settings |
| `k8s/`, `deploy/`, `charts/`, `Chart.yaml`, `kustomization.yaml`, Argo CD or Flux objects | Kubernetes, and whether Git drives it |
| `infra/`, `terraform/`, `*.tf`, `Pulumi.yaml`, `ansible/` | Infrastructure as code, where state lives, which environments exist |
| `.env.example`, the config module | The variables the app needs, by name |
| `Makefile`, `justfile`, `Taskfile.yml`, `package.json` scripts, `mise.toml`, `.tool-versions`, `.nvmrc`, `.python-version` | How people build, test and run it, and the pinned tool versions |
| `renovate.json`, `.github/dependabot.yml` | How dependencies stay current |

- Trace one change from commit to production: the workflow that runs, what it
  builds, where the artefact is stored, what deploys it, what says it is
  healthy, how it would be rolled back. The gaps are usually the real work.
- Dashboards hold settings the repository does not (Coolify, Vercel, Render,
  cloud consoles: health checks, variables, volumes, domains, scaling). Read
  them through the platform's CLI or API when you can; say what you could not
  see.
- Keep the project's tools. A second CI system, a second way of building
  images, or Terraform beside the team's Pulumi is a defect, not a choice.

## 2. Principles every change keeps

- **Reproducible.** The same commit builds the same artefact: lockfiles
  committed and installed frozen (`npm ci`, `pnpm install --frozen-lockfile`,
  `uv sync --locked`, `cargo build --locked`), tool versions pinned, base
  images pinned by version and digest, CI actions by commit SHA. Build once,
  then promote that image by digest from staging to production; never
  rebuild per environment.
- **Automated.** Every step is code: workflows, Dockerfile, migrations,
  infrastructure. A step that lives in someone's shell history or a console
  click will be skipped, or done differently, next time.
- **Observable.** Each service writes structured logs to stdout, answers a
  liveness and a readiness check, exposes the basic metrics, and has at least
  one alert that reaches a person (`devops-observability`; the app side is in
  `backend-observability`).
- **Reversible.** Each deploy can be undone in one step, and schema changes
  keep the previous version working (`devops-deploy`, `database-migrations`).
- **Least privilege.** CI tokens, cloud roles, database users and containers
  get only what the job needs, for as short a time as possible
  (`devops-security`).
- **Secrets never in git, images or logs.** They come from the platform's
  secret store at runtime; the repository holds their names, in
  `.env.example`.
- **Small blast radius.** Separate accounts or projects per environment,
  separate state per component, one risky change at a time.
- **Fail in CI, not in production.** Whatever can run before merge (lint,
  types, tests, the image build, `terraform plan`, manifest validation, a
  scan with a threshold) runs before merge and blocks it.

## 3. Safety rules for an agent

- Against anything live (a server, a cluster, a cloud account, DNS, a
  production database) run read-only commands unless the task says
  otherwise: `kubectl get`, `describe` and `logs`, `terraform plan`,
  `docker ps` and `logs`, `dig`, `curl -I`. The `bash` tool may refuse
  commands that change things; that is intended.
- Never run `terraform apply` or `destroy`, `tofu apply`, `pulumi up`,
  `kubectl apply`, `delete` or `rollout restart`, `helm upgrade`, a platform
  deploy (`fly deploy`, `vercel --prod`, a Coolify redeploy), a DNS change, a
  migration against production, or `docker system prune` on a server, unless
  the task says to. Prepare the change, validate it, show the plan or diff.
- Dry runs first: `terraform plan`, `kubectl diff` or
  `kubectl apply --dry-run=server`, `helm template`,
  `ansible-playbook --check --diff`, `docker compose config`.
- To learn whether a secret is set, test for it
  (`test -n "$DATABASE_URL" && echo set`); never print it, commit it, paste it
  into a file or quote it in the report.
- Anything destructive or hard to undo (removing a volume, deleting a bucket
  or a database, rotating a key in use, force-pushing, changing a firewall or
  a certificate others depend on) waits for the user's explicit go-ahead, with
  what it destroys named.
- In an incident, stabilise first (roll back, scale, restart) with the user's
  agreement; investigate after.

## 4. Defaults for a project that has nothing

| Need | Default | Choose otherwise when |
|---|---|---|
| CI | GitHub Actions (`devops-ci`) | The code is on GitLab, or the organisation runs something else |
| Image | Multi-stage Dockerfile, slim or distroless runtime, non-root (`devops-containers`) | The platform builds from source (Vercel, Netlify, Cloudflare) |
| Hosting | Vercel, Netlify or Cloudflare for frontends; Fly.io, Render, Railway, Cloud Run or a self-hosted Coolify for containers (`devops-platforms`) | Many services and teams, or hard compliance needs: managed Kubernetes (`devops-kubernetes`) |
| Database | Managed Postgres with backups and point-in-time recovery | A single-user or local app: SQLite, backed up |
| Infrastructure | Terraform or OpenTofu with remote state, once there is more than one platform config file (`devops-iac`) | One PaaS app: its config file is enough |
| Releases | Deploy `main` when checks pass, rolling, gated on health, previous image kept (`devops-deploy`) | Regulated or release-train products: tags and approvals |
| Telemetry | JSON logs to stdout, OpenTelemetry for traces and metrics, an error tracker, an uptime check from outside (`devops-observability`) | The organisation has a stack: use it |
| Dependencies | Renovate with `config:best-practices`, grouped updates, a minimum release age (`devops-security`) | Dependabot is set up and working |
| Secrets | The platform's secret store; a manager (a cloud secret manager, 1Password, Doppler, Infisical, Vault or OpenBao) once several services share them | |
| DNS and TLS | Automatic certificates (Caddy, Traefik, the platform, cert-manager); Cloudflare or the registrar's DNS (`devops-networking`) | |

Say in the report which default you took and why, one line each.

## 5. Done means it builds and was checked

The pipeline ran green or, where it cannot run here, every file was checked
with its own tool:

| File | Check with |
|---|---|
| GitHub workflows | `actionlint`, `zizmor` |
| Dockerfile | `hadolint Dockerfile`, then a real `docker build` |
| Compose file | `docker compose config -q` |
| Terraform or OpenTofu | `terraform fmt -check -recursive`, `terraform validate`, `tflint`, `terraform plan` |
| Kubernetes manifests | `kubeconform -strict -summary`; with access, `kubectl diff` or `kubectl apply --dry-run=server` |
| Helm chart | `helm lint`, `helm template` piped to `kubeconform` |
| Nginx or Caddy | `nginx -t`, `caddy validate --config Caddyfile` |
| Shell scripts | `shellcheck` |

A tool that is not installed can often run from its image
(`docker run --rm -i hadolint/hadolint:v2.15.1 < Dockerfile`); otherwise say
it was not run. And:

- The rollback path is written down: the command or button, what it restores,
  and what it does not (data written since).
- New secrets and variables are listed by name, with where each is set.
- The report separates what was validated, what was actually run, and what
  could not be (no Docker daemon, no credentials, no cluster access).

## 6. Which skill for which job

| Job | Skill |
|---|---|
| Principles, agent safety, defaults, what done means | `devops` (this one) |
| Pipelines: jobs, caching, permissions, OIDC, releases, previews | `devops-ci` |
| Dockerfiles, base images, Compose for local development | `devops-containers` |
| Releasing: environments, health, strategies, migrations, rollback | `devops-deploy` |
| Where it runs: PaaS, serverless, self-hosted, big clouds, managed data | `devops-platforms` |
| Manifests, charts, autoscaling, RBAC, GitOps | `devops-kubernetes` |
| Terraform, OpenTofu, Pulumi, Ansible | `devops-iac` |
| Logs, metrics, traces, SLOs, alerts, error tracking | `devops-observability` |
| Secrets, supply chain, identities, exposure, hardening, incidents | `devops-security` |
| DNS, TLS, proxies, CDNs, tunnels, load balancers | `devops-networking` |
| App side: config at start, request logs, health endpoints, shutdown | `backend-observability` |

Read the ones for the parts you touch, when you reach them.

## 7. The report

- What changed, file by file, and why.
- How it was validated: the commands and what they returned.
- What the user must do that you could not: create the secret
  `STRIPE_SECRET_KEY` in the production environment, switch on a dashboard
  setting, approve a plan, run a migration.
- How to roll it back.
- Risks and follow-ups, short.

## Check it

- `git status` and `git diff --cached --name-only`: no `.env`, private key,
  `*.tfstate`, kubeconfig or `*.tfvars` holding values is staged;
  `gitleaks git --pre-commit --redact --staged` finds nothing.
- No floating versions in what you wrote:
  `grep -rnE ':latest|@main|@master' Dockerfile* compose*.yaml .github k8s`
  prints nothing you added, and every `uses:` ends in a 40-character SHA with
  a version comment.
- Each file passed its checker from section 5, and the image was built at
  least once with `docker build`.
- The report names the rollback path and every new variable, and says what
  was not run.

## Avoid

Root containers built from the whole repository; `latest` tags and actions
pinned to a branch; secrets in git, in image layers, in CI logs or in the
report; a CI run that reinstalls everything and takes twenty minutes; a
deploy with no health check and no rollback; migrations raced by every
instance at boot; logs nobody can search; a database, Docker socket or admin
panel reachable from the internet; applying, deploying or changing DNS
against production because it seemed the obvious next step; a second tool
beside the one the project already uses; "done" for a pipeline or image that
never ran.
