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

# Grafana and LGTM Stack Skill

## Overview

The LGTM stack provides a complete observability solution with comprehensive visualization and dashboard capabilities:

- **Loki**: Log aggregation and querying (LogQL)
- **Grafana**: Visualization, dashboarding, alerting, and exploration
- **Tempo**: Distributed tracing (TraceQL)
- **Mimir**: Long-term metrics storage (Prometheus-compatible)

This skill covers setup, configuration, dashboard creation, panel design, querying, alerting, templating, and production observability best practices.

## When to Use This Skill

### Primary Use Cases

- Creating or modifying Grafana dashboards
- Designing panels and visualizations (graphs, stats, tables, heatmaps, etc.)
- Writing queries (PromQL, LogQL, TraceQL)
- Configuring data sources (Prometheus, Loki, Tempo, Mimir)
- Setting up alerting rules and notification policies
- Implementing dashboard variables and templates
- Dashboard provisioning and GitOps workflows
- Troubleshooting observability queries
- Analyzing application performance, errors, or system behavior

### Who Uses This Skill

- **senior-software-engineer** (DEFAULT): Production observability setup, LGTM stack deployment, dashboard architecture, application dashboards, service metrics visualization
- **software-engineer**: ONLY for boilerplate dashboard panels following established patterns or scaffolding from a concrete plan

## LGTM Stack Components

### Loki - Log Aggregation

#### Architecture - Loki

Horizontally scalable log aggregation inspired by Prometheus

- Indexes only metadata (labels), not log content
- Cost-effective storage with object stores (S3, GCS, etc.)
- LogQL query language similar to PromQL

#### Key Concepts - Loki

- Labels for indexing (low cardinality)
- Log streams identified by unique label sets
- Parsers: logfmt, JSON, regex, pattern
- Line filters and label filters

### Grafana - Visualization

#### Features

- Multi-datasource dashboarding
- Panel types: Time Series, Stat, Table, Heatmap, Bar Chart, Pie Chart, Gauge, Logs, Traces (the AngularJS Graph (old) panel was removed in Grafana 12)
- Templating and variables for dynamic dashboards
- Alerting (unified alerting with contact points and notification policies)
- Dashboard provisioning and GitOps integration
- Role-based access control (RBAC)
- Explore mode for ad-hoc queries
- Annotations for event markers
- Dashboard folders and organization

### Tempo - Distributed Tracing

#### Architecture - Tempo

Scalable distributed tracing backend

- Cost-effective trace storage
- TraceQL for trace querying
- Integration with logs and metrics (trace-to-logs, trace-to-metrics)
- OpenTelemetry compatible

### Mimir - Metrics Storage

#### Architecture - Mimir

Horizontally scalable long-term Prometheus storage

- Multi-tenancy support
- Query federation
- High availability
- Prometheus remote_write compatible

## Dashboard Design and Best Practices

### Dashboard Organization Principles

1. **Hierarchy**: Overview -> Service -> Component -> Deep Dive
2. **Golden Signals**: Latency, Traffic, Errors, Saturation (RED/USE method)
3. **Variable-driven**: Use templates for flexibility across environments
4. **Consistent Layouts**: Grid alignment (24-column grid), logical top-to-bottom flow
5. **Performance**: Limit queries, use query caching, appropriate time intervals

### Panel Types and When to Use Them

| Panel Type              | Use Case               | Best For                                         |
| ----------------------- | ---------------------- | ------------------------------------------------ |
| **Time Series**         | Trends over time       | Request rates, latency, resource usage           |
| **Stat**                | Single metric value    | Error rates, current values, percentage          |
| **Gauge**               | Progress toward limit  | CPU usage, memory, disk space                    |
| **Bar Gauge**           | Comparative values     | Top N items, distribution                        |
| **Table**               | Structured data        | Service lists, error details, resource inventory |
| **Pie Chart**           | Proportions            | Traffic distribution, error breakdown            |
| **Heatmap**             | Distribution over time | Latency percentiles, request patterns            |
| **Logs**                | Log streams            | Error investigation, debugging                   |
| **Traces**              | Distributed tracing    | Performance analysis, dependency mapping         |

### Panel Configuration Best Practices

#### Titles and Descriptions

- **Clear, descriptive titles**: Include units and metric context
- **Tooltips**: Add description fields for panel documentation
- **Examples**:
  - Good: "P95 Latency (seconds) by Endpoint"
  - Bad: "Latency"

#### Legends and Labels

- Show legends only when needed (multiple series)
- Use `{{label}}` format for dynamic legend names
- Place legends appropriately (bottom, right, or hidden)
- Sort by value when showing Top N

#### Axes and Units

- Always label axes with units
- Use appropriate unit formats (seconds, bytes, percent, requests/sec)
- Set reasonable min/max ranges to avoid misleading scales
- Use logarithmic scales for wide value ranges

#### Thresholds and Colors

- Use thresholds for visual cues (green/yellow/red)
- Standard threshold pattern:
  - Green: Normal operation
  - Yellow: Warning (action may be needed)
  - Red: Critical (immediate attention required)
- Examples:
  - Error rate: 0% (green), 1% (yellow), 5% (red)
  - P95 latency: <1s (green), 1-3s (yellow), >3s (red)

#### Links and Drilldowns

- Link panels to related dashboards
- Use data links for context (logs, traces, related services)
- Create drill-down paths: Overview -> Service -> Component -> Details
- Link to runbooks for alert panels

### Dashboard Variables and Templating

Dashboard variables enable reusable, dynamic dashboards that work across environments, services, and time ranges.

#### Variable Types

| Type           | Purpose                     | Example                         |
| -------------- | --------------------------- | ------------------------------- |
| **Query**      | Populate from data source   | Namespaces, services, pods      |
| **Custom**     | Static list of options      | Environments (prod/staging/dev) |
| **Interval**   | Time interval selection     | Auto-adjusted query intervals   |
| **Datasource** | Switch between data sources | Multiple Prometheus instances   |
| **Constant**   | Hidden values for queries   | Cluster name, region            |
| **Text box**   | Free-form input             | Custom filters                  |

#### Common Variable Patterns

```json
{
  "templating": {
    "list": [
      {
        "name": "datasource",
        "type": "datasource",
        "query": "prometheus",
        "description": "Select Prometheus data source"
      },
      {
        "name": "namespace",
        "type": "query",
        "datasource": "${datasource}",
        "query": "label_values(kube_pod_info, namespace)",
        "multi": true,
        "includeAll": true,
        "description": "Kubernetes namespace filter"
      },
      {
        "name": "app",
        "type": "query",
        "datasource": "${datasource}",
        "query": "label_values(kube_pod_info{namespace=~\"$namespace\"}, app)",
        "multi": true,
        "includeAll": true,
        "description": "Application filter (depends on namespace)"
      },
      {
        "name": "interval",
        "type": "interval",
        "auto": true,
        "auto_count": 30,
        "auto_min": "10s",
        "options": ["1m", "5m", "15m", "30m", "1h", "6h", "12h", "1d"],
        "description": "Query resolution interval"
      },
      {
        "name": "environment",
        "type": "custom",
        "options": [
          { "text": "Production", "value": "prod" },
          { "text": "Staging", "value": "staging" },
          { "text": "Development", "value": "dev" }
        ],
        "current": { "text": "Production", "value": "prod" }
      }
    ]
  }
}
```

#### Variable Usage in Queries

Variables are referenced with `$variable_name` or `${variable_name}` syntax:

```promql
# Multi-value / Include-All variable -> ALWAYS use =~ (regex matcher)
rate(http_requests_total{namespace=~"$namespace"}[$__rate_interval])

# Single-value variable -> = is fine
rate(http_requests_total{namespace="$namespace"}[$__rate_interval])

# Variable in legend
sum(rate(http_requests_total{app=~"$app"}[$__rate_interval])) by (method)
# Legend format: "{{method}}"

# Chained variables (app depends on namespace)
rate(http_requests_total{namespace=~"$namespace", app=~"$app"}[$__rate_interval])
```

> **⚠️ Multi-value/Include-All variables interpolate as a regex — use `=~`, never `=`.** When a variable has `multi: true` or `includeAll`, Grafana renders the selection as a pipe-joined alternation, e.g. `(prod|staging)`. With the `=` matcher that whole string is treated as a *literal* label value and silently matches nothing — no error, just empty panels. Both the `namespace` and `app` variables in the templating examples above set `multi: true`, so they MUST be matched with `=~`. (The broken case: `{namespace="$namespace"}` returns no data once more than one value is selected.) Separately, a Custom `allValue` such as `.*` is NOT escaped by Grafana, so any regex there is injected verbatim — intentional, but a footgun if it reaches a query string.

#### Advanced Variable Techniques

**Regex filtering**:

```json
{
  "name": "pod",
  "type": "query",
  "query": "label_values(kube_pod_info{namespace=\"$namespace\"}, pod)",
  "regex": "/^$app-.*/",
  "description": "Filter pods by app prefix"
}
```

**All option with custom value**:

```json
{
  "name": "status",
  "type": "custom",
  "options": ["200", "404", "500"],
  "includeAll": true,
  "allValue": ".*",
  "description": "HTTP status code filter"
}
```

**Dependent variables** (variable chain):

1. `$datasource` (datasource type)
2. `$cluster` (query: depends on datasource)
3. `$namespace` (query: depends on cluster)
4. `$app` (query: depends on namespace)
5. `$pod` (query: depends on app)

> **`label_values()` ignores the dashboard time range.** `label_values(metric, label)` scans ALL series matching the metric to extract values and does NOT filter by the selected time window — on high-series-count Prometheus/Mimir it is slow and returns stale values (e.g. pods that vanished months ago). `label_values` does not support queries, so when you need only entities active in the current window, use the time-aware `query_result()` instead:

```promql
# Time-range-aware: only services seen in the current window
query_result(count by (service) (rate(http_requests_total[$__range])))
# vs label_values(http_requests_total, service) — all-time, slow, stale
```

### Annotations

Annotations display events as vertical markers on time series panels:

```json
{
  "annotations": {
    "list": [
      {
        "name": "Deployments",
        "datasource": "Prometheus",
        "expr": "changes(kube_deployment_spec_replicas{namespace=\"$namespace\"}[5m])",
        "tagKeys": "deployment,namespace",
        "textFormat": "Deployment: {{deployment}}",
        "iconColor": "blue"
      },
      {
        "name": "Alerts",
        "datasource": "Loki",
        "expr": "{app=\"alertmanager\"} | json | alertname!=\"\"",
        "textFormat": "Alert: {{alertname}}",
        "iconColor": "red"
      }
    ]
  }
}
```

### Dashboard Performance Optimization

#### Query Optimization

- Limit number of panels (< 15 per dashboard)
- Use appropriate time ranges (avoid queries over months)
- Leverage `$__interval` for adaptive sampling
- Avoid high-cardinality grouping (too many series)
- Use query caching when available

#### Panel Performance

- Set max data points to reasonable values
- Use instant queries for current-state panels
- Combine related metrics into single queries when possible
- Disable auto-refresh on heavy dashboards

## Dashboard as Code and Provisioning

> **Generate dashboard JSON; do not commit hand-edited UI exports.** Grafana 12 ships an observability-as-code toolchain: the **Foundation SDK** (Go/TS/Python/Java/PHP) builds dashboard JSON from typed models, and the **`gcx` CLI** pushes resources via the REST API (Git Sync offers bidirectional repo sync). Typed models prevent the schema drift — deprecated panel types, wrong field names — that raw UI exports re-introduce (e.g. a re-imported `"type": "graph"`). Use the Foundation SDK + `gcx`, or **Terraform with the Grafana provider**, for production dashboards.

```typescript
// Foundation SDK (TypeScript) — typed, drift-resistant
const dashboard = new DashboardBuilder("App Observability").withPanel(
  new TimeseriesPanelBuilder().title("Request Rate").unit("reqps"),
);
// push via: gcx push
```

### Dashboard Provisioning

Dashboard provisioning enables GitOps workflows and version-controlled dashboard definitions.

> **`allowUiUpdates: true` silently discards UI edits on the next sync.** With this provider option, UI edits to provisioned dashboards persist across refreshes — but the next provisioning sync (`updateIntervalSeconds`, default 10s) overwrites the DB version with the file version, ignoring the DB version number, with no warning. For production prefer `allowUiUpdates: false` with `editable: false` and `disableDeletion: true`; if dev UI editing is needed, export the modified JSON back into the provisioning file.

#### Provisioning Provider Configuration

File: `/etc/grafana/provisioning/dashboards/dashboards.yaml`

```yaml
apiVersion: 1

providers:
  - name: "default"
    orgId: 1
    folder: ""
    type: file
    disableDeletion: false
    updateIntervalSeconds: 10
    allowUiUpdates: true
    options:
      path: /etc/grafana/provisioning/dashboards

  - name: "application"
    orgId: 1
    folder: "Applications"
    type: file
    disableDeletion: true
    editable: false
    options:
      path: /var/lib/grafana/dashboards/application

  - name: "infrastructure"
    orgId: 1
    folder: "Infrastructure"
    type: file
    options:
      path: /var/lib/grafana/dashboards/infrastructure
```

#### Dashboard JSON Structure

Complete dashboard JSON with metadata and provisioning:

```json
{
  "dashboard": {
    "title": "Application Observability - ${app}",
    "uid": "app-observability",
    "tags": ["observability", "application"],
    "timezone": "browser",
    "editable": true,
    "graphTooltip": 1,
    "time": {
      "from": "now-1h",
      "to": "now"
    },
    "refresh": "30s",
    "templating": { "list": [] },
    "panels": [],
    "links": []
  },
  "overwrite": true,
  "folderUid": null
}
```

> **Folder addressing:** `folderId` (the legacy numeric identifier) is deprecated and was removed from the relevant API paths in Grafana 10 — use only `folderUid` (a string UID, or omit/empty for the default folder). Note that `overwrite: true` always wins over the DB dashboard regardless of the `version` field.

#### Kubernetes ConfigMap Provisioning

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: grafana-dashboards
  namespace: monitoring
  labels:
    grafana_dashboard: "1"
data:
  application-dashboard.json: |
    {
      "dashboard": {
        "title": "Application Metrics",
        "uid": "app-metrics",
        "tags": ["application"],
        "panels": []
      }
    }
```

#### Grafana Operator (CRD)

```yaml
apiVersion: grafana.integreatly.org/v1beta1
kind: GrafanaDashboard
metadata:
  name: application-observability
  namespace: monitoring
spec:
  instanceSelector:
    matchLabels:
      dashboards: "grafana"
  json: |
    {
      "dashboard": {
        "title": "Application Observability",
        "panels": []
      }
    }
```

### Data Source Provisioning

#### Loki Data Source

File: `/etc/grafana/provisioning/datasources/loki.yaml`

```yaml
apiVersion: 1

datasources:
  - name: Loki
    type: loki
    access: proxy
    url: http://loki:3100
    jsonData:
      maxLines: 1000
      derivedFields:
        - datasourceUid: tempo_uid
          matcherRegex: "trace_id=(\\w+)"
          name: TraceID
          url: "$${__value.raw}"
    editable: false
```

#### Tempo Data Source

File: `/etc/grafana/provisioning/datasources/tempo.yaml`

```yaml
apiVersion: 1

datasources:
  - name: Tempo
    type: tempo
    access: proxy
    url: http://tempo:3200
    uid: tempo_uid
    jsonData:
      httpMethod: GET
      # tracesToLogsV2 supersedes the legacy tracesToLogs block (tags/mappedTags).
      # V2 adds customQuery, a query template with ${__tags}/${__span.traceId}
      # interpolation, and separate filterByTraceID/filterBySpanID flags.
      tracesToLogsV2:
        datasourceUid: loki_uid
        spanStartTimeShift: "-1h"
        spanEndTimeShift: "1h"
        filterByTraceID: true
        filterBySpanID: true
        customQuery: true
        query: '{${__tags}} |= "${__span.traceId}"'
      tracesToMetrics:
        datasourceUid: prometheus_uid
        tags: [{ key: "service.name", value: "service" }]
      serviceMap:
        datasourceUid: prometheus_uid
      nodeGraph:
        enabled: true
    editable: false
```

#### Mimir/Prometheus Data Source

File: `/etc/grafana/provisioning/datasources/mimir.yaml`

```yaml
apiVersion: 1

datasources:
  - name: Mimir
    type: prometheus
    access: proxy
    url: http://mimir:8080/prometheus
    uid: prometheus_uid
    jsonData:
      httpMethod: POST
      exemplarTraceIdDestinations:
        - datasourceUid: tempo_uid
          name: trace_id
      prometheusType: Mimir
      prometheusVersion: 2.40.0
      cacheLevel: "High"
      incrementalQuerying: true
      incrementalQueryOverlapWindow: 10m
    editable: false
```

## Alerting

### Alert Rule Configuration

Grafana unified alerting supports multi-datasource alerts with flexible evaluation and routing.

#### Prometheus/Mimir Alert Rule

File: `/etc/grafana/provisioning/alerting/rules.yaml`

```yaml
apiVersion: 1

groups:
  - name: application_alerts
    interval: 1m
    rules:
      - uid: error_rate_high
        title: High Error Rate
        condition: A
        data:
          - refId: A
            queryType: ""
            relativeTimeRange:
              from: 300
              to: 0
            datasourceUid: prometheus_uid
            model:
              expr: |
                sum(rate(http_requests_total{status=~"5.."}[5m]))
                /
                sum(rate(http_requests_total[5m]))
                > 0.05
              intervalMs: 1000
              maxDataPoints: 43200
        noDataState: NoData
        execErrState: Error
        for: 5m
        annotations:
          description: 'Error rate is {{ printf "%.2f" $values.A.Value }}% (threshold: 5%)'
          summary: Application error rate is above threshold
          runbook_url: https://wiki.company.com/runbooks/high-error-rate
        labels:
          severity: critical
          team: platform
        isPaused: false

      - uid: high_latency
        title: High P95 Latency
        condition: A
        data:
          - refId: A
            datasourceUid: prometheus_uid
            model:
              expr: |
                histogram_quantile(0.95,
                  sum(rate(http_request_duration_seconds_bucket[5m])) by (le, endpoint)
                ) > 2
        for: 10m
        annotations:
          description: "P95 latency is {{ $values.A.Value }}s on endpoint {{ $labels.endpoint }}"
          runbook_url: https://wiki.company.com/runbooks/high-latency
        labels:
          severity: warning
```

#### Loki Alert Rule

```yaml
apiVersion: 1

groups:
  - name: log_based_alerts
    interval: 1m
    rules:
      - uid: error_spike
        title: Error Log Spike
        condition: A
        data:
          - refId: A
            queryType: ""
            datasourceUid: loki_uid
            model:
              expr: |
                sum(rate({app="api"} | json | level="error" [5m]))
                > 10
        for: 2m
        annotations:
          description: "Error log rate is {{ $values.A.Value }} logs/sec"
          summary: Spike in error logs detected
        labels:
          severity: warning

      - uid: critical_error_pattern
        title: Critical Error Pattern Detected
        condition: A
        data:
          - refId: A
            datasourceUid: loki_uid
            model:
              expr: |
                sum(count_over_time({app="api"}
                  |~ "OutOfMemoryError|StackOverflowError|FatalException" [5m]
                )) > 0
        for: 0m
        annotations:
          description: "Critical error pattern found in logs"
        labels:
          severity: critical
          page: true
```

### Contact Points and Notification Policies

File: `/etc/grafana/provisioning/alerting/contactpoints.yaml`

```yaml
apiVersion: 1

contactPoints:
  - orgId: 1
    name: slack-critical
    receivers:
      - uid: slack_critical
        type: slack
        settings:
          url: https://hooks.slack.com/services/YOUR/WEBHOOK/URL
          title: "{{ .GroupLabels.alertname }}"
          text: |
            {{ range .Alerts }}
            *Alert:* {{ .Labels.alertname }}
            *Summary:* {{ .Annotations.summary }}
            *Description:* {{ .Annotations.description }}
            *Severity:* {{ .Labels.severity }}
            {{ end }}
        disableResolveMessage: false

  - orgId: 1
    name: pagerduty-oncall
    receivers:
      - uid: pagerduty_oncall
        type: pagerduty
        settings:
          integrationKey: YOUR_INTEGRATION_KEY
          severity: critical
          class: infrastructure

  - orgId: 1
    name: email-team
    receivers:
      - uid: email_team
        type: email
        settings:
          addresses: team@company.com
          singleEmail: true

notificationPolicies:
  - orgId: 1
    receiver: slack-critical
    group_by: ["alertname", "namespace"]
    group_wait: 30s
    group_interval: 5m
    repeat_interval: 4h
    routes:
      - receiver: pagerduty-oncall
        matchers:
          - severity = critical
          - page = true
        group_wait: 10s
        repeat_interval: 1h
        continue: true

      - receiver: email-team
        matchers:
          - severity = warning
          - team = platform
        group_interval: 10m
        repeat_interval: 12h
```

## LogQL Query Patterns

### Basic Log Queries

#### Stream Selection

```logql
# Simple label matching
{namespace="production", app="api"}

# Regex matching
{app=~"api|web|worker"}

# Not equal
{env!="staging"}

# Multiple conditions
{namespace="production", app="api", level!="debug"}
```

#### Line Filters

```logql
# Contains
{app="api"} |= "error"

# Does not contain
{app="api"} != "debug"

# Regex match
{app="api"} |~ "error|exception|fatal"

# Case insensitive
{app="api"} |~ "(?i)error"

# Chaining filters
{app="api"} |= "error" != "timeout"
```

### Parsing and Extraction

#### JSON Parsing

```logql
# Parse JSON logs
{app="api"} | json

# Extract specific fields
{app="api"} | json message="msg", level="severity"

# Filter on extracted field
{app="api"} | json | level="error"

# Nested JSON
{app="api"} | json | line_format "{{.response.status}}"
```

#### Logfmt Parsing

```logql
# Parse logfmt (key=value)
{app="api"} | logfmt

# Extract specific fields
{app="api"} | logfmt level, caller, msg

# Filter parsed fields
{app="api"} | logfmt | level="error"
```

#### Pattern Parsing

```logql
# Extract with pattern
{app="nginx"} | pattern `<ip> - - <_> "<method> <uri> <_>" <status> <_>`

# Filter on extracted values
{app="nginx"} | pattern `<_> <status> <_>` | status >= 400

# Complex pattern
{app="api"} | pattern `level=<level> msg="<msg>" duration=<duration>ms`
```

### Aggregations and Metrics

#### Count Queries

```logql
# Count log lines over time
count_over_time({app="api"}[5m])

# Rate of logs
rate({app="api"}[5m])

# Errors per second
sum(rate({app="api"} |= "error" [5m])) by (namespace)

# Error ratio
sum(rate({app="api"} |= "error" [5m]))
/
sum(rate({app="api"}[5m]))
```

#### Extracted Metrics

```logql
# Average duration
avg_over_time({app="api"}
  | logfmt
  | unwrap duration [5m]) by (endpoint)

# P95 latency
quantile_over_time(0.95, {app="api"}
  | regexp `duration=(?P<duration>[0-9.]+)ms`
  | unwrap duration [5m]) by (method)

# Top 10 error messages
topk(10,
  sum by (msg) (
    count_over_time({app="api"}
      | json
      | level="error" [1h]
    )
  )
)
```

## TraceQL Query Patterns

### Basic Trace Queries

```traceql
# Find traces by service
{ .service.name = "api" }

# HTTP status codes
{ .http.status_code = 500 }

# Combine conditions
{ .service.name = "api" && .http.status_code >= 400 }

# Duration filter
{ duration > 1s }
```

### Advanced TraceQL

```traceql
# Parent-child relationship
{ .service.name = "frontend" }
  >> { .service.name = "backend" && .http.status_code = 500 }

# Descendant spans
{ .service.name = "api" }
  >>+ { .db.system = "postgresql" && duration > 1s }

# Failed database queries
{ .service.name = "api" }
  >> { .db.system = "postgresql" && status = "error" }
```

## Complete Dashboard Examples

### Application Observability Dashboard

```json
{
  "dashboard": {
    "title": "Application Observability - ${app}",
    "tags": ["observability", "application"],
    "timezone": "browser",
    "editable": true,
    "graphTooltip": 1,
    "time": {
      "from": "now-1h",
      "to": "now"
    },
    "templating": {
      "list": [
        {
          "name": "datasource",
          "type": "datasource",
          "query": "prometheus"
        },
        {
          "name": "app",
          "type": "query",
          "datasource": { "type": "prometheus", "uid": "${datasource}" },
          "query": "label_values(up, app)",
          "current": {
            "selected": false,
            "text": "api",
            "value": "api"
          }
        },
        {
          "name": "namespace",
          "type": "query",
          "datasource": { "type": "prometheus", "uid": "${datasource}" },
          "query": "label_values(up{app=\"$app\"}, namespace)",
          "multi": true,
          "includeAll": true
        }
      ]
    },
    "panels": [
      {
        "id": 1,
        "title": "Request Rate",
        "type": "timeseries",
        "datasource": { "type": "prometheus", "uid": "${datasource}" },
        "targets": [
          {
            "expr": "sum(rate(http_requests_total{app=\"$app\", namespace=~\"$namespace\"}[$__rate_interval])) by (method, status)",
            "legendFormat": "{{method}} - {{status}}"
          }
        ],
        "gridPos": {
          "h": 8,
          "w": 12,
          "x": 0,
          "y": 0
        },
        "fieldConfig": {
          "defaults": {
            "unit": "reqps",
            "custom": { "lineWidth": 1, "fillOpacity": 10 }
          }
        }
      },
      {
        "id": 2,
        "title": "P95 Latency",
        "type": "timeseries",
        "datasource": { "type": "prometheus", "uid": "${datasource}" },
        "targets": [
          {
            "expr": "histogram_quantile(0.95, sum(rate(http_request_duration_seconds_bucket{app=\"$app\", namespace=~\"$namespace\"}[$__rate_interval])) by (le, endpoint))",
            "legendFormat": "{{endpoint}}"
          }
        ],
        "gridPos": {
          "h": 8,
          "w": 12,
          "x": 12,
          "y": 0
        },
        "fieldConfig": {
          "defaults": {
            "unit": "s",
            "thresholds": {
              "mode": "absolute",
              "steps": [
                { "color": "green", "value": null },
                { "color": "red", "value": 1 }
              ]
            }
          }
        }
      },
      {
        "id": 3,
        "title": "Error Rate",
        "type": "timeseries",
        "datasource": { "type": "prometheus", "uid": "${datasource}" },
        "targets": [
          {
            "expr": "sum(rate(http_requests_total{app=\"$app\", namespace=~\"$namespace\", status=~\"5..\"}[$__rate_interval])) / sum(rate(http_requests_total{app=\"$app\", namespace=~\"$namespace\"}[$__rate_interval]))",
            "legendFormat": "Error %"
          }
        ],
        "gridPos": {
          "h": 8,
          "w": 12,
          "x": 0,
          "y": 8
        },
        "fieldConfig": {
          "defaults": {
            "unit": "percentunit",
            "min": 0,
            "max": 1
          }
        }
      },
      {
        "id": 4,
        "title": "Recent Error Logs",
        "type": "logs",
        "datasource": { "type": "loki", "uid": "loki_uid" },
        "targets": [
          {
            "expr": "{app=\"$app\", namespace=~\"$namespace\"} | json | level=\"error\"",
            "refId": "A"
          }
        ],
        "options": {
          "showTime": true,
          "showLabels": false,
          "showCommonLabels": false,
          "wrapLogMessage": true,
          "dedupStrategy": "none",
          "enableLogDetails": true
        },
        "gridPos": {
          "h": 8,
          "w": 12,
          "x": 12,
          "y": 8
        }
      }
    ],
    "links": [
      {
        "title": "Explore Logs",
        "url": "/explore?left={\"datasource\":\"Loki\",\"queries\":[{\"expr\":\"{app=\\\"$app\\\",namespace=~\\\"$namespace\\\"}\"}]}",
        "type": "link",
        "icon": "doc"
      },
      {
        "title": "Explore Traces",
        "url": "/explore?left={\"datasource\":\"Tempo\",\"queries\":[{\"query\":\"{resource.service.name=\\\"$app\\\"}\",\"queryType\":\"traceql\"}]}",
        "type": "link",
        "icon": "gf-traces"
      }
    ]
  }
}
```

> **Two removed mechanisms NOT used above (and why):** The AngularJS `"type": "graph"` panel is gone — Angular support was removed in Grafana 12 and any `graph` panel (with its `yaxes[]` array and root-level `thresholds`) is force-migrated to `timeseries` on load, dropping the legacy config. Emit `"type": "timeseries"` with `fieldConfig.defaults` instead. Likewise, the in-panel `"alert"` block was removed with legacy alerting in Grafana 11 — that JSON is completely inert (no rule, no firing). Define alerts as Grafana Alerting rules under `/etc/grafana/provisioning/alerting/` (see the Alerting section), referencing the datasource by UID.

## LGTM Stack Configuration

### Loki Configuration

File: `loki.yaml`

```yaml
auth_enabled: false

server:
  http_listen_port: 3100
  grpc_listen_port: 9096
  log_level: info

common:
  path_prefix: /loki
  storage:
    filesystem:
      chunks_directory: /loki/chunks
      rules_directory: /loki/rules
  replication_factor: 1
  ring:
    kvstore:
      store: inmemory

schema_config:
  configs:
    - from: 2024-01-01
      store: tsdb
      object_store: s3
      schema: v13
      index:
        prefix: index_
        period: 24h

storage_config:
  aws:
    s3: s3://us-east-1/my-loki-bucket
    s3forcepathstyle: true
  tsdb_shipper:
    active_index_directory: /loki/tsdb-index
    cache_location: /loki/tsdb-cache
    # NOTE: shared_store was REMOVED in Loki 3.0 (boltdb_shipper, tsdb_shipper,
    # AND compactor). Storage is now selected by object_store in each
    # schema_config period_config entry (above). A config still carrying
    # shared_store fails to start ("field shared_store not found ...").

limits_config:
  retention_period: 744h # 31 days
  ingestion_rate_mb: 10
  ingestion_burst_size_mb: 20
  max_query_series: 500
  max_query_lookback: 30d
  reject_old_samples: true
  reject_old_samples_max_age: 168h

compactor:
  working_directory: /loki/compactor
  compaction_interval: 10m
  retention_enabled: true
  retention_delete_delay: 2h
  delete_request_store: s3 # REQUIRED in Loki 3.0 when retention_enabled: true
```

> **Loki 3.0 (March 2024) config changes:** New installs should use `schema: v13` + `store: tsdb` (required for structured metadata, native OTLP ingestion, and bloom filters). For an existing cluster, add a NEW `period_config` entry with a future `from:` date rather than mutating the old one.

### Tempo Configuration

File: `tempo.yaml`

```yaml
server:
  http_listen_port: 3200
  grpc_listen_port: 9096

distributor:
  receivers:
    otlp:
      protocols:
        http:
        grpc:
    jaeger:
      protocols:
        thrift_http:
        grpc:

ingester:
  max_block_duration: 5m

compactor:
  compaction:
    block_retention: 720h # 30 days

storage:
  trace:
    backend: s3
    s3:
      bucket: tempo-traces
      endpoint: s3.amazonaws.com
      region: us-east-1
    wal:
      path: /var/tempo/wal

metrics_generator:
  registry:
    external_labels:
      source: tempo
      cluster: primary
  storage:
    path: /var/tempo/generator/wal
    remote_write:
      - url: http://mimir:9009/api/v1/push
        send_exemplars: true
```

## Production Best Practices

### Performance Optimization

#### Query Optimization

- Use label filters before line filters
- Limit time ranges for expensive queries
- Use `unwrap` instead of parsing when possible
- Cache query results with query frontend

#### Dashboard Performance

- Limit number of panels (< 15 per dashboard)
- Use appropriate time intervals
- Avoid high-cardinality grouping
- Use `$__interval` for adaptive sampling

#### Storage Optimization

- Configure retention policies
- Use compaction for Loki and Tempo
- Implement tiered storage (hot/warm/cold)
- Monitor storage growth

### Security Best Practices

#### Authentication

- Enable auth (`auth_enabled: true` in Loki/Tempo)
- Use OAuth/LDAP for Grafana
- Implement multi-tenancy with org isolation

#### Authorization

- Configure RBAC in Grafana
- Limit datasource access by team
- Use folder permissions for dashboards

#### Network Security

- TLS for all components
- Network policies in Kubernetes
- Rate limiting at ingress

### Troubleshooting

#### Common Issues

1. **High Cardinality**: Too many unique label combinations
   - Solution: Reduce label dimensions, use log parsing instead

2. **Query Timeouts**: Complex queries on large datasets
   - Solution: Reduce time range, use aggregations, add query limits

3. **Storage Growth**: Unbounded retention
   - Solution: Configure retention policies, enable compaction

4. **Missing Traces**: Incomplete trace data
   - Solution: Check sampling rates, verify instrumentation

## Expert Practices: Idioms, Anti-Patterns, Gotchas & Currency

High-signal, mechanism-level rules that separate correct LGTM work from queries that "run" but quietly return wrong or empty data. Many of these are *silent* bugs — no error, just wrong numbers or blank panels.

### Querying Idioms

**Use `$__rate_interval` (never `$__interval` or a fixed `[5m]`) with `rate()` and `increase()`.** These functions need at least two scrape samples in the range vector to compute a slope. `$__interval` is the display-resolution step and shrinks below the scrape interval when a user zooms in, producing empty/NaN "no data" gaps. `$__rate_interval = max($__interval + scrape_interval, 4 * scrape_interval)`, so it is always at least 4x the scrape interval — guaranteeing enough samples. Reserve `$__interval` for non-rate aggregations like `avg_over_time`. This is a silent correctness bug at narrow zoom.

```promql
sum(rate(http_requests_total{namespace=~"$namespace"}[$__rate_interval])) by (method, status)  # correct
sum(rate(http_requests_total{namespace=~"$namespace"}[$__interval])) by (method, status)        # empty at zoom
```

**Order the LogQL pipeline: stream selector → line filters → parsers → label filters.** Loki evaluates left-to-right, so ordering controls how many lines reach each expensive stage — a documented performance rule, not style. (1) The stream selector discards whole streams via the index; (2) simple string line filters `|=` / `!=` are cheapest (prefer them over regex `|~` / `!~`); (3) regex line filters; (4) parsers (`json`, `logfmt`, `pattern`, `regexp`) are most expensive. Placing a parser before a line filter forces parsing of lines the filter would have discarded for free.

```logql
{app="api"} |= "error" | json | level="error" | duration > 500   # correct: filter cheaply, then parse
{app="api"} | json | level="error" |= "error" | duration > 500   # wrong: parses every line first
```

**Use scoped TraceQL attributes (`span.` / `resource.`) and prefer `&&` for predicate pushdown.** Tempo stores traces in columnar Parquet. Unscoped attributes (`.http.status_code`) force Tempo to check multiple columns (span AND resource) — slower. Scoped attributes map to a single column. Queries built only from logical-and (`&&`) conditions push filtering down into the Parquet layer (predicate pushdown — the most efficient path); `||`, structural operators (`>>`, `<<`), and the like bypass it.

```traceql
{ span.http.status_code = 500 && resource.service.name = "api" && span.http.method = "POST" }  # fast
{ .http.status_code = 500 && .service.name = "api" }                                            # slow (unscoped)
```

**TraceQL metrics produce RED metrics directly from traces — no metrics generator.** Since Tempo 2.4+, append a metrics function (`rate`, `count_over_time`, `quantile_over_time`, `histogram_over_time`, `avg/min/max/sum_over_time`, `compare`, `topk`/`bottomk`) to a span filter, run via the Tempo data source query type `traceql_metrics`. This aggregates rate/error/duration by any span attribute without extra instrumentation, and enables retroactive dimensional slicing Prometheus cannot do (e.g. p99 of DB spans downstream of a service, by `db.system`). `quantile_over_time` replaces app-side histogram instruments for latency SLO panels. Avoid structural operators here — they require full trace processing and are slow over large datasets.

```traceql
{ span:name = "GET /:endpoint" } | quantile_over_time(duration, .99) by (span.http.target)  # p99 per endpoint
{ status = error } | rate() by (resource.service.name)                                       # error rate per service
```

**Order Grafana transformations: normalize → combine(join) → filter → reduce.** Transformations are a pipeline; each step feeds the next. The classic mistake is applying **Reduce** (collapses each series to one value) *before* a **Join** — that destroys the shared time field the join keys on, leaving nothing to match. Expert order: (1) extract/convert types; (2) combine (Outer join on Time, or Merge) into a wide table; (3) filter; (4) reduce last, only if a scalar is needed. Note **Merge** joins on ALL matching fields, so queries sharing more fields than intended multiply rows — use Outer join on Time explicitly. The transformation debug (bug) icon shows each step's actual output shape.

### Currency: deprecated / removed mechanisms (current as of Grafana 12 / Loki 3.0 / Tempo 2.4+)

**The Graph (old) panel and in-panel alert blocks are gone.** AngularJS support was removed in Grafana 12 (May 2025); `"type": "graph"` (with `yaxes[]` + root-level `thresholds`) is force-migrated to React `timeseries` on load, losing that config. Emit `"type": "timeseries"` with `fieldConfig.defaults`. The panel-level `"alert"` block was removed with legacy alerting in Grafana 11 — it is completely inert. Express alerts as Grafana Alerting rules under `/etc/grafana/provisioning/alerting/`.

**Prometheus native histograms are GA.** A native histogram stores variable-resolution buckets in a single series (~70-80% less storage, arbitrary quantile precision, no instrumentation-time bucket choices). `histogram_quantile()` works directly on the native metric — no `_bucket` suffix and no `by(le)`. OTel SDKs emit them when configured for exponential histograms. Caveat: not all panels render them (Time Series does not; use Histogram/Heatmap).

```promql
histogram_quantile(0.95, sum(rate(http_request_duration_seconds[$__rate_interval])))              # native
histogram_quantile(0.95, sum(rate(http_request_duration_seconds_bucket[$__rate_interval])) by (le)) # classic
```

**Grafana Agent is EOL (Nov 1 2025) — use Grafana Alloy.** Agent (Static, Flow, Operator) gets no more patches or support. Alloy is Grafana's OpenTelemetry Collector distribution (OTLP-compatible; metrics, logs, traces, profiles) using the same River/Alloy config language as Agent Flow. Migrate `grafana/agent` Helm charts, images, and manifests to `grafana/alloy`.

```alloy
loki.source.kubernetes "pods" {
  targets    = discovery.kubernetes.pods.targets
  forward_to = [loki.write.default.receiver]
}
loki.write "default" {
  endpoint { url = "http://loki:3100/loki/api/v1/push" }
}
```

### LogQL Gotchas (silent wrong/empty data)

**Parsers add an `__error__` label rather than dropping lines.** When `json`/`logfmt`/`pattern` fails on a line, the line is NOT filtered out — it gets a `__error__` label and still appears in results, inflating metric aggregations. Filter with `| __error__=""`. Also: parameterless `| json` silently skips array values (extract explicitly, e.g. `| json first_server="servers[0]"`); and an extracted label colliding with an existing stream label is suffixed `_extracted` (e.g. `level_extracted`) — override with `| label_format level=level_extracted`.

```logql
{app="api"} | json | __error__="" | level="error"   # excludes parse failures
{app="api"} | json | level="error"                  # silently includes failed-parse lines
```

**`unwrap` defaults to a raw float64 conversion — use `duration_seconds()` / `bytes()`.** A Go-duration string like `150ms` or a byte string like `5 MiB` fails raw conversion; instead of erroring, the line gets `__error__` and drops out, so `quantile_over_time` / `avg_over_time` silently compute over only the parseable subset. `duration_seconds()` (alias `duration()`) expects Go format (`1s`, `100ms`, `5m30s`), NOT a raw millisecond integer — a field holding `1000` (ms) must be unwrapped raw and divided.

```logql
quantile_over_time(0.95, {app="api"} | logfmt | unwrap duration_seconds(duration) [5m]) by (endpoint)  # duration="150ms"
quantile_over_time(0.95, {app="api"} | logfmt | unwrap latency_ms [5m]) by (endpoint) / 1000             # latency_ms=150
quantile_over_time(0.95, {app="api"} | logfmt | unwrap duration [5m]) by (endpoint)                      # silently drops "150ms"
```

**LogQL label-filter boolean precedence is left-to-right, NOT AND-before-OR.** Multiple predicates evaluate strictly left-to-right with no mathematical precedence: `| duration >= 20ms or method="GET" and size <= 20KB` parses as `((duration >= 20ms or method="GET") and size <= 20KB)`, not the AND-first reading. A silent correctness bug — always parenthesize multi-predicate filters.

```logql
{app="api"} | logfmt | (duration >= 20ms or method="GET") and size <= 20KB   # explicit, correct
{app="api"} | logfmt | duration >= 20ms or method="GET" and size <= 20KB      # left-to-right surprise
```

### TraceQL Gotcha

**Structural operators (`>>`, `<<`, `~`) return the RIGHT-side spans only; `span:duration` ≠ `trace:duration`.** `{ .service.name="gateway" } >> { status = error }` returns the error *descendant* spans, not the gateway parent — counterintuitive when you set out to "find the gateway call that errored." Separately, `span:duration` (or bare `duration`) is one span's `end - start`, while `trace:duration` is whole-request wall time (`max(end) - min(start)` across the trace) — and trace-level intrinsics are significantly more performant. Use `trace:duration` for end-to-end latency dashboards.

```traceql
{ .service.name = "gateway" } >> { status = error }            # result = the error descendant spans
{ trace:rootService = "gateway" && trace:duration > 1s }        # end-to-end slow traces, fast intrinsic
{ span:name = "GET /x" && duration > 1s }                       # measures ONE span, not the request
```

### Loki Cardinality Anti-Pattern

**Loki labels are streams — keep high-cardinality fields in structured metadata, not labels.** Loki indexes only labels; each unique label combination is a separate stream with its own chunks. High-cardinality labels (`trace_id`, `request_id`, `pod`, `user_id`) cause stream/index explosion and force thousands of tiny chunk flushes (write amplification, slow queries) — and cardinality multiplies. Loki 3.0 (schema v13 + TSDB) provides **structured metadata**: per-line key-value pairs that are NOT indexed but ARE queryable; the native OTLP endpoint maps non-default OTel attributes there automatically. Critical: structured metadata cannot go in the `{}` stream selector — it must be a pipeline filter (`| trace_id="..."`), so it does NOT get index pre-filtering. Keep dynamic labels to single-/low-tens cardinality.

```logql
{namespace="prod", app="api"} | trace_id="0242ac120002" | pod="myservice-abc-56789"   # low-card labels, rest filtered
{namespace="prod", app="api", trace_id="0242ac120002", pod="myservice-abc-56789"}      # stream explosion
```

### Alerting Practices

**Alert on user-visible symptoms (latency / errors / availability), not infrastructure causes.** Cause-based alerts (e.g. high CPU during a harmless batch job) produce false positives and fatigue; reliability metrics that impact users are better paging signals. Pair two controls to cut noise: a pending period (`for`) so a condition must hold before firing, and `keep_firing_for` to avoid rapid resolve-and-fire flapping on exit.

```yaml
expr: sum(rate(http_requests_total{status=~"5.."}[$__rate_interval])) / sum(rate(http_requests_total[$__rate_interval])) > 0.01
for: 5m
keep_firing_for: 2m   # symptom-based, stabilized on entry and exit
```

**NoData / Error create independent `DatasourceNoData` / `DatasourceError` instances that bypass existing silences.** When a rule enters NoData or Error (default "Set NoData state"), Grafana creates a SEPARATE synthetic instance with `alertname=DatasourceNoData` / `DatasourceError` plus `datasource_uid` and `rulename` labels. These have different labels from the parent, so existing silences, mute timings, and notification policies keyed to the parent do NOT apply — the #1 source of surprise pages. The four options are Set NoData (default), Set Alerting, Set Normal, Keep last state; the `for` pending period applies (Normal → Pending → NoData), and `grafana_state_reason` explains why state diverged. To suppress, silence/route on the synthetic labels (`alertname=DatasourceNoData` + `rulename`), or use `Keep last state` for transient datasource hiccups.

```yaml
# Silence/route must target the synthetic labels, e.g. alertname=DatasourceNoData AND rulename=my-alert-rule
noDataState: KeepLast   # for transient datasource issues
execErrState: Error
```

### Provisioning Gotcha

**Provisioned datasources need an explicit, stable `uid`.** Without one, Grafana generates a random UID at each startup. Dashboard JSON embeds that UID in every panel's `datasource` reference, so on a fresh deploy (or delete/recreate) the new random UID no longer matches and every panel shows "Datasource not found" — a silent failure at provision time (Grafana loads fine, panels are broken). Declare a stable, human-readable `uid`, and reference datasources in dashboards via the object form `{type, uid}` rather than the legacy name string (name refs break on rename).

```yaml
datasources:
  - name: Prometheus
    type: prometheus
    uid: prometheus-main # stable, pinned — dashboards reference this UID
    url: http://prometheus:9090
```

## Resources

- [Loki Documentation](https://grafana.com/docs/loki/latest/)
- [Tempo Documentation](https://grafana.com/docs/tempo/latest/)
- [Grafana Documentation](https://grafana.com/docs/grafana/latest/)
- [LogQL Cheat Sheet](https://grafana.com/docs/loki/latest/logql/)
- [TraceQL Guide](https://grafana.com/docs/tempo/latest/traceql/)
- [Grafana Operator](https://github.com/grafana-operator/grafana-operator)
