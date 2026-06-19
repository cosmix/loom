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

## Overview

Prometheus is a powerful open-source monitoring and alerting system designed for reliability and scalability in cloud-native environments. Built for multi-dimensional time-series data with flexible querying via PromQL.

### Architecture Components

- **Prometheus Server**: Core component that scrapes and stores time-series data with local TSDB
- **Alertmanager**: Handles alerts, deduplication, grouping, routing, and notifications to receivers
- **Pushgateway**: Allows ephemeral jobs to push metrics (use sparingly - prefer pull model)
- **Exporters**: Convert metrics from third-party systems to Prometheus format (node, blackbox, etc.)
- **Client Libraries**: Instrument application code (Go, Java, Python, Rust, etc.)
- **Prometheus Operator**: Kubernetes-native deployment and management via CRDs
- **Remote Storage**: Long-term storage via Thanos, Cortex, Mimir for multi-cluster federation

### Data Model

- **Metrics**: Time-series data identified by metric name and key-value labels
- **Format**: `metric_name{label1="value1", label2="value2"} sample_value timestamp`
- **Metric Types**:
  - **Counter**: Monotonically increasing value (requests, errors) - use `rate()` or `increase()` for querying
  - **Gauge**: Value that can go up/down (temperature, memory usage, queue length)
  - **Histogram**: Observations in configurable buckets (latency, request size) - exposes `_bucket`, `_sum`, `_count`
  - **Summary**: Similar to histogram but calculates quantiles client-side - use histograms for aggregation

## Setup and Configuration

### Basic Prometheus Server Configuration

```yaml
# prometheus.yml
global:
  scrape_interval: 15s
  scrape_timeout: 10s
  evaluation_interval: 15s
  external_labels:
    cluster: "production"
    region: "us-east-1"

# Alertmanager configuration
alerting:
  alertmanagers:
    - static_configs:
        - targets:
            - alertmanager:9093

# Load rules files
rule_files:
  - "alerts/*.yml"
  - "rules/*.yml"

# Scrape configurations
scrape_configs:
  # Prometheus itself
  - job_name: "prometheus"
    static_configs:
      - targets: ["localhost:9090"]

  # Application services
  - job_name: "application"
    metrics_path: "/metrics"
    static_configs:
      - targets:
          - "app-1:8080"
          - "app-2:8080"
        labels:
          env: "production"
          team: "backend"

  # Kubernetes service discovery
  - job_name: "kubernetes-pods"
    kubernetes_sd_configs:
      - role: pod
    relabel_configs:
      # Only scrape pods with prometheus.io/scrape annotation
      - source_labels: [__meta_kubernetes_pod_annotation_prometheus_io_scrape]
        action: keep
        regex: true
      # Use custom metrics path if specified
      - source_labels: [__meta_kubernetes_pod_annotation_prometheus_io_path]
        action: replace
        target_label: __metrics_path__
        regex: (.+)
      # Use custom port if specified
      - source_labels:
          [__address__, __meta_kubernetes_pod_annotation_prometheus_io_port]
        action: replace
        regex: ([^:]+)(?::\d+)?;(\d+)
        replacement: $1:$2
        target_label: __address__
      # Add namespace label
      - source_labels: [__meta_kubernetes_namespace]
        action: replace
        target_label: kubernetes_namespace
      # Add pod name label
      - source_labels: [__meta_kubernetes_pod_name]
        action: replace
        target_label: kubernetes_pod_name
      # Add service name label
      - source_labels: [__meta_kubernetes_pod_label_app]
        action: replace
        target_label: app

  # Node Exporter for host metrics
  - job_name: "node-exporter"
    static_configs:
      - targets:
          - "node-exporter:9100"
```

### Alertmanager Configuration

```yaml
# alertmanager.yml
global:
  resolve_timeout: 5m
  slack_api_url: "https://hooks.slack.com/services/YOUR/WEBHOOK/URL"
  pagerduty_url: "https://events.pagerduty.com/v2/enqueue"

# Template files for custom notifications
templates:
  - "/etc/alertmanager/templates/*.tmpl"

# Route alerts to appropriate receivers
route:
  group_by: ["alertname", "cluster", "service"]
  group_wait: 10s
  group_interval: 10s
  repeat_interval: 12h
  receiver: "default"

  routes:
    # matchers/source_matchers/target_matchers (Alertmanager 0.27+, UTF-8 aware) replace
    # the DEPRECATED match/match_re/source_match/target_match fields, which are slated for
    # removal once UTF-8 strict mode becomes the default.
    # Critical alerts go to PagerDuty
    - matchers:
        - severity = critical
      receiver: "pagerduty"
      continue: true

    # Database alerts to DBA team
    - matchers:
        - team = database
      receiver: "dba-team"
      group_by: ["alertname", "instance"]

    # Development environment alerts
    - matchers:
        - env = development
      receiver: "slack-dev"
      group_wait: 5m
      repeat_interval: 4h

# Inhibition rules (suppress alerts)
inhibit_rules:
  # Suppress warning alerts if critical alert is firing
  - source_matchers:
      - severity = critical
    target_matchers:
      - severity = warning
    equal: ["alertname", "instance"]

  # Suppress instance alerts if entire service is down
  - source_matchers:
      - alertname = ServiceDown
    target_matchers:
      - alertname =~ ".*"
    equal: ["service"]

receivers:
  - name: "default"
    slack_configs:
      - channel: "#alerts"
        title: "Alert: {{ .GroupLabels.alertname }}"
        text: "{{ range .Alerts }}{{ .Annotations.description }}{{ end }}"

  - name: "pagerduty"
    pagerduty_configs:
      # routing_key = Events API v2 (current, richer payload: dedup keys, links, images).
      # service_key = legacy Events API v1; the two are mutually exclusive. The *_file
      # variants read the secret from a mounted file instead of embedding plaintext YAML.
      - routing_key_file: /etc/alertmanager/secrets/pagerduty-routing-key
        description: "{{ .GroupLabels.alertname }}"

  - name: "dba-team"
    slack_configs:
      - channel: "#database-alerts"
    email_configs:
      - to: "dba-team@example.com"
        headers:
          Subject: "Database Alert: {{ .GroupLabels.alertname }}"

  - name: "slack-dev"
    slack_configs:
      - channel: "#dev-alerts"
        send_resolved: true
```

## Best Practices

### Metric Naming Conventions

Follow these naming patterns for consistency:

```text
# Format: <namespace>_<subsystem>_<metric>_<unit>

# Counters (always use _total suffix)
http_requests_total
http_request_errors_total
cache_hits_total

# Gauges
memory_usage_bytes
active_connections
queue_size

# Histograms (use _bucket, _sum, _count suffixes automatically)
http_request_duration_seconds
response_size_bytes
db_query_duration_seconds

# Use consistent base units
- seconds for duration (not milliseconds)
- bytes for size (not kilobytes)
- ratio for percentages (0.0-1.0, not 0-100)
```

### Label Cardinality Management

#### DO

```yaml
# Good: Bounded cardinality
http_requests_total{method="GET", status="200", endpoint="/api/users"}

# Good: Reasonable number of label values
db_queries_total{table="users", operation="select"}
```

#### DON'T

```yaml
# Bad: Unbounded cardinality (user IDs, email addresses, timestamps)
http_requests_total{user_id="12345"}
http_requests_total{email="user@example.com"}
http_requests_total{timestamp="1234567890"}

# Bad: High cardinality (full URLs, IP addresses)
http_requests_total{url="/api/users/12345/profile"}
http_requests_total{client_ip="192.168.1.100"}
```

#### Guidelines

- Keep label values to < 10 per label (ideally)
- Total unique time-series per metric should be < 10,000
- Use recording rules to pre-aggregate high-cardinality metrics
- Avoid labels with unbounded values (IDs, timestamps, user input)

### Recording Rules for Performance

Use recording rules to pre-compute expensive queries. Prefer `without (instance)` over
`by (job)`: `without` names only the label being aggregated away and automatically
preserves `job` and any future labels, whereas `by` silently drops labels added to the
metric later. Keep `le` explicitly where `histogram_quantile()` consumes the rule.

```yaml
# rules/recording_rules.yml
groups:
  - name: performance_rules
    interval: 30s
    rules:
      # Pre-calculate request rates (without preserves job + future labels)
      - record: instance_removed:http_requests:rate5m
        expr: sum without (instance) (rate(http_requests_total[5m]))

      # Pre-calculate error rates
      - record: instance_removed:http_request_errors:rate5m
        expr: sum without (instance) (rate(http_request_errors_total[5m]))

      # Pre-calculate error ratio: aggregate numerator and denominator SEPARATELY,
      # then divide. NEVER avg()/sum() this ratio rule downstream - averaging a ratio
      # (or an average of an average) is statistically invalid; re-aggregate the
      # underlying counts instead.
      - record: job:http_request_error_ratio:rate5m
        expr: |
          sum without (instance) (rate(http_requests_total{status=~"5.."}[5m]))
          /
          sum without (instance) (rate(http_requests_total[5m]))

      # Pre-aggregate latency percentiles (le kept for histogram_quantile)
      - record: job:http_request_duration_seconds:p95
        expr: histogram_quantile(0.95, sum without (instance) (rate(http_request_duration_seconds_bucket[5m])))

      - record: job:http_request_duration_seconds:p99
        expr: histogram_quantile(0.99, sum without (instance) (rate(http_request_duration_seconds_bucket[5m])))

  - name: aggregation_rules
    interval: 1m
    rules:
      # Multi-level aggregation for dashboards
      - record: instance:node_cpu_utilization:ratio
        expr: |
          1 - avg(rate(node_cpu_seconds_total{mode="idle"}[5m])) by (instance)

      - record: cluster:node_cpu_utilization:ratio
        expr: avg(instance:node_cpu_utilization:ratio)

      # Memory aggregation
      - record: instance:node_memory_utilization:ratio
        expr: |
          1 - (
            node_memory_MemAvailable_bytes
            /
            node_memory_MemTotal_bytes
          )
```

### Alert Design (Symptoms vs Causes)

#### Alert on symptoms (user-facing impact), not causes

```yaml
# alerts/symptom_based.yml
groups:
  - name: symptom_alerts
    rules:
      # GOOD: Alert on user-facing symptoms
      - alert: HighErrorRate
        expr: |
          (
            sum(rate(http_requests_total{status=~"5.."}[5m]))
            /
            sum(rate(http_requests_total[5m]))
          ) > 0.05
        for: 5m
        labels:
          severity: critical
          team: backend
        annotations:
          summary: "High error rate detected"
          description: "Error rate is {{ $value | humanizePercentage }} (threshold: 5%)"
          runbook: "https://wiki.example.com/runbooks/high-error-rate"

      - alert: HighLatency
        expr: |
          histogram_quantile(0.95,
            sum(rate(http_request_duration_seconds_bucket[5m])) by (le, service)
          ) > 1
        for: 5m
        labels:
          severity: warning
          team: backend
        annotations:
          summary: "High latency on {{ $labels.service }}"
          description: "P95 latency is {{ $value }}s (threshold: 1s)"
          impact: "Users experiencing slow page loads"

      # GOOD: multi-window multi-burn-rate SLO alerting (Google SRE Workbook).
      # NO for: clause - duration does not scale with severity and resets on data gaps.
      # Each tier ANDs a long detection window with a short (~1/12) confirmation window;
      # tiers are ORed. Numerator/denominator are aggregated separately, then divided.
      - alert: SLOBudgetBurnRateFast
        expr: |
          (
            sum(rate(http_requests_total{status=~"5.."}[1h]))
            / sum(rate(http_requests_total[1h])) > (14.4 * 0.001)
            and
            sum(rate(http_requests_total{status=~"5.."}[5m]))
            / sum(rate(http_requests_total[5m])) > (14.4 * 0.001)
          )
        labels:
          severity: critical  # page: ~2% of monthly budget burned in 1h
          team: sre
        annotations:
          summary: "SLO budget burning fast (14.4x)"
          description: "Error budget for the 99.9% SLO is burning at 14.4x over 1h"

      - alert: SLOBudgetBurnRateSlow
        expr: |
          (
            sum(rate(http_requests_total{status=~"5.."}[6h]))
            / sum(rate(http_requests_total[6h])) > (6 * 0.001)
            and
            sum(rate(http_requests_total{status=~"5.."}[30m]))
            / sum(rate(http_requests_total[30m])) > (6 * 0.001)
          )
        labels:
          severity: warning  # ticket: gradual burn over hours
          team: sre
        annotations:
          summary: "SLO budget burning steadily (6x)"
          description: "Error budget for the 99.9% SLO is burning at 6x over 6h"
```

#### Cause-based alerts (use for debugging, not paging)

```yaml
# alerts/cause_based.yml
groups:
  - name: infrastructure_alerts
    rules:
      # Lower severity for infrastructure issues
      - alert: HighMemoryUsage
        expr: |
          (
            node_memory_MemTotal_bytes - node_memory_MemAvailable_bytes
          ) / node_memory_MemTotal_bytes > 0.9
        for: 10m
        labels:
          severity: warning # Not critical unless symptoms appear
          team: infrastructure
        annotations:
          summary: "High memory usage on {{ $labels.instance }}"
          description: "Memory usage is {{ $value | humanizePercentage }}"

      - alert: DiskSpaceLow
        expr: |
          (
            node_filesystem_avail_bytes{mountpoint="/"}
            /
            node_filesystem_size_bytes{mountpoint="/"}
          ) < 0.1
        for: 5m
        labels:
          severity: warning
          team: infrastructure
        annotations:
          summary: "Low disk space on {{ $labels.instance }}"
          description: "Only {{ $value | humanizePercentage }} disk space remaining"
          action: "Clean up logs or expand disk"
```

### Alert Best Practices

1. **For duration**: Use `for` clause to avoid flapping
2. **Meaningful annotations**: Include summary, description, runbook URL, impact
3. **Proper severity levels**: critical (page immediately), warning (ticket), info (log)
4. **Actionable alerts**: Every alert should require human action
5. **Include context**: Add labels for team ownership, service, environment

## PromQL Query Patterns

PromQL is the query language for Prometheus. Key concepts: instant vectors, range vectors, scalar, string literals, selectors, operators, functions, and aggregation.

### Selectors and Matchers

```promql
# Instant vector selector (latest sample for each time-series)
http_requests_total

# Filter by label values
http_requests_total{method="GET", status="200"}

# Regex matching (=~) and negative regex (!~)
http_requests_total{status=~"5.."}  # 5xx errors
http_requests_total{endpoint!~"/admin.*"}  # exclude admin endpoints

# Label absence/presence
http_requests_total{job="api", status=""}  # empty label
http_requests_total{job="api", status!=""}  # non-empty label

# Range vector selector (samples over time)
http_requests_total[5m]  # last 5 minutes of samples
```

### Rate Calculations

```promql
# Request rate (requests per second) - ALWAYS use rate() for counters
rate(http_requests_total[5m])

# Sum by service
sum(rate(http_requests_total[5m])) by (service)

# Approximate increase over the window - extrapolated, returns a float (e.g. 99.7),
# NOT an exact integer count. Use for trend visualization, not exact counting or integer thresholds.
increase(http_requests_total[1h])

# irate() - ONLY for high-resolution dashboard graphs of fast-moving counters.
# NEVER use in alert rules: it reads just the last 2 samples, so a brief dip resets
# the for: timer and a sustained breach may never fire. Use rate() in all alerts.
irate(http_requests_total[5m])
```

### Error Ratios

```promql
# Error rate ratio
sum(rate(http_requests_total{status=~"5.."}[5m]))
/
sum(rate(http_requests_total[5m]))

# Success rate
sum(rate(http_requests_total{status=~"2.."}[5m]))
/
sum(rate(http_requests_total[5m]))
```

### Histogram Queries

```promql
# P95 latency
histogram_quantile(0.95,
  sum(rate(http_request_duration_seconds_bucket[5m])) by (le)
)

# P50, P95, P99 latency by service
histogram_quantile(0.50, sum(rate(http_request_duration_seconds_bucket[5m])) by (le, service))
histogram_quantile(0.95, sum(rate(http_request_duration_seconds_bucket[5m])) by (le, service))
histogram_quantile(0.99, sum(rate(http_request_duration_seconds_bucket[5m])) by (le, service))

# Average request duration
sum(rate(http_request_duration_seconds_sum[5m])) by (service)
/
sum(rate(http_request_duration_seconds_count[5m])) by (service)
```

### Aggregation Operations

```promql
# Sum across all instances
sum(node_memory_MemTotal_bytes) by (cluster)

# Average CPU usage
avg(rate(node_cpu_seconds_total{mode="idle"}[5m])) by (instance)

# Maximum value
max(http_request_duration_seconds) by (service)

# Minimum value
min(node_filesystem_avail_bytes) by (instance)

# Count number of instances
count(up == 1) by (job)

# Standard deviation
stddev(http_request_duration_seconds) by (service)
```

### Advanced Queries

```promql
# Top 5 services by request rate
topk(5, sum(rate(http_requests_total[5m])) by (service))

# Bottom 3 instances by available memory
bottomk(3, node_memory_MemAvailable_bytes)

# Predict disk full time (linear regression)
predict_linear(node_filesystem_avail_bytes{mountpoint="/"}[1h], 4 * 3600) < 0

# Compare with 1 day ago
http_requests_total - http_requests_total offset 1d

# Rate of change (derivative)
deriv(node_memory_MemAvailable_bytes[5m])

# Absent metric detection
absent(up{job="critical-service"})
```

### Complex Aggregations

```promql
# Calculate Apdex score (Application Performance Index)
(
  sum(rate(http_request_duration_seconds_bucket{le="0.1"}[5m]))
  +
  sum(rate(http_request_duration_seconds_bucket{le="0.5"}[5m])) * 0.5
)
/
sum(rate(http_request_duration_seconds_count[5m]))

# Multi-window multi-burn-rate SLO (99.9% target): fast-burn tier OR slow-burn tier.
# Each tier ANDs a long window with a short (~1/12) confirmation window. No for: clause.
(
  sum(rate(http_requests_total{status=~"5.."}[1h])) / sum(rate(http_requests_total[1h])) > 0.001 * 14.4
  and
  sum(rate(http_requests_total{status=~"5.."}[5m])) / sum(rate(http_requests_total[5m])) > 0.001 * 14.4
)
or
(
  sum(rate(http_requests_total{status=~"5.."}[6h])) / sum(rate(http_requests_total[6h])) > 0.001 * 6
  and
  sum(rate(http_requests_total{status=~"5.."}[30m])) / sum(rate(http_requests_total[30m])) > 0.001 * 6
)
```

### Binary Operators and Vector Matching

```promql
# Arithmetic operators (+, -, *, /, %, ^)
node_memory_MemTotal_bytes - node_memory_MemAvailable_bytes

# Comparison operators (==, !=, >, <, >=, <=) - filter to matching values
http_request_duration_seconds > 1

# Logical operators (and, or, unless)
up{job="api"} and rate(http_requests_total[5m]) > 100

# One-to-one matching (default)
method:http_requests:rate5m / method:http_requests:total

# Many-to-one matching with group_left
sum(rate(http_requests_total[5m])) by (instance, method)
  / on(instance) group_left
sum(rate(http_requests_total[5m])) by (instance)

# One-to-many matching with group_right
sum(rate(http_requests_total[5m])) by (instance)
  / on(instance) group_right
sum(rate(http_requests_total[5m])) by (instance, method)
```

### Time Functions and Offsets

```promql
# Compare with previous time period
rate(http_requests_total[5m]) / rate(http_requests_total[5m] offset 1h)

# Day-over-day comparison
http_requests_total - http_requests_total offset 1d

# Time-based filtering
http_requests_total and hour() >= 9 and hour() < 17  # business hours
day_of_week() == 0 or day_of_week() == 6  # weekends

# Timestamp functions
time() - process_start_time_seconds  # uptime in seconds
```

## Service Discovery

Prometheus supports multiple service discovery mechanisms for dynamic environments where targets appear and disappear.

### Static Configuration

```yaml
scrape_configs:
  - job_name: "static-targets"
    static_configs:
      - targets:
          - "host1:9100"
          - "host2:9100"
        labels:
          env: production
          region: us-east-1
```

### File-based Service Discovery

```yaml
scrape_configs:
  - job_name: 'file-sd'
    file_sd_configs:
      - files:
          - '/etc/prometheus/targets/*.json'
          - '/etc/prometheus/targets/*.yml'
        refresh_interval: 30s

# targets/webservers.json
[
  {
    "targets": ["web1:8080", "web2:8080"],
    "labels": {
      "job": "web",
      "env": "prod"
    }
  }
]
```

### Kubernetes Service Discovery

```yaml
scrape_configs:
  # Pod-based discovery
  - job_name: "kubernetes-pods"
    kubernetes_sd_configs:
      - role: pod
        namespaces:
          names:
            - production
            - staging
    relabel_configs:
      # Keep only pods with prometheus.io/scrape=true annotation
      - source_labels: [__meta_kubernetes_pod_annotation_prometheus_io_scrape]
        action: keep
        regex: true

      # Extract custom scrape path from annotation
      - source_labels: [__meta_kubernetes_pod_annotation_prometheus_io_path]
        action: replace
        target_label: __metrics_path__
        regex: (.+)

      # Extract custom port from annotation
      - source_labels:
          [__address__, __meta_kubernetes_pod_annotation_prometheus_io_port]
        action: replace
        regex: ([^:]+)(?::\d+)?;(\d+)
        replacement: $1:$2
        target_label: __address__

      # Add standard Kubernetes labels
      - action: labelmap
        regex: __meta_kubernetes_pod_label_(.+)
      - source_labels: [__meta_kubernetes_namespace]
        target_label: kubernetes_namespace
      - source_labels: [__meta_kubernetes_pod_name]
        target_label: kubernetes_pod_name

  # Service-based discovery
  - job_name: "kubernetes-services"
    kubernetes_sd_configs:
      - role: service
    relabel_configs:
      - source_labels:
          [__meta_kubernetes_service_annotation_prometheus_io_scrape]
        action: keep
        regex: true
      - source_labels:
          [__meta_kubernetes_service_annotation_prometheus_io_scheme]
        action: replace
        target_label: __scheme__
        regex: (https?)
      - source_labels: [__meta_kubernetes_service_annotation_prometheus_io_path]
        action: replace
        target_label: __metrics_path__
        regex: (.+)

  # Node-based discovery (for node exporters)
  - job_name: "kubernetes-nodes"
    kubernetes_sd_configs:
      - role: node
    relabel_configs:
      - action: labelmap
        regex: __meta_kubernetes_node_label_(.+)
      - target_label: __address__
        replacement: kubernetes.default.svc:443
      - source_labels: [__meta_kubernetes_node_name]
        regex: (.+)
        target_label: __metrics_path__
        replacement: /api/v1/nodes/${1}/proxy/metrics

  # Endpoints discovery (for service endpoints)
  - job_name: "kubernetes-endpoints"
    kubernetes_sd_configs:
      - role: endpoints
    relabel_configs:
      - source_labels:
          [__meta_kubernetes_service_annotation_prometheus_io_scrape]
        action: keep
        regex: true
      - source_labels: [__meta_kubernetes_endpoint_port_name]
        action: keep
        regex: metrics
```

### Consul Service Discovery

```yaml
scrape_configs:
  - job_name: "consul-services"
    consul_sd_configs:
      - server: "consul.example.com:8500"
        datacenter: "dc1"
        services: ["web", "api", "cache"]
        tags: ["production"]
    relabel_configs:
      - source_labels: [__meta_consul_service]
        target_label: service
      - source_labels: [__meta_consul_tags]
        target_label: tags
```

### EC2 Service Discovery

```yaml
scrape_configs:
  - job_name: "ec2-instances"
    ec2_sd_configs:
      - region: us-east-1
        access_key: YOUR_ACCESS_KEY
        secret_key: YOUR_SECRET_KEY
        port: 9100
        filters:
          - name: tag:Environment
            values: [production]
          - name: instance-state-name
            values: [running]
    relabel_configs:
      - source_labels: [__meta_ec2_tag_Name]
        target_label: instance_name
      - source_labels: [__meta_ec2_availability_zone]
        target_label: availability_zone
      - source_labels: [__meta_ec2_instance_type]
        target_label: instance_type
```

### DNS Service Discovery

```yaml
scrape_configs:
  - job_name: "dns-srv-records"
    dns_sd_configs:
      - names:
          - "_prometheus._tcp.example.com"
        type: "SRV"
        refresh_interval: 30s
    relabel_configs:
      - source_labels: [__meta_dns_name]
        target_label: instance
```

### Relabeling Actions Reference

| Action      | Description                                        | Use Case                           |
| ----------- | -------------------------------------------------- | ---------------------------------- |
| `keep`      | Keep targets where regex matches source labels     | Filter targets by annotation/label |
| `drop`      | Drop targets where regex matches source labels     | Exclude specific targets           |
| `replace`   | Replace target label with value from source labels | Extract custom labels/paths/ports  |
| `labelmap`  | Map source label names to target labels via regex  | Copy all Kubernetes labels         |
| `labeldrop` | Drop labels matching regex                         | Remove internal metadata labels    |
| `labelkeep` | Keep only labels matching regex                    | Reduce cardinality                 |
| `hashmod`   | Set target label to hash of source labels modulo N | Sharding/routing                   |

## High Availability and Scalability

### Prometheus High Availability Setup

```yaml
# Deploy multiple identical Prometheus instances scraping same targets
# Use external labels to distinguish instances
global:
  external_labels:
    replica: prometheus-1 # Change to prometheus-2, etc.
    cluster: production

# Alertmanager will deduplicate alerts from multiple Prometheus instances
alerting:
  alertmanagers:
    - static_configs:
        - targets:
            - alertmanager-1:9093
            - alertmanager-2:9093
            - alertmanager-3:9093
```

### Alertmanager Clustering

```yaml
# alertmanager.yml - HA cluster configuration
global:
  resolve_timeout: 5m

route:
  receiver: "default"
  group_by: ["alertname", "cluster"]
  group_wait: 10s
  group_interval: 10s
  repeat_interval: 12h

receivers:
  - name: "default"
    slack_configs:
      - api_url: "https://hooks.slack.com/services/YOUR/WEBHOOK"
        channel: "#alerts"

# Start Alertmanager cluster members
# alertmanager-1: --cluster.peer=alertmanager-2:9094 --cluster.peer=alertmanager-3:9094
# alertmanager-2: --cluster.peer=alertmanager-1:9094 --cluster.peer=alertmanager-3:9094
# alertmanager-3: --cluster.peer=alertmanager-1:9094 --cluster.peer=alertmanager-2:9094
```

### Federation for Hierarchical Monitoring

```yaml
# Global Prometheus federating from regional instances
scrape_configs:
  - job_name: "federate"
    scrape_interval: 15s
    honor_labels: true
    metrics_path: "/federate"
    params:
      "match[]":
        # Pull aggregated metrics only
        - '{job="prometheus"}'
        - '{__name__=~"job:.*"}' # Recording rules
        - "up"
    static_configs:
      - targets:
          - "prometheus-us-east-1:9090"
          - "prometheus-us-west-2:9090"
          - "prometheus-eu-west-1:9090"
        labels:
          region: "us-east-1"
```

### Remote Storage for Long-term Retention

```yaml
# Prometheus remote write to Thanos/Cortex/Mimir
remote_write:
  - url: "http://thanos-receive:19291/api/v1/receive"
    queue_config:
      capacity: 10000
      max_shards: 50
      min_shards: 1
      max_samples_per_send: 5000
      batch_send_deadline: 5s
      min_backoff: 30ms
      max_backoff: 100ms
    write_relabel_configs:
      # Drop high-cardinality metrics before remote write
      - source_labels: [__name__]
        regex: "go_.*"
        action: drop

# Prometheus remote read from long-term storage
remote_read:
  - url: "http://thanos-query:9090/api/v1/read"
    read_recent: true
```

### Thanos Architecture for Global View

```yaml
# Thanos Sidecar - runs alongside Prometheus
thanos sidecar \
  --prometheus.url=http://localhost:9090 \
  --tsdb.path=/prometheus \
  --objstore.config-file=/etc/thanos/bucket.yml \
  --grpc-address=0.0.0.0:10901 \
  --http-address=0.0.0.0:10902

# Thanos Store - queries object storage
thanos store \
  --data-dir=/var/thanos/store \
  --objstore.config-file=/etc/thanos/bucket.yml \
  --grpc-address=0.0.0.0:10901 \
  --http-address=0.0.0.0:10902

# Thanos Query - global query interface
thanos query \
  --http-address=0.0.0.0:9090 \
  --grpc-address=0.0.0.0:10901 \
  --store=prometheus-1-sidecar:10901 \
  --store=prometheus-2-sidecar:10901 \
  --store=thanos-store:10901

# Thanos Compactor - downsample and compact blocks
thanos compact \
  --data-dir=/var/thanos/compact \
  --objstore.config-file=/etc/thanos/bucket.yml \
  --retention.resolution-raw=30d \
  --retention.resolution-5m=90d \
  --retention.resolution-1h=365d
```

### Horizontal Sharding with Hashmod

```yaml
# Split scrape targets across multiple Prometheus instances using hashmod
scrape_configs:
  - job_name: "kubernetes-pods-shard-0"
    kubernetes_sd_configs:
      - role: pod
    relabel_configs:
      # Hash pod name and keep only shard 0 (mod 3)
      - source_labels: [__meta_kubernetes_pod_name]
        modulus: 3
        target_label: __tmp_hash
        action: hashmod
      - source_labels: [__tmp_hash]
        regex: "0"
        action: keep

  - job_name: "kubernetes-pods-shard-1"
    kubernetes_sd_configs:
      - role: pod
    relabel_configs:
      - source_labels: [__meta_kubernetes_pod_name]
        modulus: 3
        target_label: __tmp_hash
        action: hashmod
      - source_labels: [__tmp_hash]
        regex: "1"
        action: keep

  # shard-2 similar pattern...
```

## Kubernetes Integration

### ServiceMonitor for Prometheus Operator

```yaml
# servicemonitor.yaml
apiVersion: monitoring.coreos.com/v1
kind: ServiceMonitor
metadata:
  name: app-metrics
  namespace: monitoring
  labels:
    app: myapp
    release: prometheus
spec:
  # Select services to monitor
  selector:
    matchLabels:
      app: myapp

  # Define namespaces to search
  namespaceSelector:
    matchNames:
      - production
      - staging

  # Endpoint configuration
  endpoints:
    - port: metrics # Service port name
      path: /metrics
      interval: 30s
      scrapeTimeout: 10s

      # Relabeling
      relabelings:
        - sourceLabels: [__meta_kubernetes_pod_name]
          targetLabel: pod
        - sourceLabels: [__meta_kubernetes_namespace]
          targetLabel: namespace

      # Metric relabeling (filter/modify metrics)
      metricRelabelings:
        - sourceLabels: [__name__]
          regex: "go_.*"
          action: drop # Drop Go runtime metrics
        - sourceLabels: [status]
          regex: "[45].."
          targetLabel: error
          replacement: "true"

  # Optional: TLS configuration
  # tlsConfig:
  #   insecureSkipVerify: true
  #   ca:
  #     secret:
  #       name: prometheus-tls
  #       key: ca.crt
```

### PodMonitor for Direct Pod Scraping

```yaml
# podmonitor.yaml
apiVersion: monitoring.coreos.com/v1
kind: PodMonitor
metadata:
  name: app-pods
  namespace: monitoring
  labels:
    release: prometheus
spec:
  # Select pods to monitor
  selector:
    matchLabels:
      app: myapp

  # Namespace selection
  namespaceSelector:
    matchNames:
      - production

  # Pod metrics endpoints
  podMetricsEndpoints:
    - port: metrics
      path: /metrics
      interval: 15s

      # Relabeling
      relabelings:
        - sourceLabels: [__meta_kubernetes_pod_label_version]
          targetLabel: version
        - sourceLabels: [__meta_kubernetes_pod_node_name]
          targetLabel: node
```

### PrometheusRule for Alerts and Recording Rules

```yaml
# prometheusrule.yaml
apiVersion: monitoring.coreos.com/v1
kind: PrometheusRule
metadata:
  name: app-rules
  namespace: monitoring
  labels:
    release: prometheus
    role: alert-rules
spec:
  groups:
    - name: app_alerts
      interval: 30s
      rules:
        - alert: HighErrorRate
          expr: |
            (
              sum(rate(http_requests_total{status=~"5..", app="myapp"}[5m]))
              /
              sum(rate(http_requests_total{app="myapp"}[5m]))
            ) > 0.05
          for: 5m
          labels:
            severity: critical
            team: backend
          annotations:
            summary: "High error rate on {{ $labels.namespace }}/{{ $labels.pod }}"
            description: "Error rate is {{ $value | humanizePercentage }}"
            dashboard: "https://grafana.example.com/d/app-overview"
            runbook: "https://wiki.example.com/runbooks/high-error-rate"

        - alert: PodCrashLooping
          # Use a floor over the window so normal single pod recycles do not page.
          expr: |
            increase(kube_pod_container_status_restarts_total[15m]) > 3
          for: 5m
          labels:
            severity: warning
          annotations:
            summary: "Pod {{ $labels.namespace }}/{{ $labels.pod }} is crash looping"
            description: "Container {{ $labels.container }} has restarted {{ $value }} times in 15m"

    - name: app_recording_rules
      interval: 30s
      rules:
        - record: app:http_requests:rate5m
          expr: sum(rate(http_requests_total{app="myapp"}[5m])) by (namespace, pod, method, status)

        - record: app:http_request_duration_seconds:p95
          expr: |
            histogram_quantile(0.95,
              sum(rate(http_request_duration_seconds_bucket{app="myapp"}[5m])) by (le, namespace, pod)
            )
```

### Prometheus Custom Resource

```yaml
# prometheus.yaml
apiVersion: monitoring.coreos.com/v1
kind: Prometheus
metadata:
  name: prometheus
  namespace: monitoring
spec:
  replicas: 2
  # Pin a current 3.x release. Upgrading 2.x -> 3.0 requires reading the migration guide:
  # scrape Content-Type is now strict (set fallback_scrape_protocol for non-compliant
  # exporters) and le/quantile labels are normalized (le="1" becomes le="1.0", so
  # expressions matching integer le/quantile values must be updated to the float form).

  # Service account for Kubernetes API access
  serviceAccountName: prometheus

  # Select ServiceMonitors
  serviceMonitorSelector:
    matchLabels:
      release: prometheus

  # Select PodMonitors
  podMonitorSelector:
    matchLabels:
      release: prometheus

  # Select PrometheusRules
  ruleSelector:
    matchLabels:
      release: prometheus
      role: alert-rules

  # Resource limits
  resources:
    requests:
      memory: 2Gi
      cpu: 1000m
    limits:
      memory: 4Gi
      cpu: 2000m

  # Storage
  storage:
    volumeClaimTemplate:
      spec:
        accessModes:
          - ReadWriteOnce
        resources:
          requests:
            storage: 50Gi
        storageClassName: fast-ssd

  # Retention
  retention: 30d
  retentionSize: 45GB

  # Alertmanager configuration
  alerting:
    alertmanagers:
      - namespace: monitoring
        name: alertmanager
        port: web

  # External labels
  externalLabels:
    cluster: production
    region: us-east-1

  # Security context
  securityContext:
    fsGroup: 2000
    runAsNonRoot: true
    runAsUser: 1000

  # Enable admin API for management operations
  enableAdminAPI: false

  # Additional scrape configs (from Secret)
  additionalScrapeConfigs:
    name: additional-scrape-configs
    key: prometheus-additional.yaml
```

## Application Instrumentation Examples

### Go Application

```go
// main.go
package main

import (
    "net/http"
    "time"

    "github.com/prometheus/client_golang/prometheus"
    "github.com/prometheus/client_golang/prometheus/promauto"
    "github.com/prometheus/client_golang/prometheus/promhttp"
)

var (
    // Counter for total requests
    httpRequestsTotal = promauto.NewCounterVec(
        prometheus.CounterOpts{
            Name: "http_requests_total",
            Help: "Total number of HTTP requests",
        },
        []string{"method", "endpoint", "status"},
    )

    // Histogram for request duration
    httpRequestDuration = promauto.NewHistogramVec(
        prometheus.HistogramOpts{
            Name:    "http_request_duration_seconds",
            Help:    "HTTP request duration in seconds",
            Buckets: []float64{.001, .005, .01, .025, .05, .1, .25, .5, 1, 2.5, 5, 10},
        },
        []string{"method", "endpoint"},
    )

    // Gauge for active connections
    activeConnections = promauto.NewGauge(
        prometheus.GaugeOpts{
            Name: "active_connections",
            Help: "Number of active connections",
        },
    )

    // Histogram for response sizes. Use a HistogramVec (not a SummaryVec) for anything
    // aggregated across instances: summary quantiles are pre-computed per process and are
    // NOT aggregatable. NativeHistogramBucketFactor enables a native histogram (no upfront
    // bucket guessing). Reserve summaries for single-process quantiles that are never aggregated.
    responseSizeBytes = promauto.NewHistogramVec(
        prometheus.HistogramOpts{
            Name:                        "http_response_size_bytes",
            Help:                        "HTTP response size in bytes",
            NativeHistogramBucketFactor: 1.1,
            Buckets:                     prometheus.ExponentialBuckets(64, 4, 8), // classic fallback
        },
        []string{"endpoint"},
    )
)

// Middleware to instrument HTTP handlers
func instrumentHandler(endpoint string, handler http.HandlerFunc) http.HandlerFunc {
    return func(w http.ResponseWriter, r *http.Request) {
        start := time.Now()
        activeConnections.Inc()
        defer activeConnections.Dec()

        // Wrap response writer to capture status code
        wrapped := &responseWriter{ResponseWriter: w, statusCode: 200}

        handler(wrapped, r)

        duration := time.Since(start).Seconds()
        httpRequestDuration.WithLabelValues(r.Method, endpoint).Observe(duration)
        httpRequestsTotal.WithLabelValues(r.Method, endpoint,
            http.StatusText(wrapped.statusCode)).Inc()
    }
}

type responseWriter struct {
    http.ResponseWriter
    statusCode int
}

func (rw *responseWriter) WriteHeader(code int) {
    rw.statusCode = code
    rw.ResponseWriter.WriteHeader(code)
}

func handleUsers(w http.ResponseWriter, r *http.Request) {
    w.Header().Set("Content-Type", "application/json")
    w.Write([]byte(`{"users": []}`))
}

func main() {
    // Register handlers
    http.HandleFunc("/api/users", instrumentHandler("/api/users", handleUsers))
    http.Handle("/metrics", promhttp.Handler())

    // Start server
    http.ListenAndServe(":8080", nil)
}
```

### Python Application (Flask)

```python
# app.py
from flask import Flask, request
from prometheus_client import Counter, Histogram, Gauge, generate_latest
import time

app = Flask(__name__)

# Define metrics
request_count = Counter(
    'http_requests_total',
    'Total HTTP requests',
    ['method', 'endpoint', 'status']
)

request_duration = Histogram(
    'http_request_duration_seconds',
    'HTTP request duration in seconds',
    ['method', 'endpoint'],
    buckets=[.001, .005, .01, .025, .05, .1, .25, .5, 1, 2.5, 5, 10]
)

active_requests = Gauge(
    'active_requests',
    'Number of active requests'
)

# Middleware for instrumentation
@app.before_request
def before_request():
    active_requests.inc()
    request.start_time = time.time()

@app.after_request
def after_request(response):
    active_requests.dec()

    duration = time.time() - request.start_time
    request_duration.labels(
        method=request.method,
        endpoint=request.endpoint or 'unknown'
    ).observe(duration)

    request_count.labels(
        method=request.method,
        endpoint=request.endpoint or 'unknown',
        status=response.status_code
    ).inc()

    return response

@app.route('/metrics')
def metrics():
    return generate_latest()

@app.route('/api/users')
def users():
    return {'users': []}

if __name__ == '__main__':
    app.run(host='0.0.0.0', port=8080)
```

## Production Deployment Checklist

- [ ] Set appropriate retention period (balance storage vs history needs)
- [ ] Configure persistent storage with adequate size
- [ ] Enable high availability (multiple Prometheus replicas or federation)
- [ ] Set up remote storage for long-term retention (Thanos, Cortex, Mimir)
- [ ] Configure service discovery for dynamic environments
- [ ] Implement recording rules for frequently-used queries
- [ ] Create symptom-based alerts with proper annotations
- [ ] Set up Alertmanager with appropriate routing and receivers
- [ ] Configure inhibition rules to reduce alert noise
- [ ] Add runbook URLs to all critical alerts
- [ ] Implement proper label hygiene (avoid high cardinality)
- [ ] Monitor Prometheus itself (meta-monitoring)
- [ ] Set up authentication and authorization
- [ ] Enable TLS for scrape targets and remote storage
- [ ] Configure rate limiting for queries
- [ ] Test alert and recording rule validity (`promtool check rules`)
- [ ] Implement backup and disaster recovery procedures
- [ ] Document metric naming conventions for the team
- [ ] Create dashboards in Grafana for common queries
- [ ] Set up log aggregation alongside metrics (Loki)

## Troubleshooting Commands

```bash
# Check Prometheus configuration syntax
promtool check config prometheus.yml

# Check rules file syntax
promtool check rules alerts/*.yml

# Test PromQL queries
promtool query instant http://localhost:9090 'up'

# Check which targets are up
curl http://localhost:9090/api/v1/targets

# Query current metric values
curl 'http://localhost:9090/api/v1/query?query=up'

# Check service discovery
curl http://localhost:9090/api/v1/targets/metadata

# View TSDB stats
curl http://localhost:9090/api/v1/status/tsdb

# Check runtime information
curl http://localhost:9090/api/v1/status/runtimeinfo
```

## Quick Reference

### Common PromQL Patterns

```promql
# Request rate per second
rate(http_requests_total[5m])

# Error ratio percentage
100 * sum(rate(http_requests_total{status=~"5.."}[5m])) / sum(rate(http_requests_total[5m]))

# P95 latency from histogram
histogram_quantile(0.95, sum(rate(http_request_duration_seconds_bucket[5m])) by (le))

# Average latency from histogram
sum(rate(http_request_duration_seconds_sum[5m])) / sum(rate(http_request_duration_seconds_count[5m]))

# Memory utilization percentage
100 * (1 - node_memory_MemAvailable_bytes / node_memory_MemTotal_bytes)

# CPU utilization (non-idle)
100 * (1 - avg(rate(node_cpu_seconds_total{mode="idle"}[5m])))

# Disk space remaining percentage
100 * node_filesystem_avail_bytes / node_filesystem_size_bytes

# Top 5 endpoints by request rate
topk(5, sum(rate(http_requests_total[5m])) by (endpoint))

# Service uptime in days
(time() - process_start_time_seconds) / 86400

# Request rate growth compared to 1 hour ago
rate(http_requests_total[5m]) / rate(http_requests_total[5m] offset 1h)
```

### Alert Rule Patterns

```yaml
# High error rate (symptom-based)
alert: HighErrorRate
expr: |
  sum(rate(http_requests_total{status=~"5.."}[5m]))
  / sum(rate(http_requests_total[5m])) > 0.05
for: 5m
labels:
  severity: critical
annotations:
  summary: "Error rate is {{ $value | humanizePercentage }}"
  runbook: "https://runbooks.example.com/high-error-rate"

# High latency P95
alert: HighLatency
expr: |
  histogram_quantile(0.95,
    sum(rate(http_request_duration_seconds_bucket[5m])) by (le, service)
  ) > 1
for: 5m
labels:
  severity: warning

# Service down
alert: ServiceDown
expr: up{job="critical-service"} == 0
for: 2m
labels:
  severity: critical

# Disk space low (cause-based, warning only)
alert: DiskSpaceLow
expr: |
  node_filesystem_avail_bytes{mountpoint="/"}
  / node_filesystem_size_bytes{mountpoint="/"} < 0.1
for: 10m
labels:
  severity: warning

# Pod crash looping (floor avoids paging on normal single restarts)
alert: PodCrashLooping
expr: increase(kube_pod_container_status_restarts_total[15m]) > 3
for: 5m
labels:
  severity: warning
```

### Recording Rule Naming Convention

```yaml
# Format: level:metric:operations
# level = aggregation level (job, instance, cluster)
# metric = base metric name
# operations = transformations applied (rate5m, sum, ratio)

groups:
  - name: aggregation_rules
    rules:
      # Instance-level aggregation
      - record: instance:node_cpu_utilization:ratio
        expr: 1 - avg(rate(node_cpu_seconds_total{mode="idle"}[5m])) by (instance)

      # Job-level aggregation (without preserves job + any future labels)
      - record: instance_removed:http_requests:rate5m
        expr: sum without (instance) (rate(http_requests_total[5m]))

      # Job-level error ratio: numerator/denominator aggregated separately, then divided
      - record: job:http_request_errors:ratio
        expr: |
          sum without (instance) (rate(http_requests_total{status=~"5.."}[5m]))
          / sum without (instance) (rate(http_requests_total[5m]))

      # Cluster-level aggregation
      - record: cluster:cpu_utilization:ratio
        expr: avg(instance:node_cpu_utilization:ratio)
```

### Metric Naming Best Practices

| Pattern          | Good Example                    | Bad Example                            |
| ---------------- | ------------------------------- | -------------------------------------- |
| Counter suffix   | `http_requests_total`           | `http_requests`                        |
| Base units       | `http_request_duration_seconds` | `http_request_duration_ms`             |
| Ratio range      | `cache_hit_ratio` (0.0-1.0)     | `cache_hit_percentage` (0-100)         |
| Byte units       | `response_size_bytes`           | `response_size_kb`                     |
| Namespace prefix | `myapp_http_requests_total`     | `http_requests_total`                  |
| Label naming     | `{method="GET", status="200"}`  | `{httpMethod="GET", statusCode="200"}` |

### Label Cardinality Guidelines

| Cardinality     | Examples                                 | Recommendation          |
| --------------- | ---------------------------------------- | ----------------------- |
| Low (<10)       | HTTP method, status code, environment    | Safe for all labels     |
| Medium (10-100) | API endpoint, service name, pod name     | Safe with aggregation   |
| High (100-1000) | Container ID, hostname                   | Use only when necessary |
| Unbounded       | User ID, IP address, timestamp, URL path | Never use as label      |

### Kubernetes Annotation-based Scraping

```yaml
# Pod annotations for automatic Prometheus scraping
apiVersion: v1
kind: Pod
metadata:
  annotations:
    prometheus.io/scrape: "true"
    prometheus.io/port: "8080"
    prometheus.io/path: "/metrics"
    prometheus.io/scheme: "http"
spec:
  containers:
    - name: app
      image: myapp:latest
      ports:
        - containerPort: 8080
          name: metrics
```

### Alertmanager Routing Patterns

```yaml
route:
  receiver: default
  group_by: ["alertname", "cluster"]
  routes:
    # Critical alerts to PagerDuty
    - match:
        severity: critical
      receiver: pagerduty
      continue: true # Also send to default

    # Team-based routing
    - match:
        team: database
      receiver: dba-team
      group_by: ["alertname", "instance"]

    # Environment-based routing
    - match:
        env: development
      receiver: slack-dev
      repeat_interval: 4h

    # Time-based routing (office hours only)
    - match:
        severity: warning
      receiver: email
      active_time_intervals:
        - business-hours

time_intervals:
  - name: business-hours
    time_intervals:
      - times:
          - start_time: "09:00"
            end_time: "17:00"
        weekdays: ["monday:friday"]
```

## Expert Practices: Idioms, Anti-Patterns & Gotchas

High-signal practices from the official docs, the Google SRE Book/Workbook, and recognized
experts. Each entry states the mechanism (the *why*), not just the rule.

### Anti-Patterns (statistically or operationally wrong)

**Always `rate()` before `sum()`, never `sum()` before `rate()`.** Counters reset to 0 on
process restart. If you sum counters first and one target restarts, the aggregate drops and
`rate()` reads that drop as a reset, emitting a huge spurious spike. Applying `rate()`
per-series handles each reset in isolation before aggregation. This is a mathematical
requirement, not a performance one, and applies to all counter arithmetic: compute
`rate(a[5m]) + rate(b[5m])`, never `rate(a[5m] + b[5m])`. The only functions safe on a raw
counter are `rate()`, `irate()`, `increase()`, and `resets()`.

```promql
# Good                                    # Bad (reset of one target spikes the whole sum)
sum by (job) (rate(http_requests_total[5m]))   # rate(sum by (job) (http_requests_total)[5m])
```

**Never use `irate()` in alerting rules.** `irate()` uses only the two most recent samples,
making it maximally volatile. In an alert with `for:`, a single brief dip below the threshold
moves the alert out of pending and resets the timer to zero, so a genuinely sustained breach
may never fire. The official functions docs say verbatim: *"Use rate for alerts and slow-moving
counters, as brief changes in the rate can reset the FOR clause."* Reserve `irate()` for
high-resolution dashboard graphs only.

**Never aggregate a ratio.** Averaging or summing pre-computed ratios is invalid (Jensen's
inequality / average-of-averages). If instance A serves 1000 req at 10% errors and B serves 10
req at 90%, `avg(10%, 90%) = 50%` but the true combined rate is ~18%. Aggregate the numerator
and denominator separately, then divide. This applies to recording-rule chains too: downstream
consumers must re-aggregate the underlying counts, never `avg()`/`sum()` a ratio rule.

**Summaries cannot be aggregated across instances — use histograms for cross-instance
percentiles.** Summaries pre-compute quantiles client-side per process; those values are not
additive, so `avg(...{quantile="0.95"})` across pods is statistically meaningless (the docs
call it "statistically nonsensical"). Histograms store raw per-bucket counts, which *are*
additive: sum buckets across any dimension, then compute the quantile. Any metric that may be
aggregated across instances/pods/jobs must be a histogram. Use summaries only for single-process
quantiles that are never aggregated.

```promql
# Good: aggregate bucket counts first, then estimate
histogram_quantile(0.95, sum by (le) (rate(http_request_duration_seconds_bucket[5m])))
# Bad: averaging pre-computed summary quantiles
avg(http_request_duration_seconds{quantile="0.95"})
```

**Pushgateway never expires metrics and has no `up` signal.** The docs: *"The Pushgateway never
forgets series pushed to it."* A batch job that succeeded Monday and never ran Tuesday still
serves Monday's success metric, so monitoring looks healthy when it is not, and you lose the
automatic `up` health signal. Push a `*_last_success_timestamp_seconds` gauge and alert on
staleness, and/or DELETE the group via the Pushgateway API on completion. Use the Pushgateway
only for genuinely ephemeral service-level batch jobs, never long-running services.

```yaml
- alert: BatchJobStale
  expr: time() - batch_job_last_success_timestamp_seconds > 3600
  for: 5m
```

### Design Patterns

**Multi-window multi-burn-rate SLO alerts (no `for:`).** The Google SRE Workbook advises against
a `for:`/duration clause in SLO alerts: duration does not scale with severity (a total outage
and a 0.1% degradation wait the same fixed time) and a metric gap resets the timer. Instead each
tier ANDs a long detection window with a short confirmation window sized ~1/12 of the long one
(the short window proves the budget is burning *now*, giving fast reset on recovery); tiers are
ORed. For a 99.9% SLO the Workbook tiers are: page at 14.4x over 1h AND 5m; page at 6x over 6h
AND 30m; ticket at 1x over 3d AND 6h. (See the corrected `SLOBudgetBurnRate*` alerts above.)

**Page on the Four Golden Signals at the user-facing boundary, not on component causes.** The
golden signals (latency, traffic, errors, saturation) say *what* to monitor; the expert nuance
is *where* to page — at the outermost user-visible boundary. In a layered system one layer's
symptom is another's cause (slow DB queries are the API's latency symptom). Vet every page: is it
urgent and user-visible; will you ever ignore it; is action required; is that action
non-automatable; is someone else already paged? Conditions failing these (pool near-full, high
memory) are warnings/tickets, not criticals.

```yaml
# Good: user-facing symptom            # Bad: cause-based page without confirmed user impact
alert: HighUserFacingErrorRate         # alert: DatabaseConnectionPoolExhausted
expr: |                                #   expr: db_connection_pool_used / db_connection_pool_max > 0.9
  sum(rate(http_requests_total{status=~"5..", job="frontend"}[5m]))
  / sum(rate(http_requests_total{job="frontend"}[5m])) > 0.05
```

### Idioms

**Prefer `without` over `by` in recording/aggregation rules.** `by (l1, l2)` forces you to
enumerate every label to keep; any label added later silently vanishes from the result. The docs:
*"Always specify a without clause with the labels you are aggregating away. This is to preserve
all the other labels such as job."* `without (instance)` future-proofs rules and preserves the
labels Alertmanager routes on. (Applied throughout the recording-rule sections above.)

**Prefer native histograms for new instrumentation — now a stable feature.** The docs: *"If you
can, use native histograms and prefer them over both classic histograms and summaries."* They use
dynamic exponential bucket schemas (no upfront bucket guessing), store all buckets+sum+count in
ONE composite series (vs N+2 classic `_bucket` series), and are inherently cross-instance
aggregatable. They were experimental through Prometheus 3.0 and became **stable** in the 3.x
line; the old `--enable-feature=native-histograms` server flag is a no-op as of v3.9. Enable
scraping via the scrape-config option, not the old flag; convert classic histograms on ingest
without re-instrumenting. Client side, set `NativeHistogramBucketFactor` (e.g. 1.1 ≈ ~10%
resolution).

```yaml
scrape_configs:
  - job_name: "myapp"
    scrape_native_histograms: true
    convert_classic_histograms_to_nhcb: true   # convert existing classic histograms to NHCB
    static_configs:
      - targets: ["myapp:8080"]
```

**Add resolution hysteresis with `keep_firing_for` (Prometheus 2.42+).** `for:` delays *firing*;
`keep_firing_for:` delays *resolution* — it keeps the alert firing for the given duration after
the condition was last met. Without it, an alert deactivates on the first evaluation where the
condition is not met, so a value oscillating around the threshold (or a brief data gap) produces
firing/resolved/firing flapping. `keep_firing_for` stops this without weakening the firing
threshold.

```yaml
- alert: HighErrorRate
  expr: sum(rate(http_requests_total{status=~"5.."}[5m])) / sum(rate(http_requests_total[5m])) > 0.05
  for: 5m
  keep_firing_for: 10m   # stays firing 10m after the condition clears
```

### Gotchas (silent failures)

**Keep `le` when aggregating classic histograms.** `histogram_quantile()` reconstructs buckets
from the `le` label. If `sum()` drops `le` (not listed in `by()`, or stripped via labeldrop),
all buckets collapse and the function returns NaN or a meaningless value with NO error — one of
the most common silent histogram failures. Always include `le` in the `by()` of any aggregation
feeding `histogram_quantile()`. Two more NaN sources: the highest bucket must have `le="+Inf"`
(hand-built exporters sometimes omit it); and a window with zero observations divides by zero.
Aggregate only across instances with identical bucket boundaries.

**`rate()` range window should be ≥ 4x the scrape interval.** `rate()` needs ≥2 samples in the
window. At a 15s scrape interval, `[15s]`/`[20s]` holds only ~2 samples under ideal timing, so
jitter or one missed scrape yields empty results and gappy graphs/alerts. Rule of thumb:
`window >= 4 * scrape_interval` (1m minimum at 15s; 2m at 30s); 5m is a common smoothing choice.
Grafana's `$__rate_interval` encodes this. Standardize on one window rather than maintaining
`rate1m`/`rate5m`/`rate1h` variants; use `avg_over_time()` when a different window is needed.

**`increase()` extrapolates and returns fractional values — never use it for exact counts.**
`increase(v[d])` is `rate(v[d]) * d`, which extrapolates to the window edges, so
`increase(errors_total[1h])` can read 99.7 or 100.2 even for integer-only counters. This makes
integer thresholds (`> 100`) fire/miss off-by-one. Use it for approximate "total over window"
visualization, never for exact event counting, auditing, billing, or precise thresholds.

**`absent()` only fires on total absence.** `absent(v)` returns 1 only when the selector matches
zero series, so it cannot detect that ONE instance stopped exporting while others still do. It
also can't be stabilized with `for:` (it only returns a vector while missing, so the timer resets
when the series reappears). For a per-target missing metric, join `up`; for flaky scrapes, use
`absent_over_time()` so it fires only after the series has been gone the whole window.

```promql
# Per-instance missing metric on a healthy target
up{job="foo"} == 1 unless on(instance) my_metric{job="foo"}
# Transient-resilient total absence
absent_over_time(up{job="critical"}[5m])
```

**Bare `sum()` drops all labels and breaks Alertmanager routing.** `sum()` with no
`by()`/`without()` collapses to a single label-less series, so an alert built on it carries no
`job`/`service`/`team`/`env` labels and Alertmanager routes/groupings matching on them silently
fail. Prefer `sum without (instance) (...)` to keep everything routable, or `sum by (job, ...)`
listing exactly the labels Alertmanager routes/groups on.

**Metrics with explicit timestamps bypass staleness markers and linger ~5 minutes.** Prometheus
inserts staleness markers when a series stops being scraped — but this is disabled for metrics
that embed their own timestamps in the exposition format (cAdvisor, some OTel collectors). Such a
vanished series keeps its last value for up to `query.lookback-delta` (5m default) and still looks
live to queries and alert evaluation. Enable proper handling with `track_timestamps_staleness:
true` in the scrape config (off by default).

### Security & Currency

**`honor_labels: true` on the Pushgateway lets anyone write any time series.** The security model:
*"As the Pushgateway is usually scraped with honor_labels enabled, this means anyone with access
to the Pushgateway can create any time series in Prometheus."* `honor_labels` lets the target's
own labels (including `job`/`instance`) override the server-assigned ones, so any client that can
POST can impersonate any service and inject arbitrary series. Treat the Pushgateway (and any
`honor_labels: true` federation of untrusted Prometheis) as a privileged write endpoint and
restrict network access accordingly.

**Prometheus 3.0 upgrade breaking changes.** (1) **Strict Content-Type:** where 2.x silently fell
back to the text 0.0.4 format, 3.0 fails the scrape on a missing/invalid Content-Type — set
`fallback_scrape_protocol: PrometheusText0.0.4` per job for non-compliant exporters. (2) **Label
normalization:** the `le` of classic histograms and the `quantile` of summaries are normalized on
ingest (`le="1"` becomes `le="1.0"`), so expressions matching integer values silently stop
matching and must use the float form. Read the official migration guide before upgrading.

## Additional Resources

- [Prometheus Documentation](https://prometheus.io/docs/)
- [PromQL Basics](https://prometheus.io/docs/prometheus/latest/querying/basics/)
- [Best Practices](https://prometheus.io/docs/practices/)
- [Alerting Rules](https://prometheus.io/docs/prometheus/latest/configuration/alerting_rules/)
- [Recording Rules](https://prometheus.io/docs/prometheus/latest/configuration/recording_rules/)
- [Prometheus Operator](https://github.com/prometheus-operator/prometheus-operator)
- [Thanos Documentation](https://thanos.io/tip/thanos/getting-started.md/)
- [Google SRE Book - Monitoring](https://sre.google/sre-book/monitoring-distributed-systems/)
