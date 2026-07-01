---
name: loom-logging-observability
description: Logging and observability patterns for production systems. Use for structured JSON logging with correlation IDs, distributed tracing (OpenTelemetry, Jaeger, Zipkin), metrics collection (Prometheus), log aggregation (ELK, Loki, Datadog), and alerting strategies.
triggers:
  - log
  - logging
  - logs
  - trace
  - tracing
  - traces
  - metrics
  - observability
  - OpenTelemetry
  - OTEL
  - Jaeger
  - Zipkin
  - structured logging
  - log level
  - debug
  - info
  - warn
  - error
  - fatal
  - correlation ID
  - span
  - spans
  - ELK
  - Elasticsearch
  - Loki
  - Datadog
  - Prometheus
  - Grafana
  - distributed tracing
  - log aggregation
  - alerting
  - monitoring
  - JSON logs
  - telemetry
  - RED method
  - USE method
  - tail sampling
  - exemplars
  - cardinality
---

# Logging and Observability

## Overview

Understand system behavior through the three pillars — logs, metrics, traces — correlated by shared IDs. This skill covers structured logging, OpenTelemetry tracing, Prometheus metrics, aggregation backends, and alerting, with emphasis on the cost/cardinality traps and sampling decisions that separate a working setup from an expensive broken one.

## Three Pillars — what each answers, and its cost model

| Pillar      | Answers                              | Cost driver                         | Use for                                        |
| ----------- | ------------------------------------ | ----------------------------------- | ---------------------------------------------- |
| **Metrics** | "Is it broken? how much?" (aggregate) | Label **cardinality** (# series)    | Dashboards, SLOs, alerting — always-on, cheap  |
| **Traces**  | "Where in the request path?" (causal) | Span volume → **sampling**          | Latency breakdown, cross-service dependency    |
| **Logs**    | "What exactly happened?" (event detail) | Volume + **indexing** strategy      | Forensics, audit, the specifics of one request |

Reach for metrics first (cheap, aggregate), traces to localize, logs for the detail. Link all three by `trace_id`/`correlation_id` so you can pivot: alert fires on a metric → jump to an exemplar trace → read that trace's logs.

## Structured Logging

Emit JSON, one object per event — never string-interpolated prose. Structured fields are queryable in any backend; `f"user {id} did {action}"` is not.

```python
import json, logging, sys
from datetime import datetime, timezone
from contextvars import ContextVar

correlation_id: ContextVar[str] = ContextVar("correlation_id", default="")
trace_id: ContextVar[str] = ContextVar("trace_id", default="")

class JsonFormatter(logging.Formatter):
    def format(self, r: logging.LogRecord) -> str:
        data = {
            "ts": datetime.now(timezone.utc).isoformat(),
            "level": r.levelname, "logger": r.name, "msg": r.getMessage(),
            "correlation_id": correlation_id.get(), "trace_id": trace_id.get(),
        }
        if r.exc_info:
            data["exception"] = self.formatException(r.exc_info)
        if hasattr(r, "fields"):
            data.update(r.fields)          # structured extras
        return json.dumps(data)

h = logging.StreamHandler(sys.stdout); h.setFormatter(JsonFormatter())
logging.getLogger().addHandler(h); logging.getLogger().setLevel(logging.INFO)

logging.getLogger(__name__).info("order processed",
    extra={"fields": {"order_id": order.id, "total": order.total}})
```

TypeScript: use a child-logger pattern so request context is bound once and inherited — `pino`/`winston` do this natively; prefer them over hand-rolling.

```typescript
// pino: bound context + fast JSON. child() inherits parent fields.
const log = pino();
const reqLog = log.child({ correlationId, traceId });
reqLog.info({ orderId, total }, "order processed");  // object first, message second
```

### Log levels

| Level | When | Example |
| ----- | ---- | ------- |
| TRACE | Fine-grained, off in prod | Loop iterations, var values |
| DEBUG | Diagnostics, off in prod | Function entry/exit, intermediate state |
| INFO  | Normal business events | Request done, job completed, user action |
| WARN  | Recoverable / degraded | Retry attempted, deprecated API, slow query |
| ERROR | Failure needing attention | Exception caught, operation failed |
| FATAL | Cannot continue | Startup config missing, data corruption |

### Logging discipline (the expensive mistakes)

- **Never log secrets/PII** — passwords, tokens, full card numbers, emails, request bodies. Redact at the formatter (allowlist fields), not by remembering at each call site. PII in logs is a compliance breach (GDPR/PCI) and log stores are rarely access-controlled like a DB.
- **Structured over interpolated** — attach IDs as fields, not baked into the message string, or you can't filter/aggregate by them.
- **Don't log in hot paths synchronously.** A blocking log write per iteration in a tight loop or per-row is a latency cliff. Use async/non-blocking appenders (`QueueHandler` in Python, pino's async transport) and log the summary, not each item.
- **Sample high-volume logs.** For chatty success paths, emit 1-in-N (keep 100% of WARN/ERROR). Reduces cost without losing the signal.
- **Consistent field names across services** — always `correlation_id`, never sometimes `request_id`. Cross-service queries depend on it.
- **Emit to stdout as JSON; let the platform collect it.** Don't manage log files/rotation inside the app in a containerized environment — the agent/sidecar (Promtail, Fluent Bit, Datadog agent) tails stdout.

## Distributed Tracing

**Use OpenTelemetry — the vendor-neutral standard (CNCF).** Don't hand-roll spans/tracers: the OTel SDK gives context propagation, batching, and OTLP export for free, and swaps backends (Jaeger, Tempo, Datadog, any OTLP endpoint) without code change. Maturity as of 2026: **tracing is stable/GA**; **metrics stable**; **logs stable spec** but SDK/collector logs support is newer than traces — verify your language SDK's status before relying on OTel logs vs a mature logging lib.

Architecture: **SDK in-process** (creates spans, propagates context) → **OTel Collector** (receive/process/export; the place to do batching, tail sampling, redaction, fan-out to backends). Run the Collector as a sidecar or gateway; keep exporters/sampling config there so apps stay backend-agnostic.

```python
from opentelemetry import trace
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.resources import Resource
from opentelemetry.sdk.trace.export import BatchSpanProcessor
from opentelemetry.exporter.otlp.proto.grpc.trace_exporter import OTLPSpanExporter
from opentelemetry.instrumentation.fastapi import FastAPIInstrumentor

provider = TracerProvider(resource=Resource.create({"service.name": "order-service"}))
provider.add_span_processor(BatchSpanProcessor(OTLPSpanExporter(endpoint="http://otel-collector:4317")))
trace.set_tracer_provider(provider)
tracer = trace.get_tracer(__name__)

FastAPIInstrumentor.instrument_app(app)   # auto-spans for incoming requests + context extraction

@app.get("/orders/{order_id}")
async def get_order(order_id: str):
    with tracer.start_as_current_span("fetch_order") as span:
        span.set_attribute("order.id", order_id)      # low-cardinality-ok on spans (unlike metrics labels)
        return await repo.get(order_id)
```

### Context propagation — W3C traceparent

A trace spans services only if context crosses the wire. The standard is the **W3C `traceparent` header**: `00-<32-hex trace-id>-<16-hex span-id>-<2-hex flags>` (flags bit 0 = sampled). OTel auto-instrumentation injects it on outgoing HTTP/gRPC and extracts it on incoming — so instrument BOTH the client and server side, or the trace breaks at the boundary and you get orphan traces. For non-HTTP hops (queues, Kafka), propagate `traceparent` as a message attribute manually. Legacy backends may use B3 (Zipkin) headers; configure the propagator to match, or run both.

### Sampling — head vs tail

You cannot afford 100% of spans at volume. Two strategies:

- **Head sampling**: decide at trace *start* (in the SDK), e.g. `ParentBased(TraceIdRatioBased(0.1))` keeps 10%. Cheap, no buffering. **Fatal limitation: the decision is made before the outcome is known**, so you cannot "keep all errors" — a 1% error might be dropped.
- **Tail sampling**: decide *after the whole trace finishes*, in the **Collector's `tailsamplingprocessor`** — keep 100% of error/slow traces + a sample of normal ones. Requires buffering every span of a trace until complete, so **all spans of one trace must reach the same Collector instance**: put a load-balancing exporter (routing by trace-id) in front of the tail-sampling collectors, or they'll each see partial traces and sample wrongly.

Rule of thumb: head-sample for cost control at the edge; add tail sampling in the Collector when you need "always keep errors/slow." Propagate the sampled flag so a downstream service doesn't independently drop spans of a kept trace.

### Correlation & exemplars

Put `trace_id` into every log line's structured fields (bind it in a middleware/context var) so a trace pivots to its logs. Link metrics → traces with **exemplars**: a sampled trace-id attached to a histogram bucket observation, letting Grafana jump from a latency spike on a graph to the exact slow trace. Prometheus needs `--enable-feature=exemplar-storage` and OpenMetrics exposition; the client library attaches the exemplar at `observe()` time.

## Metrics

**Use a real client (`prometheus_client`, OTel metrics) — don't hand-roll registries.** They handle concurrency, exposition format, and label management correctly.

```python
from prometheus_client import Counter, Gauge, Histogram, start_http_server

REQS = Counter("http_requests_total", "Requests", ["method", "route", "status"])
INFLIGHT = Gauge("http_inflight_requests", "In-flight requests")
LAT = Histogram("http_request_duration_seconds", "Latency", ["method", "route"],
                buckets=(.005, .01, .025, .05, .1, .25, .5, 1, 2.5, 5))

start_http_server(9090)                        # exposes /metrics for scrape

@INFLIGHT.track_inprogress()
def handle(req):
    with LAT.labels(req.method, req.route).time():   # times the block
        resp = process(req)
    REQS.labels(req.method, req.route, resp.status).inc()
```

- **Types**: Counter (monotonic totals — `_total`), Gauge (up/down current value — memory, queue depth), Histogram (bucketed distribution — latency; enables `histogram_quantile` percentiles server-side). Prefer Histogram over Summary for latency (Summary quantiles can't be aggregated across instances).
- **Naming (Prometheus)**: base unit suffix — `_seconds`, `_bytes`, `_total`. `http_request_duration_seconds`, not `_ms`.
- **⚠ Label cardinality is the #1 killer.** Each unique label-value combination is a **separate time series** stored in memory. Never use unbounded values as labels — user_id, request_id, email, full URL with IDs, raw error message. One high-card label can create millions of series and OOM Prometheus. Keep labels to bounded sets (method, route *template*, status class). Put the high-card identifier in a **trace or log**, not a metric label.

### Dashboards & SLOs — RED and USE

- **RED** (request-driven services): **R**ate (req/s), **E**rrors (failed req/s or ratio), **D**uration (latency distribution, alert on p95/p99). One RED panel set per service tells you if users are hurting.
- **USE** (resources: CPU, disk, pool, queue): **U**tilization (% busy), **S**aturation (queued/waiting work — the leading indicator), **E**rrors. Saturation usually predicts trouble before utilization saturates.
- Golden Signals (Google SRE) = latency, traffic, errors, saturation — RED + saturation. Alert on symptom signals (RED), use USE to diagnose the cause.

## Log Aggregation — and the indexing cost trap

**The single biggest cost/architecture decision: index everything vs index labels only.**

- **ELK / Elasticsearch / OpenSearch**: indexes *every field* → fast arbitrary full-text and field queries, but storage and compute scale with total log volume (expensive at scale; index bloat, hot-node pressure).
- **Grafana Loki**: indexes *only labels* (stream metadata), stores log content compressed and **unindexed** → cheap ingest/storage; content queries are brute-force scans over the time-and-label-narrowed set (`{app="api"} |= "timeout"`). Great when you filter by labels then grep; slow if you need ad-hoc full-text over everything.
- **Loki cardinality trap (same killer as metrics):** every unique label-value set is a separate *stream*. Putting `trace_id`/`user_id`/`pod_ip` as a Loki **label** explodes stream count and destroys performance. Keep labels low-cardinality (app, env, level, namespace); filter high-card values as *content* in the query, not as labels.
- **Datadog / hosted**: priced per ingested/indexed GB — use ingestion filters/exclusion rules (e.g. drop health-check logs) and index only what you'll query.

```yaml
# Promtail → Loki: LOW-cardinality labels only; do NOT add trace_id/user_id as labels
scrape_configs:
  - job_name: app
    static_configs: [{ targets: [localhost], labels: { job: app, __path__: /var/log/app/*.log } }]
    pipeline_stages:
      - json: { expressions: { level: level, service: service } }
      - labels: { level:, service: }        # bounded sets only
```

```yaml
# Datadog agent: exclude noise at ingest to control cost
logs_config:
  processing_rules:
    - type: exclude_at_match
      name: drop_healthchecks
      pattern: "GET /health"
```

## Alerting

```yaml
groups:
  - name: service
    rules:
      - alert: HighErrorRate                # symptom, not cause
        expr: sum(rate(http_requests_total{status=~"5.."}[5m])) / sum(rate(http_requests_total[5m])) > 0.05
        for: 5m
        labels: { severity: critical }
        annotations:
          summary: "Error rate {{ $value | humanizePercentage }}"
          runbook_url: "https://wiki/runbooks/high-error-rate"     # every alert links a runbook
      - alert: HighLatencyP95
        expr: histogram_quantile(0.95, sum(rate(http_request_duration_seconds_bucket[5m])) by (le)) > 1
        for: 10m
        labels: { severity: warning }
      - alert: ServiceDown
        expr: up == 0
        for: 1m
        labels: { severity: critical }
```

| Severity | Response | Examples |
| -------- | -------- | -------- |
| Critical | Page now | Service down, error-rate SLO breach, data loss |
| Warning  | Business hrs | Rising latency, approaching limits, retry spikes |
| Info     | Log only | Deploy started, config changed |

Principles:

- **Alert on symptoms (user impact), not causes.** Page on error rate/latency (RED); CPU high with healthy latency is not an incident. Symptoms say *what's broken*; use USE/traces to find *why*.
- **Every alert is actionable and has a runbook.** Non-actionable alerts cause fatigue → ignored pages. Delete or downgrade alerts nobody acts on.
- **Thresholds from SLOs, not vibes.** Prefer **SLO burn-rate alerts** (fast burn = page, slow burn = ticket) over static thresholds — they alert proportional to error-budget consumption and cut false pages during low traffic.
- **`for:` avoids flapping** — require the condition to hold before firing.

## Request middleware (correlation + span + metrics in one place)

```python
class ObservabilityMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request, call_next):
        corr = request.headers.get("X-Correlation-ID", str(uuid.uuid4()))
        correlation_id.set(corr)
        span = trace.get_current_span()               # created by FastAPIInstrumentor
        span.set_attribute("correlation_id", corr)
        trace_id.set(format(span.get_span_context().trace_id, "032x"))  # into logs
        start = time.perf_counter()
        try:
            resp = await call_next(request)
            LAT.labels(request.method, request.url.path).observe(time.perf_counter() - start)
            REQS.labels(request.method, request.url.path, str(resp.status_code)).inc()
            resp.headers["X-Correlation-ID"] = corr
            return resp
        except Exception as e:
            span.record_exception(e); span.set_attribute("error", True)
            raise
```

Note: use `url.path` *template* (route pattern), not the raw path with IDs, as the metric label — else cardinality explodes.

## Verification Checklist

- [ ] Logs are JSON to stdout with consistent field names; `correlation_id` + `trace_id` on every line
- [ ] No secrets/PII in logs; redaction is enforced at the formatter, not per-call-site
- [ ] Hot-path logging is async/sampled; success paths sampled, 100% of WARN/ERROR kept
- [ ] Tracing uses the OTel SDK; both client and server sides instrumented so `traceparent` propagates (no orphan traces)
- [ ] Sampling chosen deliberately: head for cost, tail (in Collector, with trace-id load balancing) if "always keep errors"
- [ ] Metric labels are bounded — no user_id/request_id/raw-path/error-string as labels
- [ ] Latency uses Histogram with sensible buckets; percentiles computed via `histogram_quantile`
- [ ] Dashboards follow RED (services) / USE (resources); metric names carry base-unit suffixes
- [ ] Loki labels are low-cardinality (no trace_id/user_id as labels); backend indexing cost understood (Loki labels-only vs ELK index-all)
- [ ] Alerts fire on symptoms, have `for:`, link a runbook, and derive thresholds from SLOs (prefer burn-rate)
- [ ] Logs, metrics, and traces are cross-linkable by shared IDs; exemplars wired if metric→trace pivot is needed
