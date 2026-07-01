---
name: loom-performance-testing
description: Performance and load testing with k6, locust, JMeter, Gatling, and artillery. Use for load/stress/spike/soak tests, API and database benchmarking, profiling, p95/p99 latency analysis, throughput measurement, and bottleneck identification.
triggers:
  - performance testing
  - load testing
  - stress testing
  - stress test
  - load test
  - performance test
  - k6
  - locust
  - JMeter
  - Gatling
  - artillery
  - benchmark
  - benchmarking
  - profiling
  - latency
  - throughput
  - RPS
  - requests per second
  - concurrent users
  - virtual users
  - percentile
  - p95
  - p99
  - p50
  - median latency
  - saturation
  - bottleneck
  - coordinated omission
  - performance budget
  - API load testing
  - database performance
  - query optimization
  - slow queries
  - scalability testing
  - capacity planning
  - response time
  - error rate
  - apdex
---

# Performance Testing

## Overview

Validate that a system meets latency, throughput, and stability targets under load, and locate the bottleneck when it doesn't. This file assumes you can write the test code; it focuses on the measurement traps that make load-test numbers lie and the decision criteria for tool/shape selection.

## Core Concepts (read first — these are where numbers go wrong)

### Open vs. closed workload models

The single most consequential choice. It determines what your numbers mean.

| Model      | Load driver                          | Throughput is…           | Overload behavior                                | Use for                          |
| ---------- | ------------------------------------ | ------------------------ | ------------------------------------------------ | -------------------------------- |
| **Closed** | Fixed VUs, each loops request→wait   | *Emergent* (backpressure) | Self-throttles: slow server → fewer requests sent | Modeling a fixed client pool     |
| **Open**   | Fixed *arrival rate* (req/s)         | *Controlled* (you set it) | Queue grows unbounded; latency explodes           | Web traffic, finding breaking pt |

⚠ **Closed models hide overload.** With fixed VUs, when the server slows down each VU sends *fewer* requests, so offered load silently drops. You can't overwhelm the server past what its own latency allows — you measure a moving target, not capacity. Real internet traffic is open (users arrive independently of server health), so **use an arrival-rate executor to find true breaking points.**

- k6: closed = `constant-vus`/`ramping-vus`; open = `constant-arrival-rate`/`ramping-arrival-rate`.
- Locust is closed-model (users); JMeter thread groups are closed; Gatling `injectOpen`/`injectClosed`; artillery `arrivalRate` is open.
- Open executors need `preAllocatedVUs`/`maxVUs` headroom; if k6 warns "insufficient VUs," it *dropped* iterations and your rate was never achieved.

### Coordinated omission — why naive latency numbers lie

A closed-model generator that waits for each response before sending the next **stops the clock during a stall.** One 10s stall that should have blocked 10,000 scheduled 1ms requests gets recorded as *one* 10s sample instead of 10,000 samples ≥ their overdue time. Result: p99 looks great while users are timing out.

- **Detect:** suspiciously clean tail (p99 ≈ p50) under load that clearly stuttered; throughput dips that don't show in latency percentiles.
- **Fix:** measure against an *intended schedule*, not send-time. Use tools that correct it:
  - `wrk2 -R<rate> --latency` (constant throughput + HdrHistogram correction) — the reference implementation.
  - k6 arrival-rate executors record each iteration against its scheduled start, so `iteration_duration` is corrected; still prefer open model for tail truth.
  - HdrHistogram `recordValueWithExpectedInterval(v, interval)` back-fills omitted samples.
- **Never** report tail latency from a closed-loop `ab`/single-thread `wrk` run at saturation.

### Percentiles, not averages

- Report **p50 / p95 / p99 / max** — never mean alone. Averages hide the tail; the mean can be 40ms while p99 is 3s.
- Latency distributions are right-skewed and multi-modal (cache hit vs miss, GC pause). Mean sits in an empty valley between modes.
- **p99 is a per-request user probability, not a rare edge case:** a page issuing 100 requests hits its p99 latency on ~63% of loads (1−0.99¹⁰⁰). Tail latency *is* the typical experience for fan-out.
- Percentiles **don't average across shards** — never mean the p99s of two load-gen workers. Merge the raw histograms (HdrHistogram) or you get garbage.

### Little's Law — sanity-check every run

`L = λ × W` → concurrency = throughput × latency.

- Required VUs ≈ target_RPS × avg_response_time_s. Wanting 1000 RPS at 200ms needs ≥200 concurrent in flight.
- If achieved `RPS ≈ VUs / (latency + think_time)` doesn't hold, your generator is the limit, not the server.

### Saturation: knee vs. collapse

As load rises: throughput climbs, then **plateaus (the knee)** while latency turns hockey-stick. Past the knee, more load buys only more latency and eventually errors/collapse.

- **Report the knee** (max sustainable throughput at acceptable latency), not the collapse point.
- USE method for the SUT: **U**tilization, **S**aturation (queue depth/run-queue), **E**rrors — per resource (CPU, mem, disk, net, connection pool).

### Warm-up, JIT, GC — discard the transient

- **Always discard a warm-up window.** Cold caches, empty connection pools, unfilled DB buffer pool, and JIT (JVM/V8) compilation make the first seconds meaningless. In k6 use a ramp stage; in JMH warm-up iterations; in Gatling a `nothingFor`/ramp.
- **GC/JIT cause tail latency**, not throughput loss — stop-the-world pauses show up as p99/max spikes. A soak test surfaces them; a 30s run won't.
- JVM needs thousands of iterations to reach steady state (C2 compiler). Benchmark JVM services *after* warm-up or numbers are 2–10× pessimistic.

### Measure the server, not the client

- **Run the generator on a separate host** from the SUT — co-located, they fight for CPU/net and you measure interference.
- **Load-generator-is-the-bottleneck** is the most common false result. Detect: latency rises but *server* CPU is low while *generator* CPU pegs, or you hit ephemeral-port/FD exhaustion. Symptoms:
  - Single-threaded generators (`ab`, artillery/Node, locust single-worker) cap ~1 core.
  - Ephemeral port exhaustion (`netstat` full of TIME_WAIT) → looks like server refusing connections.
  - FD limit (`ulimit -n`) caps concurrent connections.
- **Fix:** multi-core generators (k6, Gatling, wrk, JMeter), distributed mode (locust master/worker, k6 cloud/operator), raise `ulimit -n`, reuse connections. **Always correlate client numbers with server-side metrics** (APM/Prometheus) — divergence means you're measuring the wrong thing.

### Connection reuse & think time

- **Keep-alive dominates:** new-connection-per-request adds TCP + TLS handshake (often > the request itself). Most tools reuse per VU by default (k6 does). Test *both* if clients differ (mobile cold-start vs. pooled service). k6: `noConnectionReuse: true` / `noVUConnectionReuse`.
- **Think time** models real users (pauses between actions) and, in closed models, sets effective concurrency (Little's Law). Zero think time in a closed model = an unrealistic hammer that mostly measures your keep-alive path. Use randomized think time (not fixed — fixed sleeps create synchronized request waves).

### Realistic data & cardinality

- **Hitting one key lies:** repeating `GET /users/1` measures a cache/DB-buffer hit path nobody experiences. Use realistic key distribution — **Zipfian/Pareto** (80/20), not uniform, not constant.
- Dataset must exceed cache size, or you benchmark RAM.
- Unique-per-request params defeat query-plan/result caches (good for worst case); constant params inflate results.
- Match production **payload sizes, cardinality, and content** (unicode, large records).

---

## Load Shapes

One canonical k6 config per shape. Swap `constant-vus`→`constant-arrival-rate` (and `ramping-vus`→`ramping-arrival-rate`, `target` = req/s) for the open model when finding limits.

| Shape          | Goal                       | Pattern                         |
| -------------- | -------------------------- | ------------------------------- |
| **Smoke**      | Validate script works      | 1–5 VUs, 1 min                  |
| **Load**       | Baseline at expected load  | Ramp → hold steady → ramp down  |
| **Stress**     | Find breaking point        | Step up past expected until fail |
| **Spike**      | Sudden burst recovery      | Jump to N×, hold, drop, recover |
| **Soak**       | Leaks/degradation          | Moderate load for hours         |
| **Breakpoint** | Max capacity               | Linear ramp with no cap         |

```javascript
// Load: warm-up ramp is mandatory; thresholds abort/mark-fail the run.
export const options = {
  stages: [
    { duration: "2m", target: 100 }, // ramp (discard for analysis)
    { duration: "10m", target: 100 }, // steady state = the measurement
    { duration: "2m", target: 0 }, // ramp down
  ],
  thresholds: {
    http_req_duration: ["p(95)<500", "p(99)<1000"],
    http_req_failed: ["rate<0.01"],
  },
};
```

```javascript
// Stress (open model — reveals true limit): step arrival rate until errors/latency break.
export const options = {
  scenarios: {
    stress: {
      executor: "ramping-arrival-rate",
      startRate: 50, timeUnit: "1s",
      preAllocatedVUs: 100, maxVUs: 2000, // headroom or iterations drop silently
      stages: [
        { duration: "2m", target: 200 },
        { duration: "2m", target: 400 },
        { duration: "2m", target: 800 }, // watch for the knee here
        { duration: "2m", target: 1600 },
      ],
    },
  },
};
```

```javascript
// Spike: validate autoscaling/cache/recovery, not steady throughput.
export const options = {
  stages: [
    { duration: "1m", target: 50 }, // baseline
    { duration: "10s", target: 1000 }, // spike
    { duration: "3m", target: 1000 }, // hold — does it shed load or fall over?
    { duration: "10s", target: 50 }, // drop
    { duration: "3m", target: 50 }, // recovery — latency must return to baseline
  ],
};
```

```javascript
// Soak: surfaces leaks, FD/connection exhaustion, GC creep, disk fill. Hours, not minutes.
export const options = {
  vus: 100, duration: "4h",
  thresholds: { http_req_duration: ["p(99)<1000"], http_req_failed: ["rate<0.01"] },
};
// Watch: RSS growth, GC frequency, pool saturation, p99 drift over time (not the average).
```

---

## Tools

### k6 (default for API/protocol load testing)

Go-based, multi-core, JS scripting, open+closed executors, built-in thresholds. Best default.

```javascript
import http from "k6/http";
import { check, sleep } from "k6";
import { Rate, Trend } from "k6/metrics";

const errorRate = new Rate("errors");

export const options = {
  scenarios: {
    // open model: request rate is what you control, independent of server health
    api: {
      executor: "ramping-arrival-rate",
      startRate: 100, timeUnit: "1s",
      preAllocatedVUs: 200, maxVUs: 1000,
      stages: [{ duration: "2m", target: 500 }, { duration: "5m", target: 500 }],
    },
  },
  thresholds: {
    "http_req_duration{expected_response:true}": ["p(95)<500", "p(99)<1000"],
    http_req_failed: ["rate<0.01"],
  },
};

export function setup() {
  const r = http.post(`${__ENV.API_URL}/auth/login`,
    JSON.stringify({ email: "load@test.com", password: "x" }),
    { headers: { "Content-Type": "application/json" } });
  return { token: r.json("token") }; // runs once; returned data passed to default fn
}

export default function (data) {
  const res = http.get(`${__ENV.API_URL}/api/users/${__VU}`, {
    headers: { Authorization: `Bearer ${data.token}` },
    tags: { name: "GetUser" }, // aggregate by name, NOT by dynamic URL (avoids metric cardinality blowup)
  });
  errorRate.add(res.status !== 200);
  check(res, { "200": (r) => r.status === 200 });
  sleep(1);
}
```

k6 gotchas:

- **Tag dynamic URLs** with a static `tags.name`, or per-URL metrics explode and dashboards choke.
- `discardResponseBodies: true` cuts generator memory/CPU when you don't inspect bodies — raises max achievable load.
- `check()` failures do **not** fail the run; only `thresholds` do (and `--fail-on-check` / `abortOnFail` on a threshold aborts early).
- CLI: `k6 run -e API_URL=... script.js`; `--out experimental-prometheus-rw` or `--out influxdb=...` for Grafana; `k6 run --vus 50 --duration 30s` overrides options.
- Extensions (xk6) add gRPC, SQL, browser, Kafka — not in the core binary.

### Locust (Python, closed-model, distributed)

Use when scenarios need Python logic/libraries or complex stateful user journeys.

```python
from locust import HttpUser, task, between

class WebsiteUser(HttpUser):
    wait_time = between(1, 3)  # think time → sets effective concurrency (closed model)

    def on_start(self):
        r = self.client.post("/login", json={"username": "u", "password": "p"})
        self.token = r.json()["token"]

    @task(3)  # weight: runs 3× as often as weight-1 tasks
    def browse(self):
        # name= groups metrics for parameterized URLs (same role as k6 tags.name)
        self.client.get(f"/api/products/{randint(1,1000)}", name="/api/products/[id]",
                        headers={"Authorization": f"Bearer {self.token}"})
```

```bash
locust -f lf.py --host=https://api.example.com                       # web UI
locust -f lf.py --headless -u 500 -r 50 -t 5m --host=...             # -u users, -r spawn/s
locust -f lf.py --master   &   locust -f lf.py --worker --master-host=IP  # distributed: 1 worker/core
```

⚠ Single Locust process is **one core** — it *is* the bottleneck past a few hundred RPS. Always run master + workers (≥1 worker per CPU) for real load. `FastHttpUser` (geventhttpclient) is ~5–6× faster than the default `HttpUser` (requests-based) — use it for high RPS.

### JMeter (Java, GUI + CLI, protocol breadth)

Mature, huge protocol/plugin ecosystem (JDBC, JMS, LDAP). Verbose XML `.jmx`.

- **Author in GUI, run headless:** `jmeter -n -t plan.jmx -l results.jtl -e -o report/` (`-n` non-GUI, `-e -o` HTML report). GUI mode adds huge overhead — never load-test in it.
- Thread Group = closed model (threads, ramp-up, loops). "Concurrency Thread Group" / "Throughput Shaping Timer" plugins add open-model + arbitrary shapes.
- Distributed: `jmeter -n -t plan.jmx -R host1,host2` (remote servers).
- Disable "View Results Tree" listeners under load (memory hog); write JTL and post-process.
- ⚠ JMeter's default aggregate report can suffer coordinated omission; prefer the "Response Times Percentiles" / backend-listener → InfluxDB path and treat timers correctly.

### Gatling (Scala/Java/Kotlin DSL, efficient, code-as-test)

Async (Netty), high load per core, versioned code, good HTML reports. `injectOpen`/`injectClosed` make the model explicit.

```scala
setUp(
  scn.injectOpen(                       // open model: arrivals independent of responses
    rampUsersPerSec(10).to(200).during(5.minutes),
    constantUsersPerSec(200).during(10.minutes)
  )
).protocols(http.baseUrl("https://api.example.com"))
 .assertions(global.responseTime.percentile4.lt(1000)) // percentile4 = p99 by default
```

### artillery (YAML, quick, Node)

Fast to author; good for smoke/CI and moderate load. ⚠ Node = effectively single-core → limited generator throughput; distribute (AWS Lambda/Fargate mode) or switch to k6/Gatling for heavy load.

```yaml
config:
  target: "https://api.example.com"
  phases:
    - { duration: 60, arrivalRate: 5, name: warmup }      # open model
    - { duration: 300, arrivalRate: 20, rampTo: 50, name: ramp }
  ensure: { p95: 500, maxErrorRate: 1 }                    # fails CI on breach
scenarios:
  - flow:
      - post: { url: /auth/login, json: { u: "{{ $randomString() }}" }, capture: { json: "$.token", as: token } }
      - think: 2
      - get: { url: /api/products, headers: { Authorization: "Bearer {{ token }}" } }
```

### HTTP micro-benchmark one-liners

Fast single-endpoint checks — not user journeys:

```bash
wrk2 -t4 -c400 -R2000 --latency http://host/ep   # PREFER: constant 2000 RPS, corrects coordinated omission
wrk  -t12 -c400 -d30s --latency http://host/ep    # high throughput but closed-loop → CO at saturation
oha  -z 30s -c 100 http://host/ep                 # Rust, live TUI, HdrHistogram
hey  -n 100000 -c 100 http://host/ep              # simple; single generator core-bound
```

⚠ `ab` (ApacheBench) is single-threaded, no keep-alive by default (`-k` to enable), and severely subject to coordinated omission — avoid for anything but a trivial reachability check.

---

## API Scenario Patterns (k6)

**Mixed read/write with per-scenario thresholds** — model real traffic ratios:

```javascript
export const options = {
  scenarios: {
    reads:  { executor: "constant-arrival-rate", rate: 700, timeUnit: "1s",
              preAllocatedVUs: 50, maxVUs: 200, exec: "read" },
    writes: { executor: "constant-arrival-rate", rate: 300, timeUnit: "1s",
              preAllocatedVUs: 30, maxVUs: 100, exec: "write" },
  },
  thresholds: {
    "http_req_duration{scenario:reads}":  ["p(95)<200"],
    "http_req_duration{scenario:writes}": ["p(95)<500"],
    http_req_failed: ["rate<0.01"],
  },
};
export function read()  { /* GET with Zipfian id, not id=1 */ }
export function write() { /* POST unique payload */ }
```

**GraphQL:** POST the query, and assert on the body — a GraphQL error returns **HTTP 200**, so `http_req_failed` won't catch it:

```javascript
const res = http.post(url, JSON.stringify({ query, variables }), { headers });
check(res, { "no gql errors": (r) => !r.json("errors") }); // 200 ≠ success here
```

**WebSocket:** `import ws from "k6/ws"`; open a socket, subscribe, assert on messages, `socket.setTimeout(() => socket.close(), 60000)`; check `res.status === 101`. For long-lived connections, concurrency ≠ RPS — measure connections held + message latency.

---

## Database Performance

**Single benchmark harness** (percentiles, warm-up, sorted) — reuse for any async op, don't re-implement per query:

```typescript
import { performance } from "perf_hooks";

async function bench(name: string, fn: () => Promise<unknown>, iterations = 100) {
  for (let i = 0; i < 10; i++) await fn(); // WARM-UP: JIT, pool fill, buffer cache (discard)
  const t: number[] = [];
  for (let i = 0; i < iterations; i++) {
    const s = performance.now();
    await fn();
    t.push(performance.now() - s);
  }
  t.sort((a, b) => a - b);
  const at = (p: number) => t[Math.min(t.length - 1, Math.floor(t.length * p))];
  return { name, min: t[0], p50: at(0.5), p95: at(0.95), p99: at(0.99), max: t.at(-1) };
}
// Usage: await bench("findById", () => db.users.findById(zipfianId()));
```

⚠ Serial-loop benchmarks measure **single-connection latency**, not concurrent throughput — they miss lock contention, pool exhaustion, and buffer thrash. For throughput, drive N concurrent workers for a fixed duration and report QPS + p95 together (increase N until QPS plateaus = pool/DB knee).

**EXPLAIN / slow-query hunting (Postgres):**

```sql
EXPLAIN (ANALYZE, BUFFERS) SELECT ...;   -- ANALYZE = real timings; BUFFERS = cache hits vs disk reads

-- pg_stat_statements: rank by TOTAL time (calls × mean) — the real load driver, not slowest single query
SELECT query, calls, mean_exec_time, total_exec_time, rows
FROM pg_stat_statements ORDER BY total_exec_time DESC LIMIT 20;

-- Seq scans that should be index scans (large seq_tup_read on hot tables)
SELECT relname, seq_scan, seq_tup_read, idx_scan
FROM pg_stat_user_tables WHERE seq_scan > idx_scan ORDER BY seq_tup_read DESC;
```

- Read EXPLAIN bottom-up; watch for `Seq Scan` on big tables, row-estimate vs `actual rows` divergence (stale stats → `ANALYZE`), and `Nested Loop` over large sets (want Hash/Merge join).
- **N+1 detection:** normalize SQL (strip literals) and count executions of each shape per request; >~10 identical shapes = N+1 → batch/eager-load. ORMs hide these behind lazy relations.
- Benchmark with **production-scale rows** and realistic key distribution — a query that's instant on 10k rows table-scans on 10M.

---

## Profiling

Profile *after* a load test localizes the hot endpoint — don't profile blind.

| Target        | Tool / command                                          | Reads as                          |
| ------------- | ------------------------------------------------------- | --------------------------------- |
| Node CPU      | `node --prof app.js` → `--prof-process`; or `--cpu-prof` (`.cpuprofile` → Chrome DevTools) | flame graph of self time |
| Node heap     | `v8.writeHeapSnapshot()`; compare 2 snapshots for growth | retained objects → leak source    |
| Node loop lag | measure delay of `setImmediate`; >100ms = blocked loop  | sync work starving the event loop |
| Python CPU    | `python -m cProfile -o out.prof`; view with `snakeviz`; `py-spy top --pid` (no code change, prod-safe) | cumulative time |
| Rust          | `cargo flamegraph`; `perf record`/`perf report`         | flame graph                       |
| JVM           | async-profiler (`-agentpath`), JFR                      | flame graph, alloc profiling      |
| Any/prod      | `perf`, `bpftrace`, continuous profiling (Pyroscope/Parca) | on-CPU/off-CPU                  |

- **On-CPU vs off-CPU:** high latency + low CPU = you're blocked on I/O/locks; profile off-CPU (wall-clock), not just on-CPU, or you'll stare at an idle flame graph.
- **Memory leak signal:** RSS/heap grows monotonically across a soak and never returns after load stops. Two heap snapshots under steady load → diff retained set. GC frequency climbing is an early tell.

---

## Metrics, Budgets, Bottlenecks

### Target metrics

| Metric        | Meaning              | Typical target | Alert  |
| ------------- | -------------------- | -------------- | ------ |
| Latency p50   | median              | <100ms         | >200ms |
| Latency p95   | tail                | <500ms         | >1s    |
| Latency p99   | far tail (fan-out!) | <1s            | >2s    |
| Throughput    | req/s at the knee   | per SLO        | —      |
| Error rate    | failed %            | <0.1%          | >1%    |
| Saturation    | resource util       | <70%           | >85%   |

Define budgets **per endpoint** (not global) and **enforce in CI** — k6 `thresholds`, artillery `ensure`, Gatling `assertions`, Lighthouse CI `assert` all fail the build on breach. Web budgets: LCP <2.5s, INP <200ms, CLS <0.1, JS bundle <300KB.

Instrument the server with a **Histogram** (Prometheus `histogram_quantile(0.95, rate(http_request_duration_seconds_bucket[5m]))`), not a gauge/summary you can't aggregate across instances. Pick buckets around your SLO (e.g. `[0.01,0.05,0.1,0.25,0.5,1,2.5,5]`) — percentiles are only as precise as bucket edges.

### Common bottlenecks → fix

| Bottleneck         | Symptom                        | Diagnose               | Fix                              |
| ------------------ | ------------------------------ | ---------------------- | -------------------------------- |
| N+1 queries        | latency ∝ result count         | query count/request    | batch / eager load               |
| Missing index      | seq scan on big table          | EXPLAIN ANALYZE        | add index; check selectivity     |
| Connection pool    | timeouts at load, low CPU      | pool saturation metric | size pool (≈ cores×2..4 for DB), queue |
| Sync I/O on loop   | high event-loop lag            | loop-lag probe         | async / worker threads           |
| Memory leak        | heap grows over soak           | heap snapshot diff     | fix retention (caches, listeners) |
| Chatty serialization | high CPU in JSON             | CPU profile            | stream / faster codec / smaller payload |
| Large payloads     | network-bound, high TTFB       | response-size metric   | pagination, compression, fields  |
| Lock contention    | throughput plateaus, cores idle | mutex/off-CPU profile | shard locks, reduce critical section |

---

## Realistic Load Modeling

- **Weighted user mix, not one journey:** e.g. 60% browsers, 30% shoppers, 10% power users; pick per-VU and follow a session (login → browse N pages → maybe cart → maybe checkout) with think time between steps.
- **Key distribution:** draw IDs Zipfian/Pareto (hot keys + long tail), never constant, never pure-uniform over a tiny range.
- **Randomized think time** (jittered), not fixed — fixed sleeps synchronize VUs into artificial request waves.
- **Include error/edge scenarios** in the mix (invalid input→400, not-found→404, large payloads, timeouts) so you measure error-path cost, which is often worse than the happy path.
- **Data generation:** `@faker-js/faker` / Python `faker`; apply 80/20 (20% of users generate 80% of activity) so cache/index behavior matches production.

---

## Best Practices (condensed)

- **Environment:** production-like hardware, data volume, and config (cache/CDN/LB). Generator on a **separate host** from the SUT.
- **Baseline first:** record current p50/p95/p99 + throughput before optimizing; track over time to catch regressions. Nightly full loads, smoke on every commit.
- **Change one variable at a time**, measure before/after with the same script, keep results in version control.
- **Correlate** load-gen output with server-side APM/Prometheus every run — divergence = you're measuring the generator or the network, not the service.
- **Analyze percentiles + max, over the steady-state window only** (drop warm-up).
- **Load tests complement, never replace, production monitoring** (real user cardinality/geography).

---

## Verification checklists

**Before trusting a load-test result:**

- [ ] Generator ran on a **separate host**; generator CPU/net/FDs were **not** the limit (server-side metrics corroborate)
- [ ] **Open model** (arrival rate) used for any breaking-point/capacity claim; VU headroom sufficient (no dropped-iteration warnings)
- [ ] **Coordinated omission** ruled out (open executor or wrk2/HdrHistogram); tail isn't suspiciously flat
- [ ] **Warm-up window discarded**; steady-state window is what's reported
- [ ] Reported **p50/p95/p99/max**, not just mean; percentiles merged from raw histograms, not averaged
- [ ] Realistic **key distribution** (not id=1) and **dataset > cache**; payload sizes production-like
- [ ] Keep-alive setting matches the real client; think time randomized
- [ ] The **knee** (max sustainable throughput at acceptable latency) identified, not just the collapse point
- [ ] Little's Law sanity check passes (achieved RPS ≈ concurrency / (latency+think))

**Before shipping a perf test into CI:**

- [ ] Thresholds/assertions defined **per endpoint** and fail the build on breach
- [ ] Smoke variant (1–5 VUs) runs on every commit; full load nightly/on-PR
- [ ] Dynamic URLs tagged with static names (no metric-cardinality blowup)
- [ ] Secrets/tokens injected via env, not hardcoded; test data cleaned up in teardown

**Before calling a bottleneck fixed:**

- [ ] Root cause identified via profile/EXPLAIN, not guessed
- [ ] Re-ran the **same** script; before/after deltas on p95/p99 + throughput
- [ ] Fix didn't just move the bottleneck (next resource now saturates at a higher load)
- [ ] Regression guard in CI (budget threshold) so it can't silently return
