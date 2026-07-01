---
name: loom-prometheus
description: Prometheus monitoring and alerting for cloud-native observability. Use for writing PromQL queries, configuring scrape targets, creating alerting and recording rules, instrumenting applications, and setting up service discovery. Not for dashboards (use loom-grafana) or log analysis (use loom-logging-observability).
allowed-tools:
  - Read
  - Grep
  - Glob
  - Edit
  - Write
  - Bash
triggers:
  - metrics
  - prometheus
  - promql
  - counter
  - gauge
  - histogram
  - summary
  - alert
  - alertmanager
  - alerting rule
  - recording rule
  - scrape
  - target
  - label
  - service discovery
  - relabeling
  - exporter
  - instrumentation
  - slo
  - error budget
---

# Prometheus Monitoring and Alerting

Pull-based, multi-dimensional time-series monitoring with PromQL. Scope: PromQL, scrape/SD config, recording & alerting rules, instrumentation, Alertmanager. **Dashboards/panels → loom-grafana; log queries → loom-logging-observability.**

## Overview

| Component | Role |
| --------- | ---- |
| Prometheus server | Scrapes targets, stores TSDB locally, evaluates rules |
| Alertmanager | Dedup, group, route, inhibit, silence, notify |
| Exporters | Translate third-party systems to the exposition format (node, blackbox, ...) |
| Client libraries | In-process instrumentation (Go, Python, Rust, Java, ...) |
| Pushgateway | Ephemeral batch jobs push metrics — see gotcha; avoid for services |
| Operator / Thanos·Mimir·Cortex | K8s CRD deploy / long-term + global-view remote storage |

**Metric types.** `up` is the synthetic per-target health series (1 = scrape ok).

| Type | Semantics | Query with |
| ---- | --------- | ---------- |
| Counter | Monotonic, resets to 0 on restart (`_total` suffix) | `rate()`/`increase()` — **never graph raw** |
| Gauge | Up/down (memory, queue depth) | raw, `avg_over_time`, `deriv`, `predict_linear` |
| Histogram | Bucketed observations → `_bucket{le}`,`_sum`,`_count` | `histogram_quantile()`; buckets are additive |
| Summary | Client-side quantiles → `{quantile}`,`_sum`,`_count` | **not aggregatable** — single-process only |

## Scrape Configuration

```yaml
# prometheus.yml
global:
  scrape_interval: 15s          # rate() windows must be >= ~4x this
  evaluation_interval: 15s
  external_labels: { cluster: production, region: us-east-1 }

alerting:
  alertmanagers:
    - static_configs: [{ targets: [alertmanager:9093] }]

rule_files: ["rules/*.yml", "alerts/*.yml"]

scrape_configs:
  - job_name: prometheus
    static_configs: [{ targets: [localhost:9090] }]

  - job_name: application
    metrics_path: /metrics
    static_configs:
      - targets: [app-1:8080, app-2:8080]
        labels: { env: production, team: backend }
  # Kubernetes SD → see Service Discovery section (single canonical example)
```

## Alertmanager

```yaml
# alertmanager.yml
global:
  resolve_timeout: 5m
route:
  group_by: [alertname, cluster, service]
  group_wait: 10s          # buffer to batch related alerts in first notification
  group_interval: 10s      # wait before adding new alerts to an existing group
  repeat_interval: 12h     # re-send an unresolved alert
  receiver: default
  routes:
    # matchers/source_matchers/target_matchers (0.27+, UTF-8 aware) REPLACE the
    # deprecated match/match_re/source_match/target_match (removal pending).
    - matchers: [severity = critical]
      receiver: pagerduty
      continue: true               # also fall through to default
    - matchers: [team = database]
      receiver: dba-team
      group_by: [alertname, instance]

inhibit_rules:
  - source_matchers: [severity = critical]   # a firing critical suppresses...
    target_matchers: [severity = warning]    # ...matching warnings
    equal: [alertname, instance]

receivers:
  - name: default
    slack_configs:
      - channel: "#alerts"
        text: "{{ range .Alerts }}{{ .Annotations.description }}{{ end }}"
  - name: pagerduty
    pagerduty_configs:
      # routing_key = Events API v2 (current, richer). service_key = legacy v1;
      # mutually exclusive. *_file variants read the secret from a mounted file.
      - routing_key_file: /etc/alertmanager/secrets/pagerduty-routing-key
  - name: dba-team
    email_configs: [{ to: dba-team@example.com }]
```

**Time-based routing** — attach `active_time_intervals` (or `mute_time_intervals`) to a route referencing a named `time_intervals` block:

```yaml
route:
  routes:
    - matchers: [severity = warning]
      receiver: email
      active_time_intervals: [business-hours]
time_intervals:
  - name: business-hours
    time_intervals:
      - times: [{ start_time: "09:00", end_time: "17:00" }]
        weekdays: ["monday:friday"]
```

## Metric Naming & Cardinality

Format `<namespace>_<subsystem>_<name>_<unit>`. Base units only: **seconds** (not ms), **bytes** (not KB), **ratio 0.0–1.0** (not 0–100). Counters end `_total`.

| Pattern | Good | Bad |
| ------- | ---- | --- |
| Counter suffix | `http_requests_total` | `http_requests` |
| Base unit | `..._duration_seconds` | `..._duration_ms` |
| Ratio range | `cache_hit_ratio` (0–1) | `cache_hit_percentage` (0–100) |
| Namespace prefix | `myapp_http_requests_total` | `http_requests_total` |
| snake_case labels | `{method="GET"}` | `{httpMethod="GET"}` |

**Cardinality is the #1 Prometheus killer** — series count = product of label-value counts. Never label with unbounded values.

| Cardinality | Examples | Verdict |
| ----------- | -------- | ------- |
| Low (<10) | method, status class, env | safe anywhere |
| Medium (10–100) | endpoint (templated), service, pod | safe with aggregation |
| High (100–1k) | container id, hostname | only if necessary |
| Unbounded | user id, IP, timestamp, raw URL/path with ids | **never** |

Template path labels (`/api/users/:id`, not `/api/users/12345`). Pre-aggregate with recording rules; drop noisy metrics via `metric_relabel_configs`/`write_relabel_configs`.

## Recording Rules

Pre-compute expensive/reused queries. **Prefer `without (instance)` over `by (job)`** — `without` names only the label removed and preserves `job` + any future labels; `by` silently drops labels added later (the docs mandate `without`). Keep `le` where `histogram_quantile()` consumes the rule. Naming: `level:metric:operations`.

```yaml
# rules/recording_rules.yml
groups:
  - name: performance_rules
    interval: 30s
    rules:
      - record: instance_removed:http_requests:rate5m
        expr: sum without (instance) (rate(http_requests_total[5m]))

      # Error ratio: aggregate numerator + denominator SEPARATELY, then divide.
      # NEVER avg()/sum() a ratio rule downstream — re-aggregate the counts.
      - record: job:http_request_error_ratio:rate5m
        expr: |
          sum without (instance) (rate(http_requests_total{status=~"5.."}[5m]))
          / sum without (instance) (rate(http_requests_total[5m]))

      - record: job:http_request_duration_seconds:p95   # le kept for histogram_quantile
        expr: histogram_quantile(0.95, sum without (instance) (rate(http_request_duration_seconds_bucket[5m])))

  - name: aggregation_rules
    interval: 1m
    rules:
      - record: instance:node_cpu_utilization:ratio
        expr: 1 - avg without (cpu, mode) (rate(node_cpu_seconds_total{mode="idle"}[5m]))
      - record: cluster:node_cpu_utilization:ratio
        expr: avg without (instance) (instance:node_cpu_utilization:ratio)
```

## Alerting Rules

**Page on user-facing symptoms, not component causes** (Golden Signals: latency, traffic, errors, saturation). Page at the outermost user-visible boundary — one layer's symptom is another's cause. Vet every page: urgent, user-visible, actionable, non-automatable, not already paged? If not → warning/ticket.

```yaml
# alerts/symptom_based.yml
groups:
  - name: symptom_alerts
    rules:
      - alert: HighErrorRate
        expr: |
          sum(rate(http_requests_total{status=~"5.."}[5m]))
          / sum(rate(http_requests_total[5m])) > 0.05
        for: 5m                 # condition must hold before firing (anti-flap)
        keep_firing_for: 10m    # stay firing after it clears (anti-flap on exit; 2.42+)
        labels: { severity: critical, team: backend }
        annotations:
          summary: "High error rate"
          description: "Error rate {{ $value | humanizePercentage }} (>5%)"
          runbook: https://wiki.example.com/runbooks/high-error-rate

      - alert: HighLatency
        expr: |
          histogram_quantile(0.95,
            sum(rate(http_request_duration_seconds_bucket[5m])) by (le, service)) > 1
        for: 5m
        labels: { severity: warning, team: backend }
        annotations: { summary: "P95 latency high on {{ $labels.service }}" }

      # Detect a missing series (see absent() gotcha for per-instance / flaky cases)
      - alert: ServiceDown
        expr: up{job="critical-service"} == 0
        for: 2m
        labels: { severity: critical }
```

### Multi-window multi-burn-rate SLO (Google SRE Workbook)

**No `for:` clause** — duration doesn't scale with severity and resets on data gaps. Each tier ANDs a long detection window with a short confirmation window (~1/12 of the long one, proving the budget is burning *now*); tiers are ORed. For a 99.9% SLO (budget 0.001):

```yaml
- alert: SLOBudgetBurnFast     # page: ~2% monthly budget in 1h
  expr: |
    sum(rate(http_requests_total{status=~"5.."}[1h])) / sum(rate(http_requests_total[1h])) > 14.4 * 0.001
    and
    sum(rate(http_requests_total{status=~"5.."}[5m])) / sum(rate(http_requests_total[5m])) > 14.4 * 0.001
  labels: { severity: critical, team: sre }
- alert: SLOBudgetBurnSlow     # ticket: gradual burn
  expr: |
    sum(rate(http_requests_total{status=~"5.."}[6h])) / sum(rate(http_requests_total[6h])) > 6 * 0.001
    and
    sum(rate(http_requests_total{status=~"5.."}[30m])) / sum(rate(http_requests_total[30m])) > 6 * 0.001
  labels: { severity: warning, team: sre }
# Third tier: 1x over 3d AND 6h (slow ticket).
```

**Alert hygiene:** meaningful `summary`/`description`/`runbook`/`impact` annotations; `team`/`service`/`env` labels for routing; every alert must be actionable. Validate with `promtool check rules`.

## PromQL

Value types: instant vector, range vector, scalar, string.

```promql
# Selectors / matchers
http_requests_total{method="GET", status=~"5.."}    # =, !=, =~, !~
http_requests_total{status!=""}                     # label present
http_requests_total[5m]                             # range vector (window must be >= ~4x scrape)

# Rate — ALWAYS rate() the raw counter BEFORE aggregating
sum(rate(http_requests_total[5m])) by (service)
increase(http_requests_total[1h])   # rate*window; extrapolated float, NOT exact count
irate(http_requests_total[5m])      # last 2 samples; dashboards ONLY, never alerts

# Error / success ratio (aggregate num & denom separately, then divide)
sum(rate(http_requests_total{status=~"5.."}[5m])) / sum(rate(http_requests_total[5m]))

# Histogram percentiles (keep le in by())
histogram_quantile(0.95, sum(rate(http_request_duration_seconds_bucket[5m])) by (le, service))
# Average latency (NOT a percentile): _sum / _count
sum(rate(http_request_duration_seconds_sum[5m])) by (service)
  / sum(rate(http_request_duration_seconds_count[5m])) by (service)

# Aggregation ops: sum avg min max count stddev stdvar quantile group + by()/without()
count(up == 1) by (job)

# Selection / prediction
topk(5, sum(rate(http_requests_total[5m])) by (service))
predict_linear(node_filesystem_avail_bytes{mountpoint="/"}[1h], 4*3600) < 0   # disk-full in 4h
absent(up{job="critical-service"})                                            # see gotcha

# Offsets / time
rate(http_requests_total[5m]) / rate(http_requests_total[5m] offset 1h)
time() - process_start_time_seconds                                           # uptime (s)
```

**Vector matching.** One-to-one by default (identical label sets). Many-to-one needs `on(labels) group_left(extra)` (right side is "one"); `group_right` mirrors it. Logical: `and`, `or`, `unless`.

```promql
sum(rate(http_requests_total[5m])) by (instance, method)
  / on(instance) group_left
sum(rate(http_requests_total[5m])) by (instance)
```

**Apdex** (target T, tolerable 4T):

```promql
( sum(rate(http_request_duration_seconds_bucket{le="0.1"}[5m]))
  + sum(rate(http_request_duration_seconds_bucket{le="0.4"}[5m])) ) / 2
/ sum(rate(http_request_duration_seconds_count[5m]))
```

## Service Discovery

```yaml
scrape_configs:
  # Static + file-based
  - job_name: file-sd
    file_sd_configs:
      - files: ["/etc/prometheus/targets/*.json"]
        refresh_interval: 30s
  # targets/*.json: [{ "targets": ["web1:8080"], "labels": { "job": "web" } }]

  # Kubernetes pods (annotation-driven) — canonical relabel pattern
  - job_name: kubernetes-pods
    kubernetes_sd_configs: [{ role: pod }]      # roles: pod|service|endpoints|node|ingress
    relabel_configs:
      - source_labels: [__meta_kubernetes_pod_annotation_prometheus_io_scrape]
        action: keep
        regex: "true"
      - source_labels: [__meta_kubernetes_pod_annotation_prometheus_io_path]
        action: replace
        target_label: __metrics_path__
        regex: (.+)
      - source_labels: [__address__, __meta_kubernetes_pod_annotation_prometheus_io_port]
        action: replace
        regex: ([^:]+)(?::\d+)?;(\d+)
        replacement: $1:$2
        target_label: __address__
      - action: labelmap
        regex: __meta_kubernetes_pod_label_(.+)
      - source_labels: [__meta_kubernetes_namespace]
        target_label: namespace
      - source_labels: [__meta_kubernetes_pod_name]
        target_label: pod

  - job_name: consul
    consul_sd_configs: [{ server: consul:8500, services: [web, api] }]
  - job_name: ec2
    ec2_sd_configs:
      - region: us-east-1
        port: 9100
        filters: [{ name: "tag:Environment", values: [production] }]
  - job_name: dns-srv
    dns_sd_configs: [{ names: ["_prometheus._tcp.example.com"], type: SRV }]
```

Corresponding pod annotations: `prometheus.io/scrape: "true"`, `prometheus.io/port: "8080"`, `prometheus.io/path: "/metrics"`.

### Relabeling actions

`relabel_configs` runs pre-scrape (targets); `metric_relabel_configs`/`write_relabel_configs` runs post-scrape (samples).

| Action | Effect | Use |
| ------ | ------ | --- |
| `keep`/`drop` | Include/exclude targets whose source labels match regex | filter by annotation |
| `replace` | Write `replacement` (regex-templated) into `target_label` | extract path/port/labels |
| `labelmap` | Copy source label names matching regex to new names | copy all K8s labels |
| `labeldrop`/`labelkeep` | Drop/keep labels by regex | strip metadata / cut cardinality |
| `hashmod` | `target_label = hash(source) % modulus` | shard targets across replicas |

## HA, Scale & Long-Term Storage

- **HA:** run 2+ identical Prometheis with distinct `external_labels.replica`; Alertmanager dedups. Run Alertmanager as a gossip cluster (`--cluster.peer=...`, port 9094).
- **Federation:** a global Prometheus scrapes `/federate` with `match[]` selecting only aggregates (`{__name__=~"job:.*"}`); set `honor_labels: true`. Use sparingly — federate recording-rule aggregates, not raw series.
- **Remote write** to Thanos/Mimir/Cortex for long-term + global query; tune `queue_config`, and drop cardinality with `write_relabel_configs` before shipping.
- **Sharding:** split targets across replicas with `hashmod` on a stable label + `keep` on the shard index.

```yaml
remote_write:
  - url: http://mimir:8080/api/v1/push
    write_relabel_configs:
      - source_labels: [__name__]
        regex: "go_.*"
        action: drop
```

## Prometheus Operator (CRDs)

`ServiceMonitor`/`PodMonitor` (scrape targets), `PrometheusRule` (alerts+recording), `Prometheus` (server). CRD `relabelings` run pre-scrape, `metricRelabelings` post-scrape (drop noise). Selectors on the `Prometheus` CR (`serviceMonitorSelector`, `ruleSelector`, ...) must match the CRDs' labels or they are silently ignored.

```yaml
apiVersion: monitoring.coreos.com/v1
kind: ServiceMonitor
metadata:
  name: app-metrics
  labels: { release: prometheus }        # must match prometheus.spec.serviceMonitorSelector
spec:
  selector: { matchLabels: { app: myapp } }
  namespaceSelector: { matchNames: [production] }
  endpoints:
    - port: metrics                       # Service port NAME (not number)
      interval: 30s
      metricRelabelings:
        - sourceLabels: [__name__]
          regex: "go_.*"
          action: drop
---
apiVersion: monitoring.coreos.com/v1
kind: PrometheusRule
metadata:
  labels: { release: prometheus }
spec:
  groups:
    - name: app_alerts
      rules:
        - alert: PodCrashLooping           # floor over window; single recycles don't page
          expr: increase(kube_pod_container_status_restarts_total[15m]) > 3
          for: 5m
          labels: { severity: warning }
```

⚠️ **Prometheus 3.x migration** (`spec.version`): scrape Content-Type is now strict — set `fallback_scrape_protocol: PrometheusText0.0.4` per job for non-compliant exporters; `le`/`quantile` labels are normalized on ingest (`le="1"` → `le="1.0"`), so expressions matching integer values must use the float form. `retention`, `retentionSize`, `storage.volumeClaimTemplate` are set on the CR.

## Instrumentation

**Use HistogramVec (never SummaryVec) for anything aggregated across instances** — summary quantiles are per-process and non-additive. Set `NativeHistogramBucketFactor` (~1.1) for a native histogram (no upfront bucket guessing) with a classic `Buckets` fallback.

```go
var (
    httpRequests = promauto.NewCounterVec(prometheus.CounterOpts{
        Name: "http_requests_total", Help: "Total HTTP requests",
    }, []string{"method", "endpoint", "status"})

    httpDuration = promauto.NewHistogramVec(prometheus.HistogramOpts{
        Name: "http_request_duration_seconds",
        NativeHistogramBucketFactor: 1.1,
        Buckets: prometheus.DefBuckets,          // classic fallback
    }, []string{"method", "endpoint"})

    active = promauto.NewGauge(prometheus.GaugeOpts{Name: "active_connections"})
)

func instrument(endpoint string, h http.HandlerFunc) http.HandlerFunc {
    return func(w http.ResponseWriter, r *http.Request) {
        start := time.Now(); active.Inc(); defer active.Dec()
        rw := &responseWriter{ResponseWriter: w, statusCode: 200}
        h(rw, r)
        httpDuration.WithLabelValues(r.Method, endpoint).Observe(time.Since(start).Seconds())
        httpRequests.WithLabelValues(r.Method, endpoint, strconv.Itoa(rw.statusCode)).Inc()
    }
}
// main: http.Handle("/metrics", promhttp.Handler())
```

```python
# Flask — prometheus_client
request_count = Counter("http_requests_total", "Total HTTP requests",
                        ["method", "endpoint", "status"])
request_duration = Histogram("http_request_duration_seconds", "Request duration s",
                             ["method", "endpoint"])  # buckets=[...] to override defaults

@app.before_request
def _before(): request.start = time.time()

@app.after_request
def _after(resp):
    ep = request.endpoint or "unknown"
    request_duration.labels(request.method, ep).observe(time.time() - request.start)
    request_count.labels(request.method, ep, resp.status_code).inc()
    return resp

@app.route("/metrics")
def metrics(): return generate_latest()   # multiprocess (gunicorn): MultiProcessCollector
```

⚠️ Label endpoints with the **route template**, never the concrete path — `request.endpoint` (Flask) / router pattern (Go), not `request.path`.

## Expert Practices: Idioms, Anti-Patterns & Gotchas

Mechanism-level rules from the docs, the Google SRE Book/Workbook, and practitioners. Each states the *why*.

### Anti-Patterns (statistically or operationally wrong)

**Always `rate()` before `sum()`, never `sum()` before `rate()`.** Counters reset to 0 on restart. Sum counters first and if one target restarts the aggregate drops; `rate()` reads that drop as a reset and emits a huge spurious spike. Per-series `rate()` handles each reset in isolation. Mathematical, not performance. Applies to all counter arithmetic: `rate(a[5m]) + rate(b[5m])`, never `rate(a[5m] + b[5m])`. Only `rate`, `irate`, `increase`, `resets` are safe on a raw counter.

```promql
sum by (job) (rate(http_requests_total[5m]))        # correct
rate(sum by (job) (http_requests_total)[5m])        # WRONG (and won't parse as written)
```

**Never `irate()` in alerting rules.** It uses only the last two samples (maximally volatile); with `for:`, one brief dip resets the pending timer, so a sustained breach may never fire. Docs: *"Use rate for alerts and slow-moving counters."* Reserve `irate()` for high-res dashboards.

**Never aggregate a ratio.** Averaging/summing pre-computed ratios is invalid (Jensen / average-of-averages): A serves 1000 req @10% err, B serves 10 @90% → `avg(10%,90%)=50%` but true combined ≈18%. Aggregate numerator and denominator separately, then divide — including down recording-rule chains.

**Summaries can't be aggregated across instances — use histograms for cross-instance percentiles.** Summary quantiles are per-process and non-additive; `avg(...{quantile="0.95"})` across pods is "statistically nonsensical" (docs). Histogram buckets are additive: sum buckets across any dimension, then `histogram_quantile()`.

```promql
histogram_quantile(0.95, sum by (le) (rate(http_request_duration_seconds_bucket[5m])))  # correct
avg(http_request_duration_seconds{quantile="0.95"})                                     # WRONG
```

**Pushgateway never expires metrics and has no `up` signal.** Docs: *"The Pushgateway never forgets series pushed to it."* A job that succeeded Monday and never ran Tuesday still serves success. Push a `*_last_success_timestamp_seconds` gauge and alert on staleness (`time() - ... > 3600`), and/or DELETE the group on completion. Only for genuinely ephemeral batch jobs.

### Design & Idioms

**Multi-window multi-burn-rate SLO alerts, no `for:`** (see Alerting section) — duration doesn't scale with severity and resets on gaps; each tier ANDs long+short (~1/12) windows.

**Prefer `without` over `by`.** `by (l1,l2)` drops any label added later; `without (instance)` future-proofs and preserves `job` + routing labels. Docs mandate `without`.

**Prefer native histograms for new instrumentation (stable in 3.x).** Dynamic exponential buckets (no upfront guessing), one composite series (vs N+2 classic `_bucket` series), cross-instance aggregatable. The `--enable-feature=native-histograms` flag is a no-op as of v3.9 — enable via scrape config, not the flag; convert classic on ingest without re-instrumenting.

```yaml
scrape_configs:
  - job_name: myapp
    scrape_native_histograms: true
    convert_classic_histograms_to_nhcb: true
    static_configs: [{ targets: [myapp:8080] }]
```

**`keep_firing_for` (2.42+) adds resolution hysteresis.** `for:` delays *firing*; `keep_firing_for:` delays *resolution*, keeping the alert firing N after the condition last held — stops firing/resolved/firing flapping on a value oscillating around the threshold, without weakening the threshold.

### Gotchas (silent failures — no error, wrong/empty data)

**Keep `le` when aggregating classic histograms.** `histogram_quantile()` reconstructs from the `le` label; if `sum()` drops `le` (not in `by()`, or labeldrop'd), buckets collapse → NaN/garbage, silently. Also: highest bucket must be `le="+Inf"`; a window with zero observations divides by zero; only aggregate instances with identical bucket boundaries.

**`rate()` window must be ≥ ~4× scrape interval.** `rate()` needs ≥2 samples; at 15s scrape, `[15s]`/`[20s]` holds ~2 under ideal timing, so jitter/one miss → empty results and gappy graphs. Rule: `window >= 4 * scrape_interval` (1m min at 15s). Grafana's `$__rate_interval` encodes this.

**`increase()` extrapolates and returns fractionals.** `increase(v[d]) = rate(v[d])*d`, extrapolated to window edges, so `increase(errors_total[1h])` may read 99.7 even for integer counters — integer thresholds (`>100`) fire/miss off-by-one. Visualization only, never exact counting/billing.

**`absent()` only fires on TOTAL absence.** Returns 1 only when the selector matches zero series — can't detect one instance stopping while others export, and can't be stabilized with `for:` (timer resets when the series reappears). For per-target: join `up`; for flaky scrapes: `absent_over_time()`.

```promql
up{job="foo"} == 1 unless on(instance) my_metric{job="foo"}   # per-instance missing metric
absent_over_time(up{job="critical"}[5m])                      # transient-resilient total absence
```

**Bare `sum()` drops all labels and breaks Alertmanager routing.** `sum()` with no `by()`/`without()` yields one label-less series, so alerts carry no `job`/`service`/`team`/`env` and routing/grouping matching them silently fails. Use `sum without (instance) (...)`.

**Metrics with explicit timestamps bypass staleness markers.** Prometheus normally inserts staleness markers when a series stops — disabled for exposition that embeds its own timestamps (cAdvisor, some OTel collectors). A vanished series keeps its last value for up to `query.lookback-delta` (5m default; scrape interval must be well under it) and looks live to alerts. Enable `track_timestamps_staleness: true` per scrape config.

### Security & Currency

**`honor_labels: true` is a privileged write endpoint.** It lets the target's own `job`/`instance` override server-assigned ones, so any client that can POST (Pushgateway, untrusted federation) can impersonate any service and inject arbitrary series. Restrict network access.

**Prometheus 3.0 breaking changes.** (1) Strict Content-Type — set `fallback_scrape_protocol: PrometheusText0.0.4` for non-compliant exporters. (2) `le`/`quantile` normalized on ingest (`le="1"` → `le="1.0"`) — match the float form. Read the migration guide.

## Verification Checklist

- [ ] Every counter query wraps the raw counter in `rate()`/`increase()` BEFORE any `sum()`/`avg()`
- [ ] No `irate()` in alert rules; `rate()` windows are ≥ ~4× scrape interval
- [ ] Ratios aggregate numerator & denominator separately, then divide (never `avg`/`sum` a ratio)
- [ ] `histogram_quantile()` keeps `le` in every feeding `by()`; cross-instance metrics are histograms, not summaries
- [ ] Alerts page on user-facing symptoms; SLO alerts use multi-window burn-rate with no `for:`
- [ ] Aggregations keep routing labels (`sum without (instance)`, not bare `sum()`); recording rules use `without`
- [ ] No unbounded-cardinality labels (ids, IPs, raw paths, timestamps)
- [ ] `promtool check config` and `promtool check rules` pass
- [ ] CR/Operator selector labels match the ServiceMonitor/PrometheusRule labels
- [ ] Critical alerts carry runbook + team/service/env labels

```bash
promtool check config prometheus.yml
promtool check rules alerts/*.yml
promtool query instant http://localhost:9090 'up'
curl -s http://localhost:9090/api/v1/targets | jq '.data.activeTargets[] | {job:.labels.job, health}'
curl -s http://localhost:9090/api/v1/status/tsdb        # top series / cardinality offenders
```

## Resources

- Prometheus docs: <https://prometheus.io/docs/> · PromQL: /prometheus/latest/querying/basics/
- Best practices: <https://prometheus.io/docs/practices/>
- Prometheus Operator: <https://github.com/prometheus-operator/prometheus-operator>
- Google SRE Book/Workbook — Monitoring & Alerting on SLOs
