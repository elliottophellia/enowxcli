---
name: devops-kubernetes
description: "Kubernetes when it is warranted: Deployments with requests, limits, probes and security contexts, Services, Ingress and the Gateway API, ConfigMaps and Secrets with an external secret store, autoscaling, disruption budgets, Helm versus Kustomize, namespaces and RBAC, network policies, and GitOps. Read before writing or changing Kubernetes manifests or charts."
---

# Kubernetes

The generated manifest is a Deployment of `app:latest` with one replica, no
requests or limits, no probes, running as root on a writable filesystem, next
to a committed Secret that is only base64, a `LoadBalancer` Service per app,
and `kubectl apply` from a laptop. It works until the first node drain, noisy
neighbour or leaked repository. This is what a workload needs to survive
rollouts, node loss and review, and how to package and ship it. One part of
devops; the whole is in the `devops` skill.

## 1. When, and which cluster

- Not for a few services run by a small team: a PaaS or ECS costs a fraction
  to operate (`devops-platforms`). It pays off with many services and teams,
  or where the organisation already runs it. Use a managed control plane
  (GKE Autopilot or EKS Auto Mode when nobody wants to run nodes); k3s or
  Talos only when self-hosting is a requirement.
- Upstream is at 1.37 (August 2026), three minors a year, about 14 months of
  patches each; managed providers trail by one or two. Upgrade at least
  yearly, one minor at a time, after `kubent` or `pluto` finds removed APIs.
- Ingress NGINX (`kubernetes/ingress-nginx`) was retired in March 2026, with
  no more fixes. Use the Gateway API with a maintained implementation (Envoy
  Gateway, Cilium, Istio, NGINX Gateway Fabric, Traefik, Kong, the cloud's).

## 2. The Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: api
  labels: { app.kubernetes.io/name: api }
spec:
  replicas: 3
  revisionHistoryLimit: 5
  selector:
    matchLabels: { app.kubernetes.io/name: api }
  strategy:
    rollingUpdate: { maxSurge: 25%, maxUnavailable: 0 }
  template:
    metadata:
      labels: { app.kubernetes.io/name: api }
    spec:
      serviceAccountName: api
      automountServiceAccountToken: false
      terminationGracePeriodSeconds: 30
      securityContext:
        runAsNonRoot: true
        runAsUser: 10001
        runAsGroup: 10001
        seccompProfile: { type: RuntimeDefault }
      containers:
        - name: api
          image: ghcr.io/acme/api:1.4.2@sha256:<digest>
          ports: [{ name: http, containerPort: 8080 }]
          envFrom:
            - configMapRef: { name: api-config }
            - secretRef: { name: api-secrets }
          resources: { requests: { cpu: 250m, memory: 256Mi }, limits: { memory: 512Mi } }
          startupProbe: { httpGet: { path: /healthz, port: http }, periodSeconds: 5, failureThreshold: 30 }
          readinessProbe: { httpGet: { path: /readyz, port: http }, periodSeconds: 5, failureThreshold: 3 }
          livenessProbe: { httpGet: { path: /healthz, port: http }, periodSeconds: 10, failureThreshold: 3 }
          lifecycle: { preStop: { sleep: { seconds: 5 } } }
          securityContext: { allowPrivilegeEscalation: false, readOnlyRootFilesystem: true, capabilities: { drop: ["ALL"] } }
          volumeMounts: [{ name: tmp, mountPath: /tmp }]
      volumes: [{ name: tmp, emptyDir: {} }]
      topologySpreadConstraints:
        - maxSkew: 1
          topologyKey: topology.kubernetes.io/zone
          whenUnsatisfiable: ScheduleAnyway
          labelSelector: { matchLabels: { app.kubernetes.io/name: api } }
```

- Requests always: scheduling and the autoscaler's maths use them. A memory
  limit always, 1 to 2 times the request, with the runtime sized below it
  (`--max-old-space-size`, `-XX:MaxRAMPercentage=75`, `GOMEMLIMIT`), since the
  kernel kills the process at the limit. CPU limits rarely: throttling adds
  latency even on an idle node; keep them for batch work or where policy
  demands (Go 1.25 and later read the CPU limit for `GOMAXPROCS`).
- Size requests from measurement (p95 over a week, VPA recommendations,
  `kubectl top`); in-place resize is stable since 1.35.
- Probes: startup covers the slowest boot (30 x 5s here); readiness checks
  what a request needs, with a short timeout; liveness checks only the
  process (`backend-observability`). No heavy work in any of them.
- The `preStop` sleep (stable since 1.34, no `sleep` binary needed) lets
  endpoints drop the pod before `SIGTERM`; the grace period covers the sleep
  plus the app's drain (`devops-deploy`).
- At least 2 replicas (3 for anything that matters) spread over zones, and no
  `replicas` field once an HPA owns it (GitOps and the HPA would fight). The
  image by digest, a numeric user matching the image (`devops-containers`).

## 3. Services and the Gateway API

```yaml
apiVersion: v1
kind: Service
metadata: { name: api }
spec:
  selector: { app.kubernetes.io/name: api }
  ports: [{ name: http, port: 80, targetPort: http }]
---
apiVersion: gateway.networking.k8s.io/v1
kind: HTTPRoute
metadata: { name: api }
spec:
  parentRefs: [{ name: public, namespace: gateway }]
  hostnames: ["api.example.com"]
  rules:
    - matches: [{ path: { type: PathPrefix, value: / } }]
      backendRefs: [{ name: api, port: 80 }]
```

- `ClusterIP` Services inside and one Gateway outside, not a `LoadBalancer`
  per app. The platform owns the Gateway (its namespace, HTTPS listeners,
  `allowedRoutes`); app teams own their HTTPRoutes.
- TLS through cert-manager: set `config.gatewayAPI.enabled=true` in its Helm
  values, annotate the Gateway with `cert-manager.io/cluster-issuer:
  letsencrypt`, and each HTTPS listener's `certificateRefs` names the Secret
  it fills. On an Ingress, the same annotation plus a `tls:` block.
- Timeouts, body limits and rate limits are per implementation (policies or
  filters); set them for uploads and long requests (`devops-networking`).

## 4. Configuration and secrets

- A ConfigMap change must roll the pods: Kustomize's `configMapGenerator`
  hashes the name; Helm charts add a `checksum/config` pod annotation.
- A Secret is base64, not encryption; plain Secret manifests never go into
  git. Pull them from a secret manager with External Secrets Operator, or
  keep them encrypted in git with Sealed Secrets or SOPS:

```yaml
apiVersion: external-secrets.io/v1
kind: ExternalSecret
metadata: { name: api-secrets }
spec:
  refreshInterval: 1h
  secretStoreRef: { kind: ClusterSecretStore, name: aws-secrets-manager }
  target: { name: api-secrets }
  dataFrom:
    - extract: { key: prod/api }
```

- Secrets encrypted at rest (KMS on managed clusters), readable by few
  subjects. Cloud access through workload identity (EKS Pod Identity or IRSA,
  GKE Workload Identity Federation, Azure Workload Identity), never keys.

## 5. Scaling and disruption

```yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata: { name: api }
spec:
  scaleTargetRef: { apiVersion: apps/v1, kind: Deployment, name: api }
  minReplicas: 3
  maxReplicas: 20
  metrics:
    - type: Resource
      resource: { name: cpu, target: { type: Utilization, averageUtilization: 70 } }
---
apiVersion: policy/v1
kind: PodDisruptionBudget
metadata: { name: api }
spec:
  maxUnavailable: 1
  selector: { matchLabels: { app.kubernetes.io/name: api } }
```

- CPU utilisation is measured against the request, so requests must be real;
  60 to 75% is the usual target. Request rate or queue depth through KEDA or
  a metrics adapter; KEDA also scales workers to zero on an empty queue. The
  default 300s scale-down window stops flapping; keep it.
- A PDB with `minAvailable` equal to the replica count blocks every drain and
  upgrade; `maxUnavailable: 1` does not. Nodes follow pending pods (Cluster
  Autoscaler or Karpenter); a `PriorityClass` protects what matters.

## 6. Packaging: Kustomize or Helm

| | Kustomize | Helm |
|---|---|---|
| Best for | Your own apps: plain YAML, a base and overlays | Configurable software for others; third-party components |
| Per environment | `overlays/staging`, `overlays/prod` (image digest, replicas, config) | `values-staging.yaml`, `values-prod.yaml` |
| Render | `kubectl kustomize deploy/overlays/prod` | `helm template api ./chart -f values-prod.yaml` |
| Lint | kubeconform on the output | `helm lint --strict`, kubeconform on the output |

- Helm 4 is current (server-side apply for new releases). Pin chart versions,
  prefer OCI charts and the upstream project's chart (Bitnami's point at
  images without free updates); label with the `app.kubernetes.io/*` set.

## 7. Namespaces, RBAC, network policies

- A namespace per application or team and environment; production in its own
  cluster when the blast radius matters. RBAC: a ServiceAccount per workload,
  Roles per namespace, no `cluster-admin` for apps or CI; check with
  `kubectl auth can-i --list --as=system:serviceaccount:shop:api -n shop`.
- Pod Security Admission: label namespaces
  `pod-security.kubernetes.io/enforce: restricted`; the pod above passes.
- Network policies: deny by default, allow DNS, then each needed path (from
  the gateway's namespace in, to the database out). They need an enforcing
  CNI (Cilium, Calico, GKE Dataplane V2, the EKS VPC CNI with policies on);
  without one they are silently ignored.

```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata: { name: default-deny }
spec:
  podSelector: {}
  policyTypes: [Ingress, Egress]
  egress:
    - to: [{ namespaceSelector: {}, podSelector: { matchLabels: { k8s-app: kube-dns } } }]
      ports: [{ port: 53, protocol: UDP }, { port: 53, protocol: TCP }]
```

## 8. Jobs, CronJobs, migrations

- CronJobs: `concurrencyPolicy: Forbid`, an explicit `timeZone`,
  `startingDeadlineSeconds`, and `backoffLimit`, `activeDeadlineSeconds` and
  `ttlSecondsAfterFinished` on the job, so stuck runs end and old ones go.
- Migrations run as a Job before the rollout: an Argo CD `PreSync` hook, a
  Helm `pre-upgrade` hook, or a CI step that waits
  (`kubectl wait --for=condition=complete job/migrate --timeout=10m`). Never
  an init container: it runs in every pod, racing (`devops-deploy`).

## 9. GitOps

```yaml
apiVersion: argoproj.io/v1alpha1
kind: Application
metadata: { name: api-prod, namespace: argocd }
spec:
  project: shop
  source:
    repoURL: https://github.com/acme/deploy.git
    targetRevision: main
    path: apps/api/overlays/prod
  destination: { server: https://kubernetes.default.svc, namespace: shop }
  syncPolicy:
    automated: { prune: true, selfHeal: true }
```

- The cluster pulls its state from git (Argo CD, or Flux `GitRepository` and
  `Kustomization`). CI pushes the image and commits its digest; CI holds no
  cluster credentials, a rollback is a `git revert`, and self-heal undoes
  hand edits. Logs to stdout as JSON, metrics by ServiceMonitor or OTLP,
  alerts on restarts and `OOMKilled` (`devops-observability`).

## Check it

- Offline, with your cluster's version (the second schema location covers
  CRDs such as HTTPRoute and ExternalSecret), plus `helm lint --strict` and
  `kube-linter lint deploy/` or `trivy config deploy/`:

```sh
kubectl kustomize deploy/overlays/prod | kubeconform -strict -summary \
  -kubernetes-version 1.35.0 -schema-location default \
  -schema-location 'https://raw.githubusercontent.com/datreeio/CRDs-catalog/main/{{.Group}}/{{.ResourceKind}}_{{.ResourceAPIVersion}}.json'
```

- With read access: `kubectl diff -k deploy/overlays/prod` and
  `kubectl apply --dry-run=server -k deploy/overlays/prod`; a real apply only
  when the task says so.
- In staging: `kubectl rollout status deployment/api --timeout=5m`, pods
  over zones, no probe failures in `kubectl get events --sort-by=.lastTimestamp`.

## Avoid

`latest` or tag-only images; no requests, no memory limit, or tight CPU
limits on latency-sensitive services; liveness probes that check the
database; no `preStop` sleep; root users and writable filesystems; Secrets in
git as base64; one `LoadBalancer` per service; new work on Ingress NGINX;
`replicas` fighting an HPA; a PDB that blocks every drain; migrations in init
containers; `cluster-admin` for CI; network policies on a CNI that ignores
them; `kubectl apply` to production from a laptop; Kubernetes for one small
service.
