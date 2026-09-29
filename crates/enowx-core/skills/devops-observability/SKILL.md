---
name: devops-observability
description: "Seeing what production does: structured logs with request ids, metrics for rates, errors and durations, traces with OpenTelemetry, dashboards per service, SLOs and error budgets, alerts people act on, uptime checks, error tracking, retention and the cost of telemetry, and runbooks. Read before adding logging, metrics, tracing or alerts."
---

# Seeing production

The generated version logs whole request bodies with `console.log`, keeps them
on one server until the disk fills, counts nothing, has a dashboard of CPU
graphs nobody opens, and pages someone at 3 a.m. for "CPU over 80%" while users
get errors nobody sees. This is the platform side: shipping logs, metrics and
traces somewhere searchable, dashboards that answer "is it broken, and where",
SLOs, a few alerts that each need a human, error tracking, and what it costs.
The app side (the logger, request ids, health endpoints) is in
`backend-observability`. One part of devops; the whole is in the `devops`
skill.

## 1. The pipeline

- Apps write JSON logs to stdout and send traces and metrics over OTLP. A
  collector (the OpenTelemetry Collector, or Grafana Alloy, which embeds it)
  receives, filters, batches and forwards. Backends store: logs in Loki,
  OpenSearch, a ClickHouse-based stack or CloudWatch; metrics in Prometheus,
  Mimir or VictoriaMetrics; traces in Tempo or Jaeger; or one vendor for all
  (Grafana Cloud, Datadog, Honeycomb, Better Stack, Axiom, SigNoz).
- OpenTelemetry keeps the backend swappable: instrument once, change the
  exporter. Promtail reached end of life in March 2026 (Grafana Agent before
  it); ship with Alloy or the Collector.
- A small service can start with the platform's log viewer, an error tracker
  and an outside uptime check; add the rest as it starts to matter.

```yaml
# otel-collector config
receivers:
  otlp:
    protocols:
      grpc: { endpoint: 0.0.0.0:4317 }
      http: { endpoint: 0.0.0.0:4318 }
processors:
  memory_limiter: { check_interval: 1s, limit_percentage: 80, spike_limit_percentage: 20 }
exporters:
  otlp_grpc/tempo:
    endpoint: tempo:4317
    tls: { insecure: true }
    sending_queue: { batch: {} }
  otlp_http/loki:
    endpoint: http://loki:3100/otlp
    sending_queue: { batch: {} }
  otlp_http/prometheus:
    endpoint: http://prometheus:9090/api/v1/otlp
    sending_queue: { batch: {} }
service:
  pipelines:
    traces: { receivers: [otlp], processors: [memory_limiter], exporters: [otlp_grpc/tempo] }
    logs: { receivers: [otlp], processors: [memory_limiter], exporters: [otlp_http/loki] }
    metrics: { receivers: [otlp], processors: [memory_limiter], exporters: [otlp_http/prometheus] }
```

- Since Collector 0.144 the exporters are `otlp_grpc` and `otlp_http` (the old
  `otlp` and `otlphttp` names are deprecated aliases), and batching lives in
  each exporter's `sending_queue`; add the `file_storage` extension to keep
  the queue across restarts. `memory_limiter` comes first.
- Prometheus accepts OTLP only with `--web.enable-otlp-receiver`; Loki takes
  it at `/otlp`. Inside a cluster, keep 4317 and 4318 off the internet.
- Stdout logs reach this pipeline through an agent on each node (Alloy, Fluent
  Bit, Vector, or the contrib collector's `filelog` receiver) or through the
  logger's OpenTelemetry bridge over OTLP.

## 2. Logs

- JSON, one event per line: `time` (RFC 3339, UTC), `level`, `msg`,
  `service`, `env`, `version`, `trace_id`, `span_id`, `request_id`, then the
  event's own fields (`order_id`, `duration_ms`, `status`).
- `trace_id` on every line, through the logger's OpenTelemetry integration
  (pino, structlog, slog, Logback), so a log line opens its trace.
- `error` means someone should look, `warn` is unusual but handled, `info`
  tells the story, `debug` is off in production and switchable at runtime.
- Never logged: passwords, tokens, cookies, `Authorization` headers, card
  numbers, whole request or response bodies, personal data beyond an id.
  Redact in the app; a collector `transform` or `redaction` processor is the
  second net.
- Drop or sample the noise at the collector (health checks, successful static
  requests); never sample errors.
- Index labels stay low-cardinality (service, env, level): Loki indexes only
  labels, and a user id as a label multiplies its index. Ids go in the body
  or structured metadata.
- Retention: 7 to 30 days searchable; 90 days to a year in cheap object
  storage only where audit or law requires it.

## 3. Metrics

- RED for every service: rate, errors, duration (as a histogram). USE for
  every resource: utilisation, saturation (queue length, pool waits, CPU
  throttling), errors. Plus the few business numbers that show breakage:
  sign-ups, orders, failed payments, jobs processed, the age of the oldest
  queued message.
- Names from the OpenTelemetry conventions: `http.server.request.duration`
  (seconds, histogram) with `http.request.method`, `http.route` and
  `http.response.status_code`; Prometheus shows it as
  `http_server_request_duration_seconds`. Base units (seconds, bytes),
  `_total` on counters.
- Every label combination is a series. The route template (`/orders/{id}`),
  never the raw path; never user ids, emails, request ids or error messages
  as labels.
- Histograms, not averages, with buckets around the SLO threshold (0.1,
  0.25, 0.5, 1, 2.5 seconds).
- Infrastructure exporters where the platform gives none: `node_exporter`,
  cAdvisor, `postgres_exporter`, `redis_exporter`, `blackbox_exporter`.

```promql
# requests per second, by route
sum by (http_route) (rate(http_server_request_duration_seconds_count[5m]))
# share of requests that failed
sum(rate(http_server_request_duration_seconds_count{http_response_status_code=~"5.."}[5m]))
  / sum(rate(http_server_request_duration_seconds_count[5m]))
# p95 latency
histogram_quantile(0.95, sum by (le) (rate(http_server_request_duration_seconds_bucket[5m])))
```

## 4. Traces

- OpenTelemetry SDKs with auto-instrumentation for HTTP, database drivers and
  queues. Node: `@opentelemetry/sdk-node` with
  `@opentelemetry/auto-instrumentations-node`, loaded first
  (`node --import ./instrumentation.mjs dist/server.js`). Python:
  `opentelemetry-distro`, `opentelemetry-bootstrap -a install`, run under
  `opentelemetry-instrument`. Java: the OpenTelemetry Java agent. Go:
  instrumentation packages (`otelhttp`, `otelgrpc`) wired in code.
- Configured by the standard variables, the same in every language:

```sh
OTEL_SERVICE_NAME=checkout
OTEL_RESOURCE_ATTRIBUTES=service.version=1.4.2,deployment.environment.name=production
OTEL_EXPORTER_OTLP_ENDPOINT=http://otel-collector:4318
OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf
OTEL_TRACES_SAMPLER=parentbased_traceidratio
OTEL_TRACES_SAMPLER_ARG=0.1
```

- Context crosses services in the W3C `traceparent` header; proxies and CDNs
  pass it through. Across a queue, the producer puts the context in the
  message headers and the consumer continues from it.
- Sampling: head sampling (above, parent-based so a trace is kept or dropped
  whole) is cheap. Tail sampling in the collector (`tail_sampling`) keeps
  every error and slow trace plus a share of the rest, but needs all of a
  trace's spans at one collector (a load-balancing exporter keyed by trace
  id).
- Span names stay low-cardinality (`GET /orders/{id}`); attributes carry ids,
  never secrets or personal data.

## 5. Dashboards

- One per service. The top row is the golden signals (traffic, errors, p50,
  p95 and p99 latency, saturation); then its dependencies (database latency,
  pool use, queue depth); then resources.
- Deploy markers as annotations from the deploy job: most regressions line up
  with one. Links from each panel to that service's logs and traces for the
  same window (data links, exemplars on latency).
- Dashboards live in git (provisioned JSON, or the Grafana Terraform
  provider), and an overview shows every service's SLO status.

## 6. SLOs and error budgets

- The indicator is what users feel: the share of good requests (not 5xx, and
  under the latency threshold) measured at the edge or load balancer; for
  jobs, the share done within the promised time.
- One objective per user journey over 30 days. 99.9% suits most APIs; each
  extra nine costs a lot more.

| Objective (30 days) | Error budget |
|---|---|
| 99% | 7 h 12 min |
| 99.5% | 3 h 36 min |
| 99.9% | 43 min 12 s |
| 99.95% | 21 min 36 s |
| 99.99% | 4 min 19 s |

- Agree the budget policy in advance: while budget remains, ship; once it is
  spent, reliability work comes before features.
- Alert on burn rate over two windows: page when 2% of the month's budget
  goes in an hour (rate 14.4, confirmed over 5 minutes) or 5% in 6 hours
  (rate 6, over 30 minutes); open a ticket when 10% goes in 3 days (rate 1,
  over 6 hours). Sloth or Pyrra generate the full rule set.

```yaml
groups:
  - name: checkout-slo
    rules:
      - alert: CheckoutErrorBudgetFastBurn
        expr: |
          (sum(rate(http_server_request_duration_seconds_count{job="checkout",http_response_status_code=~"5.."}[1h]))
            / sum(rate(http_server_request_duration_seconds_count{job="checkout"}[1h]))) > (14.4 * 0.001)
          and
          (sum(rate(http_server_request_duration_seconds_count{job="checkout",http_response_status_code=~"5.."}[5m]))
            / sum(rate(http_server_request_duration_seconds_count{job="checkout"}[5m]))) > (14.4 * 0.001)
        for: 2m
        labels: { severity: page }
        annotations:
          summary: Checkout is spending its 30-day error budget 14 times too fast
          runbook_url: https://github.com/acme/runbooks/blob/main/checkout/errors.md
```

## 7. Alerts

- Alert on symptoms users feel (errors, latency, burn rate, the site
  unreachable), not on causes (CPU at 80%, one pod restarted); causes belong
  on dashboards and in tickets.
- Two severities: page (a person acts now) and ticket (next working day).
  Anything else is not an alert.
- Every page says what is wrong in one line, links a runbook and a
  dashboard, and was fired once in staging to prove it arrives. `for:` of 2
  to 5 minutes rides out blips; Alertmanager grouping and inhibition stop one
  dead database from sending thirty pages.
- More than a couple of pages per on-call shift means the alerts need work:
  review monthly, and tune or delete every page that needed no action.
- Always present: an uptime check from outside, every minute, from several
  regions, alerting after 2 to 3 failures; certificate expiry (warn at 14
  days, page at 7; Let's Encrypt stopped emailing expiry warnings in June
  2025); disk filling (`predict_linear`); backup and cron heartbeats (a dead
  man's switch such as Healthchecks.io or an Uptime Kuma push monitor); and
  an always-firing watchdog alert that proves the pipeline delivers.

## 8. Error tracking

- Sentry or similar (GlitchTip is API-compatible and self-hostable) in the
  backend and the frontend, with `environment` and `release` set (the git
  SHA, as deployed).
- Source maps uploaded at build (`sentry-cli sourcemaps inject` then
  `upload`, or the bundler plugin) and not served publicly.
- `sendDefaultPii: false`, a `beforeSend` that scrubs, users identified by id
  only.
- Grouping tuned so one bug is one issue; alerts on new issues in a release,
  not on every event. A low `tracesSampleRate` (0.05 to 0.2) when
  OpenTelemetry already traces.

## 9. Runbooks and cost

- A runbook per page, one screen long: what the alert means, the first
  checks (its dashboard, recent deploys, dependencies' status pages), safe
  actions (roll back, scale, fail over, turn off a flag), who to call next,
  and when it last fired.
- Telemetry cost is volume times retention times cardinality. The usual
  waste: debug logs in production, health check logs, high-cardinality
  labels, traces kept at 100%, the same logs shipped twice. Cut it with
  collector filters and sampling, retention tiers and recording rules, and
  compare the bill with the infrastructure it watches each month.

## Check it

- Send one request with a known id: its log line, its trace (opened from the
  log) and its count on the dashboard are all findable.
- `otelcol-contrib validate --config=collector.yaml`, `promtool check rules
  alerts.yml`, `promtool test rules alerts_test.yml`,
  `amtool check-config alertmanager.yml`.
- Fire each new alert once in staging and confirm it reaches the right person
  with a working runbook link.
- Grep a sample of real output for `password`, `authorization`, `bearer`,
  `cookie` and `token`: nothing sensitive.
- Cardinality: `topk(10, count by (__name__) ({__name__=~".+"}))`; no series
  count growing with users or requests.

## Avoid

`console.log` as a pipeline; bodies, tokens or personal data in logs; logs
that exist only on one server; user ids or raw paths as labels; averages
instead of histograms; dashboards nobody opens; alerts on causes, alerts with
no runbook, alerts never tested; paging for what can wait until morning; no
outside uptime check or certificate expiry alert; source maps served to the
public; 100% trace sampling and debug logs in production by default; a
telemetry bill nobody reads.
