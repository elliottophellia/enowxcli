---
name: security-infra
description: "Reviewing infrastructure and deployment security: cloud IAM and public exposure, storage buckets, security groups, instance metadata, container and Kubernetes privileges, network policies, TLS configuration, exposed admin panels and databases, logging and audit trails, and backup protection. Read before reviewing cloud, container or server configuration."
---

# Infrastructure and deployment review

The naive review runs a cloud scanner and pastes four hundred lines, or
"fixes" a security group live. It misses the PostgreSQL port that Docker
published on every interface past the firewall, the Docker socket mounted
into the web container, the CI role with `*` on `*`, IMDSv1 one SSRF away
from the instance's keys, and backups in a bucket anyone can list. This is
what to check in configuration and with read-only commands, how to rate it,
and the rule that you never change live infrastructure. The rules to build by
are in `devops-security`; the audit method in `security`.

## 1. Rules of engagement

- Read the configuration in the repository first: Terraform or OpenTofu,
  CloudFormation, CDK, Pulumi, Helm, Kubernetes manifests, Dockerfiles,
  compose files, nginx, Caddy or Traefik config, Ansible, platform files
  (`fly.toml`, `vercel.json`, `render.yaml`). Say whether you reviewed code
  or what runs: they drift.
- Live checks only with credentials the owner gave for it, read-only: AWS
  `SecurityAudit` plus `ViewOnlyAccess`, GCP `roles/iam.securityReviewer`
  plus `roles/viewer`, Azure Reader plus Security Reader. First
  `aws sts get-caller-identity`, to know which account you are in.
- Describe, list and get only: never `apply`, `create`, `put`, `modify`,
  `delete`, `kubectl apply`, `edit` or `exec`, `docker stop` or `rm`. No port
  scans of hosts nobody asked you to test.
- No secret values: not `aws secretsmanager get-secret-value`,
  `kubectl get secret -o yaml`, plain `docker inspect` (it prints the
  environment), `terraform output` or `terraform show` (state holds secrets).
  `kubectl get secrets` lists names; `docker inspect --format` shows only
  the fields asked for.

## 2. Identity and access

- **Wildcards**: `"Action": "*"` or `"s3:*"` with `"Resource": "*"`;
  `iam:PassRole` on `*`; `AdministratorAccess` on a workload or CI role; a
  role able to change its own policies (`iam:PutRolePolicy`,
  `iam:AttachRolePolicy`, `iam:CreatePolicyVersion`), which is admin in
  disguise.
- **Trust**: `"Principal": "*"` or `{"AWS": "*"}` with no condition; a CI
  OIDC trust on `token.actions.githubusercontent.com` without a `sub`
  condition naming the repository and branch or environment, and `aud`
  equal to `sts.amazonaws.com`.
- **Keys**: long-lived access keys for workloads instead of instance
  profiles, ECS task roles, EKS Pod Identity or IRSA, GCP workload identity,
  Azure managed identities; keys unused or older than 90 days; a root user
  with access keys or no MFA (`aws iam get-account-summary`:
  `AccountAccessKeysPresent`, `AccountMFAEnabled`).
- Humans through SSO with MFA; no shared users; break-glass accounts
  monitored.
- **GCP**: `roles/owner` or `roles/editor` on service accounts (the default
  compute service account has Editor), downloaded service account keys,
  `allUsers` or `allAuthenticatedUsers` bindings. **Azure**: Owner or
  Contributor at subscription scope for applications, client secrets where a
  managed identity would do.
- Tools: IAM Access Analyzer (public, cross-account and unused access),
  Cloudsplaining for policies, Prowler.

```sh
rg -n '"Action"\s*:\s*"(\*|[a-z0-9-]+:\*)"|"Resource"\s*:\s*"\*"|"Principal"\s*:\s*("\*"|\{\s*"AWS"\s*:\s*"\*")|iam:PassRole|AdministratorAccess' .
rg -n 'actions\s*=\s*\[\s*"\*"|token\.actions\.githubusercontent\.com|roles/(owner|editor)|allUsers|allAuthenticatedUsers' -g '*.tf' -g '*.json' -g '*.y*ml' .
```

## 3. Public exposure

- Rules open to `0.0.0.0/0` or `::/0` on admin and data ports: 22 (SSH),
  3389 (RDP), 5432 (PostgreSQL), 3306 (MySQL), 1433 (SQL Server), 27017
  (MongoDB), 6379 (Redis), 9200 (Elasticsearch, OpenSearch), 5601 (Kibana),
  11211 (memcached), 2375 and 2376 (Docker API), 6443 (Kubernetes API),
  10250 (kubelet), 2379 (etcd), 15672 (RabbitMQ), 9090 (Prometheus), 8080
  (Jenkins and many dashboards).
- **Docker and the firewall**: `ports: - "5432:5432"` binds every interface,
  and Docker's own iptables rules bypass ufw. Publish on
  `127.0.0.1:5432:5432` or not at all (an internal network). The commonest
  exposure on a single server.
- **Storage**: bucket policies with `"Principal": "*"`, public ACLs, Block
  Public Access turned off (on by default for new S3 buckets since 2023);
  GCS `allUsers`; Azure containers with anonymous access; public EBS or RDS
  snapshots and AMIs; presigned URLs to private data valid for days.
- **Admin panels on the internet**: database consoles (phpMyAdmin, Adminer,
  pgAdmin), queue and cache dashboards, the Kubernetes dashboard, Grafana
  with default credentials, deployment panels without MFA.
- **Dangling DNS**: a CNAME to a deleted bucket, app or page can be claimed
  by someone else (subdomain takeover).

```sh
rg -n '0\.0\.0\.0/0|::/0' -g '*.tf' -g '*.json' -g '*.y*ml' .
rg -n '^\s*-\s*"?\d+:\d+' -g '*compose*.y*ml' .     # ports published on every interface
aws ec2 describe-security-groups --query "SecurityGroups[?IpPermissions[?contains(IpRanges[].CidrIp, '0.0.0.0/0')]].[GroupId,GroupName]" --output text
aws s3api get-public-access-block --bucket name
```

## 4. Instance metadata

- IMDSv2 required (`HttpTokens: required`) with a hop limit of 1 on hosts
  (2 only where containers must reach it and have no role of their own);
  Terraform `metadata_options { http_tokens = "required" }`; the account
  default set with `aws ec2 modify-instance-metadata-defaults` (the owner's
  change to make).
- `aws ec2 describe-instances --query "Reservations[].Instances[].[InstanceId,MetadataOptions.HttpTokens,MetadataOptions.HttpPutResponseHopLimit]" --output text`
- GCP and Azure metadata need a header (`Metadata-Flavor: Google`,
  `Metadata: true`): that stops a naive SSRF, not one that can set headers.
- Pods with their own identity (EKS Pod Identity or IRSA, GKE Workload
  Identity) and a network policy blocking `169.254.169.254`. The SSRF side
  is in `security-web` section 5.

## 5. Containers

- **Privilege**: no `USER` in the Dockerfile, or `USER root`; `privileged`;
  `cap_add` of `SYS_ADMIN`, `NET_ADMIN` or `ALL`; `network_mode: host`,
  `pid: host`; seccomp or AppArmor `unconfined`; the Docker socket mounted
  (`/var/run/docker.sock` is root on the host); host paths such as `/`,
  `/etc` or `/root` mounted; a writable root filesystem where
  `read_only: true` with a `tmpfs` would do.
- **Images**: `latest` tags, unknown registries, `ADD` from a URL, secrets in
  `ENV` or layers (`security-secrets`), compilers left in the runtime image.
  grype, `docker scout cves` and dockle scan images as trivy does; rootless
  Docker or Podman narrows what a breakout gains.

```sh
rg -n 'privileged|docker\.sock|cap_add|SYS_ADMIN|network_mode:\s*host|pid:\s*host|unconfined|hostPath|hostNetwork|hostPID|allowPrivilegeEscalation:\s*true|runAsUser:\s*0' .
rg --files-without-match '^USER\s' -g '*Dockerfile*' .
hadolint Dockerfile && trivy config . && trivy image name:tag
docker ps --format '{{.Names}} {{.Image}} {{.Ports}}'
docker inspect --format '{{.Name}} user={{.Config.User}} privileged={{.HostConfig.Privileged}} binds={{json .HostConfig.Binds}}' $(docker ps -q)
```

## 6. Kubernetes

- **RBAC**: `cluster-admin` bound to service accounts or to
  `system:authenticated`, `system:unauthenticated` or `system:anonymous`;
  wildcard verbs or resources; `get`, `list` or `watch` on `secrets` granted
  widely (`list` returns the values); `create` on pods (runs as any service
  account in the namespace), `pods/exec`, `escalate`, `bind`, `impersonate`,
  `nodes/proxy`.
- `automountServiceAccountToken: false` where pods never call the API.
- **Pod security**: namespaces labelled
  `pod-security.kubernetes.io/enforce: restricted` (`baseline` at least);
  `runAsNonRoot: true`, `allowPrivilegeEscalation: false`,
  `readOnlyRootFilesystem: true`, capabilities dropped (`ALL`), seccomp
  `RuntimeDefault`; no `hostNetwork`, `hostPID`, `hostIPC` or `hostPath`.
- **NetworkPolicies**: a default deny for ingress (and egress where
  practical) in each namespace. They do nothing unless the CNI enforces them
  (Calico and Cilium do, plain flannel does not).
- **Secrets**: base64 in etcd unless encryption at rest is configured
  (managed services differ); External Secrets or the CSI secrets driver
  rather than manifests in git; nothing secret in ConfigMaps.
- **Admission**: Kyverno, OPA Gatekeeper or the built-in
  ValidatingAdmissionPolicy (GA in 1.30) enforcing the above.
- **Control plane**: the API server private or limited to known networks,
  anonymous auth off, kubelet with `--anonymous-auth=false` and webhook
  authorisation, etcd and dashboards not exposed.

```sh
kubectl get clusterrolebindings -o wide
kubectl auth can-i --list --as=system:serviceaccount:app:default -n app
kubectl get ns -L pod-security.kubernetes.io/enforce
kubectl get networkpolicy -A
```

Tools: `kubescape scan`, `trivy k8s --report summary`, `kube-linter lint`
and Polaris for manifests; `kube-bench` (the CIS benchmark) runs as a Job
on the nodes, so the owner starts it. kube-hunter is unmaintained and probes
the network: only when the owner asks.

## 7. TLS

- TLS 1.2 minimum, 1.3 preferred; no SSLv3, TLS 1.0 or 1.1; Mozilla's
  "intermediate" profile for ciphers; a full chain; renewal automated
  (public certificate lifetimes fall to 200 days from March 2026, 100 in
  2027 and 47 in 2029, so manual renewal will fail).
- HSTS on HTTPS responses: `max-age=31536000; includeSubDomains`, and
  `preload` once every subdomain serves HTTPS.
- Plain text inside: database connections with `sslmode=disable` or
  `require` (encrypts, verifies nothing; `verify-full` checks the host),
  Redis without TLS between hosts, service-to-service HTTP across networks
  carrying tokens.
- An origin behind a CDN reachable directly by IP skips the WAF and the TLS
  settings: allow only the CDN's ranges, authenticated origin pulls, or a
  tunnel.
- For hosts in scope, the last two commands below; testssl.sh for a full
  pass, with permission.

```sh
rg -n 'sslmode=(disable|allow|prefer|require)\b|ssl\s*[:=]\s*false|tls\s*[:=]\s*false|InsecureSkipVerify|rejectUnauthorized:\s*false' .
openssl s_client -connect host:443 -servername host -tls1_1 </dev/null   # must fail
curl -sI https://host | grep -i strict-transport-security
```

## 8. Databases, caches and the edge

- No public endpoint (RDS `PubliclyAccessible`, a managed database allowing
  `0.0.0.0/0`); authentication on (Redis `requirepass` or ACLs with
  `protected-mode yes`, MongoDB `security.authorization: enabled`,
  Elasticsearch security, on by default since 8.0); private interfaces; TLS;
  encryption at rest; default passwords changed.
- The application's database user is not a superuser or the schema owner;
  migrations run as another user; reporting reads through a read-only one.
- nginx: `autoindex on`; the project root as `root` (serving `.env` and
  `.git`); a `location` without a trailing slash with an `alias` that has
  one lets a request climb a directory; `server_tokens on` (low).
- Traefik `api.insecure=true` publishes its dashboard without auth; Caddy's
  admin API stays on localhost.
- The app trusts `X-Forwarded-For` only from its own proxy
  (`security-web` section 13); body limits and timeouts at the edge.

```sh
rg -n 'autoindex\s+on|\balias\s|server_tokens\s+on|api\.insecure|PubliclyAccessible|publicly_accessible\s*=\s*true|protected-mode\s+no' .
```

## 9. Logging and audit

- AWS CloudTrail as a multi-region or organisation trail with log file
  validation, delivered to a bucket in a separate account; GuardDuty on.
  GCP Admin Activity logs are always on, Data Access logs are off by
  default. Azure Activity Log exported, Defender for Cloud on. Kubernetes
  audit logging on (EKS control plane logs are off by default).
- Retention long enough for an investigation (a year is common: breaches
  are often found months later), and logs out of reach of the accounts they
  record.
- Alerts on root or break-glass sign-in, IAM and security group changes,
  logging turned off, new access keys (the CIS benchmarks list them).
- Application audit trails: `backend-auth` section 7; personal data in
  logs: `security-privacy`.

## 10. Backups

- Encrypted, in a separate account or project that production credentials
  cannot write to or delete from; immutable where ransomware matters (S3
  Object Lock in compliance mode, AWS Backup Vault Lock, GCS retention
  locks, Azure immutable blob storage).
- Three copies, two media, one off-site; a dump never in the web root or a
  public bucket.
- A restore tested on a schedule, with the date of the last one known; a
  backup never restored is a hope. Retention matches the privacy policy.

## 11. Tools and rating

| Tool | Covers |
|---|---|
| Prowler (`prowler aws`, also `azure`, `gcp`, `kubernetes`) | CIS and best-practice checks for a whole account |
| ScoutSuite | A multi-cloud configuration report |
| `checkov -d .` | Terraform, CloudFormation, Kubernetes, Helm, Dockerfiles |
| `trivy config .` | IaC and Dockerfile misconfiguration (tfsec's checks now live here) |
| kubescape, kube-bench | Kubernetes posture, CIS benchmark |
| hadolint, dockle | Dockerfile and image practice |

- **Critical**: a database, cache, Docker API or dashboard reachable from the
  internet without auth or with data; admin-equivalent credentials usable by
  an outsider; a public bucket of personal data.
- **High**: wildcard admin on an internet-facing workload; IMDSv1 behind an
  SSRF; a privileged container or the Docker socket in an internet-facing
  service; CI OIDC trust without `sub`; SSH open to the world with
  passwords.
- **Medium**: no network policies; root containers with no other flaw; TLS
  1.0 enabled; audit logging off; untested backups.
- **Low**: version banners, missing HSTS preload, tags instead of digests.

## Check it

- Every port the configuration exposes is in the report with its reason, or
  flagged for the owner to confirm.
- Each workload's identity and permissions are listed, and none is admin.
- Every finding names the file and line or the resource id, and whether it
  came from code or a live read.
- Nothing was changed: your shell history holds only reads.

## Avoid

Changing live infrastructure to "fix" something; printing secrets with
`docker inspect`, `kubectl get secret -o yaml` or `terraform output`; port
scans nobody asked for; scanner output as the report; `0.0.0.0` ports in
compose on a public server; `*` on `*`; the Docker socket in an app
container; IMDSv1; network policies without a CNI that enforces them;
backups the production role can delete.
