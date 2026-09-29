---
name: devops-iac
description: "Infrastructure as code: Terraform or OpenTofu with remote state and locking, modules, one configuration per environment, plan in CI and apply with approval, drift, importing what exists, variables and secrets, naming and tagging, policy checks, and Pulumi or Ansible where they fit. Read before writing or changing infrastructure code."
---

# Infrastructure as code

The generated version is one `main.tf` holding every resource of every
environment, local state (or a committed `terraform.tfstate` with the database
password inside), unpinned providers, `terraform apply -auto-approve` from a
laptop, a `count` list whose middle item, removed, recreates everything after
it, and a production database one typo from deletion. This is how to lay out
infrastructure code so each change is small, reviewed as a plan, applied by CI
with approval, and cannot quietly delete data. One part of devops; the whole
is in the `devops` skill.

## 1. Choosing the tool

| Tool | Choose when |
|---|---|
| Terraform (1.16) or OpenTofu (1.12) | The default: declarative HCL, every major provider. OpenTofu is the MPL-licensed fork and can encrypt state client-side; Terraform is BSL-licensed with HCP Terraform. One per repository. |
| Pulumi | The team wants TypeScript, Python or Go with real loops and tests; same plan and state model |
| CloudFormation, CDK, Bicep | The organisation standardised on one cloud's native tool |
| Ansible | Configuring servers (packages, users, files, services), not creating cloud resources |
| cloud-init, Packer | First-boot setup; baked machine images |

A single app on a PaaS needs only the platform's config file; bring in
Terraform when DNS, buckets, databases and several services must change
together.

## 2. State

```hcl
terraform {
  required_version = ">= 1.11"
  required_providers {
    aws = { source = "hashicorp/aws", version = "~> 6.0" }
  }
  backend "s3" {
    bucket       = "acme-tfstate-prod"
    key          = "network/terraform.tfstate"
    region       = "eu-west-1"
    encrypt      = true
    use_lockfile = true
  }
}
```

- Remote, locked, encrypted and versioned: S3 with `use_lockfile = true`
  (native locking; DynamoDB locking is deprecated), GCS (locks built in),
  `azurerm`, or HCP Terraform. The bucket has versioning, blocked public
  access, KMS encryption, and access for the CI roles and a break-glass admin
  only.
- State holds every attribute of every resource, passwords included. Treat it
  as a secret: never commit `*.tfstate`, never paste it into a chat or a PR.
- Split it per environment (separate accounts where possible) and per
  component (network, data, apps), so a plan covers tens of resources, not
  thousands. Share values through data sources or read-only
  `terraform_remote_state`.
- The state bucket itself comes from a small bootstrap configuration,
  documented.
- Never edit state by hand; `terraform force-unlock <id>` only when certain no
  run is active; refactor with `moved`, `removed` and `import` blocks
  (section 5), not `state mv` and `state rm`.

## 3. Layout

```text
infra/
  modules/
    network/          main.tf variables.tf outputs.tf versions.tf README.md
    service/
  live/
    staging/network/  backend.tf main.tf terraform.tfvars
    staging/app/
    prod/network/
    prod/app/
```

- Root modules per environment and component call shared modules;
  environments differ by variables, never by copied code.
- A directory per environment, not `terraform workspace`: workspaces share one
  backend and one set of credentials, so production is one `workspace select`
  away from a mistake.
- Modules are small, with typed variables (descriptions and `validation`
  blocks) and outputs, no provider blocks inside, and pinned versions:
  `source = "git::https://github.com/acme/tf-modules.git//network?ref=v1.4.0"`
  or a registry `version`.
- `.terraform.lock.hcl` is committed with hashes for every platform in use:
  `terraform providers lock -platform=linux_amd64 -platform=linux_arm64 -platform=darwin_arm64`.
- Terragrunt or Terramate only when many stacks share wiring.

## 4. Plan in CI, apply with approval

- On each PR that touches a stack: `fmt -check`, `validate`, `tflint`, a
  misconfiguration scan (`trivy config .` or `checkov -d .`), then a plan
  posted to the PR; policy on the plan JSON (`terraform show -json tfplan >
  plan.json`, then `conftest test plan.json`); a cost diff (Infracost) where
  money matters.
- Apply only from CI, only the saved plan that was reviewed, after approval
  in a protected environment, with OIDC credentials (`devops-ci`); one run
  per state at a time (the lock and a concurrency group). The plan role is
  read-only; the apply role is separate.

```yaml
      - run: terraform init -input=false
      - run: terraform plan -input=false -lock-timeout=5m -out=tfplan
      - run: terraform show -no-color tfplan > plan.txt
```

- The saved plan contains secrets: a private, short-lived artefact.
- Read the plan. Every `destroy` and `-/+` replacement is explained before
  approval; "forces replacement" on a database, a volume or a bucket stops
  the change.
- `-target` only in an emergency, followed by a full plan. Atlantis, Digger,
  HCP Terraform, Spacelift and env0 run this loop if the team wants a tool.

## 5. Existing resources and refactoring

```hcl
import {
  to = aws_s3_bucket.assets
  id = "acme-assets-prod"
}

moved {
  from = aws_instance.web
  to   = module.web.aws_instance.this
}

removed {
  from = aws_instance.legacy
  lifecycle {
    destroy = false
  }
}
```

- Adopt what exists with `import` blocks (inside modules too, since Terraform
  1.16); `terraform plan -generate-config-out=generated.tf` drafts the HCL,
  which you then tidy until the plan shows no changes.
- `moved` renames without destroying; `removed` with `destroy = false` stops
  managing a resource without deleting it.
- `for_each` over a map with stable keys, not `count` over a list: removing a
  list item shifts every later index and replaces those resources.

## 6. Variables and secrets

- `terraform.tfvars` per environment holds non-secret values and is
  committed. Secrets never go in tfvars, variable defaults or code.
- Better still, Terraform never sees the secret: RDS
  `manage_master_user_password = true` keeps the password in Secrets
  Manager. Otherwise read it from a secret manager through an `ephemeral`
  resource (Terraform 1.10 and later) into a write-only argument such as
  `password_wo` (1.11 and later), which stays out of state.
- `sensitive = true` hides a value in plan output only; it is still in state.
  `random_password` values live in state too.

## 7. Protecting data, and drift

```hcl
resource "aws_db_instance" "main" {
  identifier                  = "shop-prod"
  engine                      = "postgres"
  instance_class              = "db.t4g.medium"
  allocated_storage           = 50
  username                    = "shop"
  manage_master_user_password = true
  storage_encrypted           = true
  backup_retention_period     = 14
  deletion_protection         = true
  final_snapshot_identifier   = "shop-prod-final"

  lifecycle {
    prevent_destroy = true
  }
}
```

- `prevent_destroy` fails any plan that would destroy the resource, but not
  one where its block was deleted, so the provider's own protection stays on
  as well (`deletion_protection`, bucket versioning without `force_destroy`).
- `create_before_destroy` for things others reference (certificates, launch
  templates), so a replacement causes no gap.
- Drift: a scheduled plan per stack (nightly) with `-detailed-exitcode`; exit
  code 2 means reality differs, and it opens an issue. `plan -refresh-only`
  shows what changed outside Terraform; accept it with `apply -refresh-only`
  or let the next apply revert it. A console change made in an emergency is
  written back into code the same day.
- `ignore_changes` only for attributes something else owns (an autoscaler's
  desired count), with a comment saying what.

## 8. Naming, tagging, IAM

- Names `<project>-<env>-<component>`, lowercase with hyphens
  (`shop-prod-db`), within each service's limits (S3 bucket names are global,
  up to 63 characters).
- Tags on everything, set once on the provider:

```hcl
provider "aws" {
  region = "eu-west-1"
  default_tags {
    tags = {
      Project     = "shop"
      Environment = "prod"
      Owner       = "platform"
      ManagedBy   = "terraform"
      CostCentre  = "web"
    }
  }
}
```

- IAM in code with least privilege: `aws_iam_policy_document` data sources,
  named actions on named resources, no `"*"` on `"*"` (`devops-security`).

## 9. Pulumi and Ansible

- Pulumi: a stack per environment, `pulumi preview --diff` on PRs,
  `pulumi up` from CI after approval, state in Pulumi Cloud or a
  self-managed backend (`pulumi login s3://...`), secrets with
  `pulumi config set --secret` or ESC, `protect: true` on data stores.
- Ansible: idempotent roles, an inventory per environment (from Terraform
  outputs), `ansible-lint`, `--check --diff` before a real run, secrets in
  Ansible Vault or SOPS, `become` only where needed.

## Check it

- `terraform fmt -check -recursive`,
  `terraform init -backend=false && terraform validate` (no credentials
  needed), `tflint --init && tflint --recursive`, `trivy config .`.
- `terraform plan` with read-only credentials when you have them; every
  destroy and replacement read and explained in the report.
- `.terraform.lock.hcl` committed; no `*.tfstate`, `*.tfplan` or secret
  values in git (`gitleaks dir infra/`).
- No `apply`, `destroy`, `import` against real state or `force-unlock` unless
  the task says so.

## Avoid

Local or committed state; one state for everything; workspaces as the
boundary between production and staging; unpinned providers and modules;
secrets in tfvars, defaults or outputs without a plan for state; applies from
laptops or of an unreviewed plan; `-auto-approve` against production;
`count` over lists of named things; hand edits to state; `-target` as a
habit; data stores without `prevent_destroy` and provider deletion
protection; drift left to pile up; Ansible used to create cloud resources;
applying on your own initiative.
