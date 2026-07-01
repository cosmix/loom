---
name: loom-grafana
description: Observability visualization with Grafana and the LGTM stack. Use for creating dashboards, configuring panels, writing LogQL/TraceQL queries, setting up data sources, dashboard variables/templates, and Grafana alerts. Do not use for PromQL (see loom-prometheus).
when_to_use: Grafana dashboards, panel configuration, LogQL or TraceQL queries, Loki/Tempo/Mimir data sources, dashboard templating.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Edit
  - Write
  - Bash
triggers:
  - grafana
  - dashboard
  - panel
  - visualization
  - logql
  - traceql
  - loki
  - tempo
  - mimir
  - data source
  - annotation
  - variable
  - template
  - row
  - stat
  - graph
  - table
  - heatmap
  - gauge
  - bar chart
  - pie chart
  - time series
  - logs panel
  - traces panel
  - LGTM stack
---

# Grafana and the LGTM Stack

Visualization, dashboards, alerting, and exploration. Scope: dashboards/panels, LogQL, TraceQL, data sources, templating, Grafana alerting. **PromQL query semantics → loom-prometheus.**

## Overview

| Component | Role | Query lang |
| --------- | ---- | ---------- |
| **L**oki | Log aggregation; indexes labels only, not content | LogQL |
| **G**rafana | Visualization, dashboards, unified alerting, Explore | — |
| **T**empo | Distributed tracing; columnar Parquet, OTel-native | TraceQL |
| **M**imir | Long-term, multi-tenant Prometheus storage (`remote_write`) | PromQL |

Collector: **Grafana Alloy** (OTel Collector distribution). Grafana Agent is EOL (2025-11-01) — migrate.

## Dashboard Design

**Hierarchy:** Overview → Service → Component → Deep Dive. **Signals:** RED (Rate/Errors/Duration) for services, USE (Utilization/Saturation/Errors) for resources. Variable-driven, 24-column grid, `< 15` panels/dashboard.

### Panel types

| Panel | Best for |
| ----- | -------- |
| Time Series | trends: rates, latency, resource usage (default; replaces removed Graph) |
| Stat | single current value / sparkline |
| Gauge / Bar Gauge | progress toward a limit / Top-N comparison |
| Table | structured rows: service lists, error details |
| Heatmap | distribution over time (latency buckets); renders native histograms |
| Pie | proportions (use sparingly) |
| Logs / Traces | Loki streams / Tempo trace view |

### Panel config

- **Titles** include the unit + dimension ("P95 Latency (s) by Endpoint", not "Latency"); add a description tooltip.
- **Units:** always set `fieldConfig.defaults.unit` (`s`, `bytes`, `reqps`, `percentunit` for 0–1 ratios, `percent` for 0–100). Wrong unit is the most common panel bug.
- **Legends:** `{{label}}` templating; hide for single series; sort by value for Top-N.
- **Thresholds:** green (normal) / yellow (warning) / red (critical); drive Stat/Gauge color and Time Series bands.
- **Data links:** drill Overview→Service→Component; link to logs/traces/runbooks.

## Variables & Templating

| Type | Purpose |
| ---- | ------- |
| Query | populate from a data source (`label_values(...)`) |
| Custom | static option list (envs) |
| Interval | selectable resolution step |
| Datasource | switch data sources |
| Constant / Textbox | hidden value / free-form input |

```json
{ "templating": { "list": [
  { "name": "datasource", "type": "datasource", "query": "prometheus" },
  { "name": "namespace", "type": "query", "datasource": "${datasource}",
    "query": "label_values(kube_pod_info, namespace)", "multi": true, "includeAll": true },
  { "name": "app", "type": "query", "datasource": "${datasource}",
    "query": "label_values(kube_pod_info{namespace=~\"$namespace\"}, app)",
    "multi": true, "includeAll": true },
  { "name": "interval", "type": "interval", "auto": true,
    "options": ["1m","5m","15m","1h","6h","1d"] }
] } }
```

Reference with `$var` / `${var}`. **Dependent chain:** `$datasource → $cluster → $namespace → $app → $pod`, each query filtering on the previous. Regex-filter a variable's values with `"regex": "/^$app-.*/"`.

> **⚠️ Multi-value / Include-All variables interpolate as a regex alternation — use `=~`, never `=`.** With `multi: true`/`includeAll`, Grafana renders the selection as `(prod|staging)`; the `=` matcher treats that whole string as a *literal* value and silently matches nothing (no error, blank panel). Both `$namespace` and `$app` above are `multi: true` → MUST use `{namespace=~"$namespace"}`. A Custom `allValue` like `.*` is injected verbatim (not escaped) — intentional, but a footgun in a query string.
>
> **⚠️ `label_values()` ignores the dashboard time range.** It scans ALL series matching the metric — slow on high-series backends and returns stale values (pods gone months ago). For only entities active in the window use `query_result()`, which is time-aware:

```promql
query_result(count by (service) (rate(http_requests_total[$__range])))   # only services seen in window
# vs label_values(http_requests_total, service)  — all-time, slow, stale
```

## Annotations

Event markers on time-series panels, from any data source:

```json
{ "annotations": { "list": [
  { "name": "Deployments", "datasource": "Prometheus",
    "expr": "changes(kube_deployment_spec_replicas{namespace=\"$namespace\"}[5m])",
    "tagKeys": "deployment,namespace", "iconColor": "blue" },
  { "name": "Alerts", "datasource": "Loki",
    "expr": "{app=\"alertmanager\"} | json | alertname!=\"\"", "iconColor": "red" }
] } }
```

## Dashboard as Code & Provisioning

> **Generate JSON from typed models; do not commit hand-edited UI exports.** Grafana 12 ships the **Foundation SDK** (Go/TS/Python/Java/PHP) + the **`gcx` CLI** (REST push; Git Sync = bidirectional). Typed models prevent schema drift (deprecated panel types, wrong field names) that UI exports re-introduce. For production use Foundation SDK + `gcx`, or **Terraform with the Grafana provider**.

```typescript
const dashboard = new DashboardBuilder("App Observability")
  .withPanel(new TimeseriesPanelBuilder().title("Request Rate").unit("reqps"));
// gcx push
```

> **⚠️ `allowUiUpdates: true` silently discards UI edits on the next sync.** UI edits persist across refreshes but the next provisioning sync (`updateIntervalSeconds`, default 10s) overwrites the DB version with the file version, ignoring the DB `version` — no warning. Production: `allowUiUpdates: false`, `editable: false`, `disableDeletion: true`. To keep a UI edit, export the JSON back into the provisioning file.

```yaml
# /etc/grafana/provisioning/dashboards/dashboards.yaml
apiVersion: 1
providers:
  - name: application
    orgId: 1
    folder: Applications
    type: file
    disableDeletion: true
    editable: false
    updateIntervalSeconds: 10
    options: { path: /var/lib/grafana/dashboards/application }
```

> **Folder addressing:** `folderId` (numeric) was removed from API paths in Grafana 10 — use `folderUid` (string; omit/empty = default folder). `overwrite: true` always wins over the DB dashboard regardless of `version`.

**K8s delivery:** a ConfigMap with label `grafana_dashboard: "1"` (sidecar auto-import), or the Grafana Operator `GrafanaDashboard` CRD (`spec.instanceSelector` + inline `json`).

## Data Sources

> **⚠️ Provisioned data sources need an explicit, stable `uid`.** Without one Grafana generates a random UID each startup; dashboard panels embed that UID, so on redeploy/recreate every panel shows "Datasource not found" — silent at provision time (Grafana loads fine, panels broken). Declare a pinned human-readable `uid` and reference it from dashboards via the object form `{type, uid}`, not the legacy name string (name refs break on rename).

```yaml
# datasources.yaml (apiVersion: 1)
datasources:
  - name: Mimir
    type: prometheus
    uid: prometheus_uid           # pinned, stable
    url: http://mimir:8080/prometheus
    jsonData:
      httpMethod: POST
      prometheusType: Mimir
      exemplarTraceIdDestinations: [{ datasourceUid: tempo_uid, name: trace_id }]
      cacheLevel: High

  - name: Loki
    type: loki
    uid: loki_uid
    url: http://loki:3100
    jsonData:
      maxLines: 1000
      derivedFields:             # link a log field to a trace
        - datasourceUid: tempo_uid
          matcherRegex: "trace_id=(\\w+)"
          name: TraceID
          url: "$${__value.raw}"

  - name: Tempo
    type: tempo
    uid: tempo_uid
    url: http://tempo:3200
    jsonData:
      # tracesToLogsV2 supersedes legacy tracesToLogs (tags/mappedTags); adds customQuery
      # with ${__tags}/${__span.traceId} interpolation.
      tracesToLogsV2:
        datasourceUid: loki_uid
        spanStartTimeShift: "-1h"
        spanEndTimeShift: "1h"
        customQuery: true
        query: '{${__tags}} |= "${__span.traceId}"'
      tracesToMetrics: { datasourceUid: prometheus_uid }
      serviceMap: { datasourceUid: prometheus_uid }
      nodeGraph: { enabled: true }
```

## LogQL (Loki)

**Pipeline order = stream selector → line filters → parsers → label filters.** Loki evaluates left-to-right; ordering controls how many lines reach each expensive stage (documented perf rule). Line filters `|=`/`!=` (string, cheapest) before `|~`/`!~` (regex) before parsers (`json`/`logfmt`/`pattern`/`regexp`, most expensive).

```logql
# Stream selection (indexed labels — keep low-cardinality)
{namespace="production", app=~"api|web", level!="debug"}

# Line filters
{app="api"} |= "error" != "timeout" |~ "(?i)fatal"

# Parse + label filter (filter cheaply FIRST, then parse)
{app="api"} |= "error" | json | level="error" | duration > 500ms
{app="api"} | logfmt level, caller, msg
{app="nginx"} | pattern `<ip> - - <_> "<method> <uri> <_>" <status> <_>` | status >= 400

# Metric queries
sum(rate({app="api"} |= "error" [$__rate_interval])) by (namespace)
sum(rate({app="api"} |= "error" [5m])) / sum(rate({app="api"}[5m]))       # error ratio
quantile_over_time(0.95, {app="api"} | logfmt | unwrap duration_seconds(duration) [5m]) by (endpoint)
topk(10, sum by (msg) (count_over_time({app="api"} | json | level="error" [1h])))
```

## TraceQL (Tempo)

**Use scoped attributes (`span.` / `resource.`) and prefer `&&`.** Tempo stores traces in columnar Parquet; unscoped `.attr` checks multiple columns (slower). All-`&&` queries push filtering into Parquet (predicate pushdown — fastest path); `||` and structural operators bypass it.

```traceql
{ span.http.status_code = 500 && resource.service.name = "api" && span.http.method = "POST" }  # fast
{ trace:rootService = "gateway" && trace:duration > 1s }                # end-to-end slow traces
{ .service.name = "frontend" } >> { .service.name = "backend" && status = error }   # structural
```

**TraceQL metrics = RED metrics from traces, no metrics generator** (Tempo 2.4+, query type `traceql_metrics`). Append `rate`/`quantile_over_time`/`count_over_time`/`histogram_over_time`/`compare`/`topk` to a span filter. Enables retroactive dimensional slicing Prometheus can't (p99 of DB spans by `db.system`). Avoid structural operators here — full trace processing, slow.

```traceql
{ span:name = "GET /:endpoint" } | quantile_over_time(duration, .99) by (span.http.target)
{ status = error } | rate() by (resource.service.name)
```

## Grafana Alerting

Unified alerting: multi-datasource rules, provisioned as YAML. Alert *expressions* use PromQL/LogQL (see loom-prometheus / LogQL above); the structure is Grafana-specific.

```yaml
# /etc/grafana/provisioning/alerting/rules.yaml (apiVersion: 1)
groups:
  - name: application_alerts
    interval: 1m
    rules:
      - uid: error_rate_high
        title: High Error Rate
        condition: A
        data:
          - refId: A
            relativeTimeRange: { from: 300, to: 0 }
            datasourceUid: prometheus_uid
            model:
              expr: |
                sum(rate(http_requests_total{status=~"5.."}[5m]))
                / sum(rate(http_requests_total[5m])) > 0.05
        for: 5m                 # pending period before firing
        keep_firing_for: 2m     # anti-flap on exit
        noDataState: NoData
        execErrState: Error
        labels: { severity: critical, team: platform }
        annotations:
          description: 'Error rate {{ printf "%.2f" $values.A.Value }}%'
          runbook_url: https://wiki.company.com/runbooks/high-error-rate

# Loki-backed rule: model.expr = sum(rate({app="api"} | json | level="error" [5m])) > 10
```

Contact points + notification policies (routing tree, matchers, grouping):

```yaml
# contactpoints.yaml / policies
contactPoints:
  - orgId: 1
    name: pagerduty-oncall
    receivers: [{ uid: pd, type: pagerduty, settings: { integrationKey: KEY, severity: critical } }]
notificationPolicies:
  - orgId: 1
    receiver: slack-critical
    group_by: [alertname, namespace]
    group_wait: 30s
    group_interval: 5m
    repeat_interval: 4h
    routes:
      - receiver: pagerduty-oncall
        matchers: [severity = critical, page = true]
        group_wait: 10s
        continue: true
```

## Complete Dashboard Example

RED dashboard: request rate, P95 latency, error ratio, error logs. Note `$__rate_interval` with `rate()`, `percentunit` on the 0–1 ratio, `{type, uid}` datasource refs, and Explore data links.

```json
{
  "dashboard": {
    "title": "Application Observability - ${app}",
    "tags": ["observability"], "graphTooltip": 1, "refresh": "30s",
    "time": { "from": "now-1h", "to": "now" },
    "templating": { "list": [
      { "name": "datasource", "type": "datasource", "query": "prometheus" },
      { "name": "app", "type": "query", "datasource": {"type":"prometheus","uid":"${datasource}"},
        "query": "label_values(up, app)" },
      { "name": "namespace", "type": "query", "datasource": {"type":"prometheus","uid":"${datasource}"},
        "query": "label_values(up{app=\"$app\"}, namespace)", "multi": true, "includeAll": true }
    ] },
    "panels": [
      { "id": 1, "title": "Request Rate", "type": "timeseries",
        "datasource": {"type":"prometheus","uid":"${datasource}"},
        "gridPos": {"h":8,"w":12,"x":0,"y":0},
        "fieldConfig": {"defaults": {"unit": "reqps"}},
        "targets": [{ "expr": "sum(rate(http_requests_total{app=\"$app\", namespace=~\"$namespace\"}[$__rate_interval])) by (method, status)",
          "legendFormat": "{{method}} - {{status}}" }] },
      { "id": 2, "title": "P95 Latency", "type": "timeseries",
        "datasource": {"type":"prometheus","uid":"${datasource}"},
        "gridPos": {"h":8,"w":12,"x":12,"y":0},
        "fieldConfig": {"defaults": {"unit": "s",
          "thresholds": {"mode":"absolute","steps":[{"color":"green","value":null},{"color":"red","value":1}]}}},
        "targets": [{ "expr": "histogram_quantile(0.95, sum(rate(http_request_duration_seconds_bucket{app=\"$app\", namespace=~\"$namespace\"}[$__rate_interval])) by (le, endpoint))",
          "legendFormat": "{{endpoint}}" }] },
      { "id": 3, "title": "Error Rate", "type": "timeseries",
        "datasource": {"type":"prometheus","uid":"${datasource}"},
        "gridPos": {"h":8,"w":12,"x":0,"y":8},
        "fieldConfig": {"defaults": {"unit": "percentunit", "min": 0, "max": 1}},
        "targets": [{ "expr": "sum(rate(http_requests_total{app=\"$app\", namespace=~\"$namespace\", status=~\"5..\"}[$__rate_interval])) / sum(rate(http_requests_total{app=\"$app\", namespace=~\"$namespace\"}[$__rate_interval]))",
          "legendFormat": "Error %" }] },
      { "id": 4, "title": "Recent Error Logs", "type": "logs",
        "datasource": {"type":"loki","uid":"loki_uid"},
        "gridPos": {"h":8,"w":12,"x":12,"y":8},
        "options": {"showTime": true, "wrapLogMessage": true, "enableLogDetails": true},
        "targets": [{ "expr": "{app=\"$app\", namespace=~\"$namespace\"} | json | level=\"error\"", "refId": "A" }] }
    ],
    "links": [
      { "title": "Explore Traces", "type": "link", "icon": "gf-traces",
        "url": "/explore?left={\"datasource\":\"Tempo\",\"queries\":[{\"query\":\"{resource.service.name=\\\"$app\\\"}\",\"queryType\":\"traceql\"}]}" }
    ]
  },
  "overwrite": true, "folderUid": null
}
```

## LGTM Backend Config

```yaml
# loki.yaml — new installs: schema v13 + store tsdb (required for structured metadata, OTLP, bloom)
schema_config:
  configs:
    - from: 2024-01-01
      store: tsdb
      object_store: s3
      schema: v13
      index: { prefix: index_, period: 24h }
compactor:
  retention_enabled: true
  delete_request_store: s3     # REQUIRED in Loki 3.0 when retention_enabled
limits_config:
  retention_period: 744h
# ⚠ shared_store was REMOVED in Loki 3.0 (boltdb_shipper/tsdb_shipper/compactor). Storage is
#   selected per-period via object_store above; a lingering shared_store fails to start.
#   Existing cluster: add a NEW period_config with a future from:, don't mutate the old one.
```

```yaml
# tempo.yaml — OTLP + metrics generator remote_write to Mimir
distributor:
  receivers: { otlp: { protocols: { http: {}, grpc: {} } } }
metrics_generator:
  storage: { path: /var/tempo/generator/wal,
    remote_write: [{ url: http://mimir:9009/api/v1/push, send_exemplars: true }] }
storage:
  trace: { backend: s3, s3: { bucket: tempo-traces, region: us-east-1 }, wal: { path: /var/tempo/wal } }
```

## Expert Practices: Idioms, Anti-Patterns & Gotchas

Mechanism-level rules. Many are *silent* — no error, just wrong or empty panels.

### Query Idioms

**Use `$__rate_interval` (never `$__interval` or fixed `[5m]`) with `rate()`/`increase()`.** These need ≥2 samples in the range vector. `$__interval` is the display step and shrinks below the scrape interval on zoom → empty/NaN gaps. `$__rate_interval = max($__interval + scrape_interval, 4 * scrape_interval)`, always ≥4× scrape. Reserve `$__interval` for `avg_over_time`-style aggregations. Silent correctness bug at narrow zoom.

**Order Grafana transformations: normalize → combine(join) → filter → reduce.** A pipeline; each step feeds the next. Classic mistake: **Reduce** (collapses each series to one value) *before* a **Join** destroys the shared Time field the join keys on → nothing matches. Correct: convert types; Outer join on Time (or Merge); filter; reduce last only if a scalar is needed. **Merge joins on ALL matching fields** — queries sharing more fields than intended multiply rows; use Outer join on Time explicitly. The transformation debug icon shows each step's output shape.

### LogQL Gotchas (silent wrong/empty data)

**Parsers add an `__error__` label rather than dropping lines.** A failed `json`/`logfmt`/`pattern` line is NOT filtered — it gets `__error__` and still counts, inflating aggregations. Filter `| __error__=""`. Also: bare `| json` silently skips array values (`| json first="servers[0]"`); an extracted label colliding with a stream label is suffixed `_extracted` — reconcile with `| label_format level=level_extracted`.

```logql
{app="api"} | json | __error__="" | level="error"   # excludes parse failures
{app="api"} | json | level="error"                  # silently includes failed-parse lines
```

**`unwrap` defaults to raw float64 — use `duration_seconds()` / `bytes()`.** `150ms` or `5 MiB` fails raw conversion; instead of erroring, the line gets `__error__` and drops out, so `quantile_over_time`/`avg_over_time` compute over only the parseable subset — silently. `duration_seconds()` expects Go format (`1s`,`100ms`); a field holding integer `1000` (ms) must be unwrapped raw and divided by 1000.

**LogQL label-filter boolean precedence is strictly left-to-right, NOT AND-before-OR.** `| duration >= 20ms or method="GET" and size <= 20KB` parses as `((duration>=20ms or method="GET") and size<=20KB)`. Always parenthesize multi-predicate filters.

**Loki labels are streams — keep high-cardinality fields in structured metadata, not labels.** Each unique label set is a separate stream with its own chunks; high-card labels (`trace_id`, `request_id`, `pod`, `user_id`) explode the index and force tiny-chunk write amplification. Loki 3.0 (v13+TSDB) adds **structured metadata**: per-line KV, not indexed but queryable; the OTLP endpoint maps non-default OTel attributes there. Critical: structured metadata CANNOT go in the `{}` selector — it's a pipeline filter (`| trace_id="..."`) with no index pre-filter. Keep dynamic labels to single-/low-tens cardinality.

```logql
{namespace="prod", app="api"} | trace_id="0242ac12" | pod="svc-abc"   # low-card labels, rest filtered
{namespace="prod", app="api", trace_id="0242ac12", pod="svc-abc"}     # stream explosion
```

### TraceQL Gotcha

**Structural operators return the RIGHT-side spans; `span:duration` ≠ `trace:duration`.** `{ .service.name="gateway" } >> { status=error }` returns the error *descendant* spans, not the gateway parent. `span:duration` is one span's `end-start`; `trace:duration` is whole-request wall time (`max(end)-min(start)`) and trace-level intrinsics are far more performant — use `trace:duration` for end-to-end latency.

### Alerting Gotchas

**NoData / Error create synthetic `DatasourceNoData` / `DatasourceError` instances that bypass existing silences.** They carry different labels (`alertname=DatasourceNoData`, plus `datasource_uid`, `rulename`) from the parent, so silences/mute-timings/policies keyed to the parent do NOT apply — the #1 surprise-page source. Options: Set NoData (default), Set Alerting, Set Normal, Keep last state; `for` applies (Normal→Pending→NoData); `grafana_state_reason` explains divergence. To suppress: silence/route on the synthetic labels, or `noDataState: KeepLast` for transient hiccups.

**Symptom-based alerting + `for`/`keep_firing_for`** — page on user-visible latency/errors/availability, not causes (see loom-prometheus for the SRE rationale). Grafana-specific: pair `for` (pending) with `keep_firing_for` (anti-flap on exit).

### Currency: removed mechanisms (Grafana 12 / Loki 3.0 / Tempo 2.4+)

**Graph (old) panel and in-panel `alert` blocks are gone.** AngularJS was removed in Grafana 12; `"type": "graph"` (with `yaxes[]` + root-level `thresholds`) is force-migrated to `timeseries` on load, dropping that config — emit `"type": "timeseries"` with `fieldConfig.defaults`. The panel-level `"alert"` block was removed with legacy alerting in Grafana 11 — completely inert (no rule, no firing). Define alerts under `/etc/grafana/provisioning/alerting/`.

**Prometheus native histograms are GA.** `histogram_quantile()` works on the native metric directly — no `_bucket`, no `by(le)` (details → loom-prometheus). **Grafana caveat:** the Time Series panel can't render a native histogram; use Histogram or Heatmap.

## Verification Checklist

- [ ] Every `rate()`/`increase()` in a panel/alert uses `[$__rate_interval]`, not `[$__interval]` or a fixed window
- [ ] Multi-value / Include-All variables are matched with `=~` (never `=`)
- [ ] Panels reference data sources by `{type, uid}`; provisioned data sources have a pinned `uid`
- [ ] Every panel sets an explicit `unit` (`percentunit` for 0–1 ratios); titles include units
- [ ] `"type": "timeseries"` (no `"graph"`); no in-panel `"alert"` blocks
- [ ] LogQL pipeline is selector → line filter → parser → label filter; parsers followed by `| __error__=""`; `unwrap` uses `duration_seconds()`/`bytes()`
- [ ] TraceQL uses scoped `span.`/`resource.` attributes and `trace:duration` for end-to-end latency
- [ ] Loki: schema v13 + tsdb; no `shared_store`; `delete_request_store` set when retention enabled
- [ ] Alert rules account for NoData/Error routing on synthetic labels
- [ ] Provisioning: production dashboards `allowUiUpdates: false`; generated from typed models, not UI exports

## Resources

- Grafana: <https://grafana.com/docs/grafana/latest/> · LogQL: /docs/loki/latest/query/
- Tempo/TraceQL: <https://grafana.com/docs/tempo/latest/traceql/>
- Foundation SDK + `gcx`, Terraform Grafana provider, Grafana Operator
