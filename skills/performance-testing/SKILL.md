---
name: performance-testing
description: Performance testing and load testing expertise including k6, locust, JMeter, Gatling, artillery, API load testing, database query optimization, benchmarking strategies, profiling techniques, metrics analysis (p95/p99 latency, throughput, RPS), performance budgets, and bottleneck identification. Use when implementing load tests, stress tests, spike tests, soak tests, analyzing system behavior under concurrent users, measuring saturation points, or optimizing application performance under load. Trigger keywords: performance testing, load testing, stress testing, stress test, load test, performance test, k6, locust, JMeter, Gatling, artillery, benchmark, benchmarking, profiling, latency, throughput, RPS, requests per second, concurrent users, virtual users, percentile, p95, p99, p50, median latency, saturation, bottleneck, performance budget, API load testing, database performance, query optimization, slow queries, scalability testing, capacity planning, response time, error rate, apdex.
---

# Performance Testing

## Overview

Performance testing validates that applications meet speed, scalability, and stability requirements under various load conditions. This skill provides comprehensive expertise in load testing tools (k6, locust, JMeter, Gatling), API and database performance testing, benchmarking strategies, profiling techniques, and systematic approaches to identifying and resolving performance bottlenecks.

## When to Use This Skill

Use this skill when you need to:
- Implement load tests, stress tests, spike tests, or soak tests
- Measure API endpoint performance under concurrent users
- Optimize database queries and identify slow queries
- Analyze latency percentiles (p50, p95, p99) and throughput (RPS)
- Set up performance monitoring and alerting
- Identify system saturation points and bottlenecks
- Establish performance budgets and SLOs
- Conduct capacity planning and scalability analysis

## Instructions

## Test Types and Patterns

### Load Test Types

| Test Type | Purpose | Pattern | When to Use |
|-----------|---------|---------|-------------|
| **Load Test** | Validate performance under expected load | Constant VUs over time | Establish baseline performance |
| **Stress Test** | Find breaking point | Gradual ramp-up until failure | Determine system limits |
| **Spike Test** | Test sudden traffic bursts | Rapid increase to high load | Validate autoscaling, caching |
| **Soak Test** | Detect memory leaks, degradation | Moderate load for extended time (hours) | Production readiness |
| **Breakpoint Test** | Find maximum capacity | Incremental load increases | Capacity planning |

### k6 Test Patterns

**Pattern: Baseline Load Test**
```javascript
export const options = {
  vus: 50,
  duration: '5m',
  thresholds: {
    http_req_duration: ['p(95)<500'],
  },
};
```

**Pattern: Stress Test (Find Breaking Point)**
```javascript
export const options = {
  stages: [
    { duration: '2m', target: 100 },
    { duration: '5m', target: 100 },
    { duration: '2m', target: 200 },
    { duration: '5m', target: 200 },
    { duration: '2m', target: 300 },
    { duration: '5m', target: 300 },
    { duration: '5m', target: 0 },
  ],
};
```

**Pattern: Spike Test**
```javascript
export const options = {
  stages: [
    { duration: '30s', target: 50 },   // Normal load
    { duration: '10s', target: 500 },  // Spike!
    { duration: '1m', target: 500 },   // Hold spike
    { duration: '10s', target: 50 },   // Drop
    { duration: '1m', target: 50 },    // Recovery
  ],
};
```

**Pattern: Soak Test (Memory Leaks)**
```javascript
export const options = {
  vus: 100,
  duration: '4h',  // Extended duration
  thresholds: {
    http_req_duration: ['p(95)<500'],
    http_req_failed: ['rate<0.01'],
  },
};
```

## Instructions

### 1. Load Testing with Modern Tools

**k6 (Recommended for API Load Testing):**

```bash
# Installation
brew install k6
# or
npm install -g k6
```

```javascript
// load-test.js
import http from "k6/http";
import { check, sleep } from "k6";
import { Rate, Trend } from "k6/metrics";

// Custom metrics
const errorRate = new Rate("errors");
const latencyTrend = new Trend("latency");

export const options = {
  stages: [
    { duration: "2m", target: 100 }, // Ramp up
    { duration: "5m", target: 100 }, // Steady state
    { duration: "2m", target: 200 }, // Spike
    { duration: "5m", target: 200 }, // Sustained spike
    { duration: "2m", target: 0 }, // Ramp down
  ],
  thresholds: {
    http_req_duration: ["p(95)<500", "p(99)<1000"],
    errors: ["rate<0.01"],
    http_req_failed: ["rate<0.01"],
  },
};

export default function () {
  const payload = JSON.stringify({
    username: `user_${__VU}_${__ITER}`,
    action: "test",
  });

  const params = {
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${__ENV.API_TOKEN}`,
    },
  };

  const response = http.post(
    "https://api.example.com/endpoint",
    payload,
    params,
  );

  latencyTrend.add(response.timings.duration);
  errorRate.add(response.status !== 200);

  check(response, {
    "status is 200": (r) => r.status === 200,
    "response time < 500ms": (r) => r.timings.duration < 500,
    "has required fields": (r) => {
      const body = JSON.parse(r.body);
      return body.id && body.status;
    },
  });

  sleep(1);
}

export function handleSummary(data) {
  return {
    "summary.json": JSON.stringify(data),
    stdout: textSummary(data, { indent: " ", enableColors: true }),
  };
}
```

**Run k6 Tests:**

```bash
# Basic run
k6 run load-test.js

# With environment variables
k6 run -e API_TOKEN=xxx load-test.js

# Cloud execution
k6 cloud load-test.js

# Output to InfluxDB for Grafana
k6 run --out influxdb=http://localhost:8086/k6 load-test.js
```

**Locust (Python-based Load Testing):**

```python
# locustfile.py
from locust import HttpUser, task, between
from locust import events
import time

class WebsiteUser(HttpUser):
    wait_time = between(1, 3)

    def on_start(self):
        """Login on start"""
        response = self.client.post("/login", json={
            "username": "testuser",
            "password": "testpass"
        })
        self.token = response.json().get("token")

    @task(3)
    def view_products(self):
        """Most common action"""
        self.client.get("/api/products", headers={
            "Authorization": f"Bearer {self.token}"
        })

    @task(2)
    def view_product_detail(self):
        """View individual product"""
        self.client.get("/api/products/1", headers={
            "Authorization": f"Bearer {self.token}"
        })

    @task(1)
    def add_to_cart(self):
        """Less common action"""
        self.client.post("/api/cart", json={
            "product_id": 1,
            "quantity": 1
        }, headers={
            "Authorization": f"Bearer {self.token}"
        })

class AdminUser(HttpUser):
    wait_time = between(2, 5)
    weight = 1  # 1 admin per 10 regular users

    @task
    def view_dashboard(self):
        self.client.get("/admin/dashboard")

# Custom metrics
@events.request.add_listener
def on_request(request_type, name, response_time, response_length, exception, **kwargs):
    if exception:
        print(f"Request failed: {name} - {exception}")
```

**Run Locust:**

```bash
# Web UI mode
locust -f locustfile.py --host=https://api.example.com

# Headless mode
locust -f locustfile.py --headless -u 100 -r 10 --run-time 5m --host=https://api.example.com

# Distributed mode
locust -f locustfile.py --master
locust -f locustfile.py --worker --master-host=192.168.1.1
```

**Artillery (YAML-based Load Testing):**

```yaml
# artillery-config.yml
config:
  target: "https://api.example.com"
  phases:
    - duration: 60
      arrivalRate: 5
      name: "Warm up"
    - duration: 120
      arrivalRate: 20
      rampTo: 50
      name: "Ramp up"
    - duration: 300
      arrivalRate: 50
      name: "Sustained load"
  defaults:
    headers:
      Content-Type: "application/json"
  plugins:
    expect: {}
  ensure:
    p95: 500
    maxErrorRate: 1

scenarios:
  - name: "User journey"
    flow:
      - post:
          url: "/auth/login"
          json:
            username: "{{ $randomString() }}"
            password: "password123"
          capture:
            - json: "$.token"
              as: "authToken"
          expect:
            - statusCode: 200
            - hasProperty: "token"

      - get:
          url: "/api/products"
          headers:
            Authorization: "Bearer {{ authToken }}"
          expect:
            - statusCode: 200
            - contentType: "application/json"

      - think: 2

      - post:
          url: "/api/cart"
          headers:
            Authorization: "Bearer {{ authToken }}"
          json:
            productId: "{{ $randomNumber(1, 100) }}"
            quantity: 1
          expect:
            - statusCode: 201
```

**Run Artillery:**

```bash
# Run test
artillery run artillery-config.yml

# Generate report
artillery run artillery-config.yml --output report.json
artillery report report.json --output report.html
```

### 2. API Load Testing Patterns

**REST API Load Testing:**

```javascript
// k6 API load test with authentication and data variation
import http from 'k6/http';
import { check, sleep } from 'k6';
import { SharedArray } from 'k6/data';
import { randomIntBetween } from 'k6/x/util';

// Load test data from CSV
const testData = new SharedArray('users', function () {
  return JSON.parse(open('./test-data.json'));
});

export const options = {
  scenarios: {
    // Read-heavy workload (70% reads)
    reads: {
      executor: 'constant-arrival-rate',
      rate: 700,
      timeUnit: '1s',
      duration: '5m',
      preAllocatedVUs: 50,
      maxVUs: 200,
      exec: 'readScenario',
    },
    // Write workload (30% writes)
    writes: {
      executor: 'constant-arrival-rate',
      rate: 300,
      timeUnit: '1s',
      duration: '5m',
      preAllocatedVUs: 30,
      maxVUs: 100,
      exec: 'writeScenario',
    },
  },
  thresholds: {
    'http_req_duration{scenario:reads}': ['p(95)<200', 'p(99)<500'],
    'http_req_duration{scenario:writes}': ['p(95)<500', 'p(99)<1000'],
    'http_req_failed': ['rate<0.01'],
  },
};

let authToken;

export function setup() {
  const loginRes = http.post(`${__ENV.API_URL}/auth/login`, JSON.stringify({
    email: 'loadtest@example.com',
    password: 'test123',
  }), {
    headers: { 'Content-Type': 'application/json' },
  });

  return { token: loginRes.json('token') };
}

export function readScenario(data) {
  const headers = {
    'Authorization': `Bearer ${data.token}`,
    'Content-Type': 'application/json',
  };

  // GET request with query parameters
  const userId = randomIntBetween(1, 10000);
  const res = http.get(
    `${__ENV.API_URL}/api/users/${userId}`,
    { headers, tags: { name: 'GetUser' } }
  );

  check(res, {
    'status is 200': (r) => r.status === 200,
    'has user data': (r) => r.json('id') === userId,
    'response time OK': (r) => r.timings.duration < 200,
  });

  sleep(0.5);
}

export function writeScenario(data) {
  const headers = {
    'Authorization': `Bearer ${data.token}`,
    'Content-Type': 'application/json',
  };

  // POST request with dynamic payload
  const user = testData[Math.floor(Math.random() * testData.length)];
  const res = http.post(
    `${__ENV.API_URL}/api/orders`,
    JSON.stringify({
      userId: user.id,
      items: [
        { productId: randomIntBetween(1, 100), quantity: 1 },
      ],
      timestamp: new Date().toISOString(),
    }),
    { headers, tags: { name: 'CreateOrder' } }
  );

  check(res, {
    'status is 201': (r) => r.status === 201,
    'order created': (r) => r.json('id') !== undefined,
  });

  sleep(1);
}
```

**GraphQL API Load Testing:**

```javascript
import http from 'k6/http';
import { check } from 'k6';

export default function() {
  const query = `
    query GetUserWithOrders($userId: ID!) {
      user(id: $userId) {
        id
        name
        orders(limit: 10) {
          id
          total
          items {
            productId
            quantity
          }
        }
      }
    }
  `;

  const variables = {
    userId: `${__VU}`,
  };

  const res = http.post('https://api.example.com/graphql', JSON.stringify({
    query,
    variables,
  }), {
    headers: {
      'Content-Type': 'application/json',
      'Authorization': `Bearer ${__ENV.TOKEN}`,
    },
  });

  check(res, {
    'no GraphQL errors': (r) => !r.json('errors'),
    'user data present': (r) => r.json('data.user.id') === variables.userId,
  });
}
```

**WebSocket Load Testing:**

```javascript
import ws from 'k6/ws';
import { check } from 'k6';

export default function() {
  const url = 'wss://api.example.com/ws';
  const params = { tags: { my_tag: 'websocket' } };

  const res = ws.connect(url, params, function (socket) {
    socket.on('open', () => {
      console.log('Connected');
      socket.send(JSON.stringify({ type: 'subscribe', channel: 'updates' }));
    });

    socket.on('message', (data) => {
      const msg = JSON.parse(data);
      check(msg, {
        'valid message': (m) => m.type !== undefined,
      });
    });

    socket.on('error', (e) => {
      console.log('Error:', e.error());
    });

    socket.setTimeout(() => {
      socket.close();
    }, 60000);
  });

  check(res, { 'status is 101': (r) => r && r.status === 101 });
}
```

### 3. Database Performance Testing

**Query Performance Testing:**

```typescript
// database-perf-test.ts
import { performance } from 'perf_hooks';
import { Pool } from 'pg';

interface QueryBenchmark {
  query: string;
  params?: any[];
  iterations: number;
  results: {
    min: number;
    max: number;
    avg: number;
    p50: number;
    p95: number;
    p99: number;
  };
}

async function benchmarkQuery(
  pool: Pool,
  query: string,
  params: any[] = [],
  iterations: number = 100
): Promise<QueryBenchmark> {
  const timings: number[] = [];

  // Warmup
  for (let i = 0; i < 10; i++) {
    await pool.query(query, params);
  }

  // Benchmark
  for (let i = 0; i < iterations; i++) {
    const start = performance.now();
    await pool.query(query, params);
    timings.push(performance.now() - start);
  }

  timings.sort((a, b) => a - b);

  return {
    query: query.substring(0, 100),
    params,
    iterations,
    results: {
      min: timings[0],
      max: timings[timings.length - 1],
      avg: timings.reduce((a, b) => a + b, 0) / timings.length,
      p50: timings[Math.floor(timings.length * 0.5)],
      p95: timings[Math.floor(timings.length * 0.95)],
      p99: timings[Math.floor(timings.length * 0.99)],
    },
  };
}

// Compare query performance
async function compareQueries() {
  const pool = new Pool({ /* config */ });

  const queries = [
    {
      name: 'Without Index',
      sql: 'SELECT * FROM users WHERE email = $1',
      params: ['test@example.com'],
    },
    {
      name: 'With Index',
      sql: 'SELECT * FROM users WHERE id = $1',
      params: [1],
    },
    {
      name: 'Complex Join',
      sql: `
        SELECT u.*, COUNT(o.id) as order_count
        FROM users u
        LEFT JOIN orders o ON u.id = o.user_id
        WHERE u.created_at > $1
        GROUP BY u.id
        LIMIT 100
      `,
      params: ['2024-01-01'],
    },
  ];

  for (const { name, sql, params } of queries) {
    const result = await benchmarkQuery(pool, sql, params);
    console.log(`\n${name}:`);
    console.table(result.results);
  }

  await pool.end();
}
```

**Connection Pool Load Testing:**

```typescript
// connection-pool-test.ts
import { Pool } from 'pg';
import { performance } from 'perf_hooks';

async function testConnectionPool(
  poolSize: number,
  concurrentQueries: number,
  duration: number
) {
  const pool = new Pool({
    max: poolSize,
    idleTimeoutMillis: 30000,
    connectionTimeoutMillis: 2000,
  });

  const stats = {
    totalQueries: 0,
    successfulQueries: 0,
    failedQueries: 0,
    timeouts: 0,
    queryTimes: [] as number[],
  };

  const startTime = Date.now();
  const workers: Promise<void>[] = [];

  for (let i = 0; i < concurrentQueries; i++) {
    workers.push((async () => {
      while (Date.now() - startTime < duration) {
        try {
          const start = performance.now();
          await pool.query('SELECT 1');
          const elapsed = performance.now() - start;

          stats.queryTimes.push(elapsed);
          stats.successfulQueries++;
        } catch (err) {
          stats.failedQueries++;
          if (err.message.includes('timeout')) {
            stats.timeouts++;
          }
        }
        stats.totalQueries++;
      }
    })());
  }

  await Promise.all(workers);
  await pool.end();

  stats.queryTimes.sort((a, b) => a - b);

  return {
    poolSize,
    concurrentQueries,
    duration,
    ...stats,
    avgQueryTime: stats.queryTimes.reduce((a, b) => a + b, 0) / stats.queryTimes.length,
    p95QueryTime: stats.queryTimes[Math.floor(stats.queryTimes.length * 0.95)],
    qps: stats.successfulQueries / (duration / 1000),
  };
}

// Test different pool sizes
async function findOptimalPoolSize() {
  const results = [];

  for (const poolSize of [5, 10, 20, 50, 100]) {
    console.log(`Testing pool size: ${poolSize}`);
    const result = await testConnectionPool(poolSize, 100, 30000);
    results.push(result);
  }

  console.table(results);
}
```

**N+1 Query Detection:**

```typescript
// n-plus-one-detector.ts
class QueryTracker {
  private queries: Map<string, number> = new Map();
  private startTime: number = 0;

  start() {
    this.queries.clear();
    this.startTime = Date.now();
  }

  track(sql: string) {
    const normalized = this.normalizeSql(sql);
    this.queries.set(normalized, (this.queries.get(normalized) || 0) + 1);
  }

  detectNPlusOne(threshold: number = 10): Array<{ query: string; count: number }> {
    const suspicious: Array<{ query: string; count: number }> = [];

    for (const [query, count] of this.queries.entries()) {
      if (count > threshold) {
        suspicious.push({ query, count });
      }
    }

    return suspicious.sort((a, b) => b.count - a.count);
  }

  private normalizeSql(sql: string): string {
    // Replace literals with placeholders for comparison
    return sql
      .replace(/\d+/g, '?')
      .replace(/'[^']*'/g, '?')
      .replace(/\s+/g, ' ')
      .trim();
  }

  report() {
    const duration = Date.now() - this.startTime;
    const nPlusOne = this.detectNPlusOne();

    console.log(`\nQuery Analysis (${duration}ms):`);
    console.log(`Total unique queries: ${this.queries.size}`);
    console.log(`Total query executions: ${Array.from(this.queries.values()).reduce((a, b) => a + b, 0)}`);

    if (nPlusOne.length > 0) {
      console.log('\nPotential N+1 Queries:');
      console.table(nPlusOne);
    }
  }
}
```

### 4. Benchmarking Strategies

**Micro-benchmarking (Function Level):**

```typescript
// Node.js with benchmark.js
import Benchmark from "benchmark";

const suite = new Benchmark.Suite();

const data = Array.from({ length: 10000 }, (_, i) => i);

suite
  .add("for loop", function () {
    let sum = 0;
    for (let i = 0; i < data.length; i++) {
      sum += data[i];
    }
    return sum;
  })
  .add("forEach", function () {
    let sum = 0;
    data.forEach((n) => {
      sum += n;
    });
    return sum;
  })
  .add("reduce", function () {
    return data.reduce((sum, n) => sum + n, 0);
  })
  .on("cycle", function (event: Benchmark.Event) {
    console.log(String(event.target));
  })
  .on("complete", function (this: Benchmark.Suite) {
    console.log("Fastest is " + this.filter("fastest").map("name"));
  })
  .run({ async: true });
```

**Database Query Benchmarking:**

```typescript
// benchmark-queries.ts
import { performance } from "perf_hooks";

interface BenchmarkResult {
  query: string;
  avgTime: number;
  minTime: number;
  maxTime: number;
  p95: number;
  iterations: number;
}

async function benchmarkQuery(
  name: string,
  queryFn: () => Promise<any>,
  iterations: number = 100,
): Promise<BenchmarkResult> {
  const times: number[] = [];

  // Warm up
  for (let i = 0; i < 10; i++) {
    await queryFn();
  }

  // Actual benchmark
  for (let i = 0; i < iterations; i++) {
    const start = performance.now();
    await queryFn();
    times.push(performance.now() - start);
  }

  times.sort((a, b) => a - b);

  return {
    query: name,
    avgTime: times.reduce((a, b) => a + b) / times.length,
    minTime: times[0],
    maxTime: times[times.length - 1],
    p95: times[Math.floor(times.length * 0.95)],
    iterations,
  };
}

// Usage
const results = await Promise.all([
  benchmarkQuery("findUserById", () => db.users.findById(1)),
  benchmarkQuery("findUserWithJoin", () =>
    db.users.findById(1).include("orders"),
  ),
  benchmarkQuery("complexAggregation", () =>
    db.orders.aggregate([
      /* pipeline */
    ]),
  ),
]);

console.table(results);
```

**HTTP Endpoint Benchmarking:**

```bash
# Using wrk
wrk -t12 -c400 -d30s --latency https://api.example.com/endpoint

# Using autocannon (Node.js)
npx autocannon -c 100 -d 30 -p 10 https://api.example.com/endpoint

# Using hey
hey -n 10000 -c 100 https://api.example.com/endpoint
```

### 5. Profiling Techniques

**Node.js CPU Profiling:**

```typescript
// Enable built-in profiler
// node --prof app.js
// node --prof-process isolate-*.log > processed.txt

// Programmatic profiling
import { Session } from "inspector";
import { writeFileSync } from "fs";

async function profileFunction(fn: () => Promise<any>) {
  const session = new Session();
  session.connect();

  session.post("Profiler.enable");
  session.post("Profiler.start");

  await fn();

  return new Promise<void>((resolve) => {
    session.post("Profiler.stop", (err, { profile }) => {
      writeFileSync("profile.cpuprofile", JSON.stringify(profile));
      session.disconnect();
      resolve();
    });
  });
}

// Usage
await profileFunction(async () => {
  // Code to profile
  await heavyComputation();
});
// Open profile.cpuprofile in Chrome DevTools
```

**Memory Profiling:**

```typescript
// Memory snapshot
import v8 from "v8";
import { writeFileSync } from "fs";

function takeHeapSnapshot(filename: string) {
  const snapshotStream = v8.writeHeapSnapshot(filename);
  console.log(`Heap snapshot written to ${snapshotStream}`);
}

// Track memory usage
function logMemoryUsage(label: string) {
  const usage = process.memoryUsage();
  console.log(`Memory [${label}]:`, {
    heapUsed: `${Math.round(usage.heapUsed / 1024 / 1024)}MB`,
    heapTotal: `${Math.round(usage.heapTotal / 1024 / 1024)}MB`,
    external: `${Math.round(usage.external / 1024 / 1024)}MB`,
    rss: `${Math.round(usage.rss / 1024 / 1024)}MB`,
  });
}

// Detect memory leaks
class MemoryLeakDetector {
  private samples: number[] = [];
  private interval: NodeJS.Timer | null = null;

  start(sampleInterval: number = 1000) {
    this.interval = setInterval(() => {
      this.samples.push(process.memoryUsage().heapUsed);

      if (this.samples.length > 60) {
        const trend = this.calculateTrend();
        if (trend > 0.1) {
          // 10% growth per minute
          console.warn("Potential memory leak detected!");
        }
        this.samples.shift();
      }
    }, sampleInterval);
  }

  private calculateTrend(): number {
    if (this.samples.length < 2) return 0;
    const first = this.samples[0];
    const last = this.samples[this.samples.length - 1];
    return (last - first) / first;
  }

  stop() {
    if (this.interval) clearInterval(this.interval);
  }
}
```

**Database Query Profiling:**

```sql
-- PostgreSQL: Enable query logging
SET log_statement = 'all';
SET log_duration = on;

-- Analyze query plan
EXPLAIN ANALYZE SELECT * FROM users
WHERE created_at > '2024-01-01'
ORDER BY created_at DESC
LIMIT 100;

-- Find slow queries
SELECT
  query,
  calls,
  total_time / 1000 as total_seconds,
  mean_time / 1000 as mean_seconds,
  rows
FROM pg_stat_statements
ORDER BY total_time DESC
LIMIT 20;
```

```typescript
// Application-level query profiling
import { performance } from "perf_hooks";

const queryLogger = {
  queries: [] as Array<{ sql: string; duration: number; timestamp: Date }>,

  log(sql: string, duration: number) {
    this.queries.push({ sql, duration, timestamp: new Date() });
    if (duration > 100) {
      console.warn(`Slow query (${duration}ms): ${sql.substring(0, 100)}...`);
    }
  },

  getSlowQueries(threshold: number = 100) {
    return this.queries.filter((q) => q.duration > threshold);
  },

  getStats() {
    const durations = this.queries.map((q) => q.duration);
    return {
      count: durations.length,
      avg: durations.reduce((a, b) => a + b, 0) / durations.length,
      max: Math.max(...durations),
      p95: durations.sort((a, b) => a - b)[Math.floor(durations.length * 0.95)],
    };
  },
};
```

### 6. Track Key Metrics

**Essential Performance Metrics:**

| Metric        | Description             | Target    | Alert Threshold |
| ------------- | ----------------------- | --------- | --------------- |
| Latency (p50) | Median response time    | <100ms    | >200ms          |
| Latency (p95) | 95th percentile         | <500ms    | >1000ms         |
| Latency (p99) | 99th percentile         | <1000ms   | >2000ms         |
| Throughput    | Requests per second     | >1000 RPS | <500 RPS        |
| Error Rate    | Failed requests %       | <0.1%     | >1%             |
| Saturation    | Resource utilization    | <70%      | >85%            |
| Apdex         | User satisfaction score | >0.9      | <0.7            |

**Implementing Metrics Collection:**

```typescript
// metrics.ts
import { Counter, Histogram, Gauge, Registry } from "prom-client";

const register = new Registry();

// Request metrics
const httpRequestDuration = new Histogram({
  name: "http_request_duration_seconds",
  help: "Duration of HTTP requests in seconds",
  labelNames: ["method", "route", "status_code"],
  buckets: [0.01, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10],
  registers: [register],
});

const httpRequestTotal = new Counter({
  name: "http_requests_total",
  help: "Total number of HTTP requests",
  labelNames: ["method", "route", "status_code"],
  registers: [register],
});

const activeConnections = new Gauge({
  name: "active_connections",
  help: "Number of active connections",
  registers: [register],
});

// Middleware for Express
export function metricsMiddleware(
  req: Request,
  res: Response,
  next: NextFunction,
) {
  const start = process.hrtime.bigint();

  activeConnections.inc();

  res.on("finish", () => {
    const duration = Number(process.hrtime.bigint() - start) / 1e9;
    const route = req.route?.path || req.path;

    httpRequestDuration.observe(
      { method: req.method, route, status_code: res.statusCode },
      duration,
    );

    httpRequestTotal.inc({
      method: req.method,
      route,
      status_code: res.statusCode,
    });

    activeConnections.dec();
  });

  next();
}

// Metrics endpoint
app.get("/metrics", async (req, res) => {
  res.set("Content-Type", register.contentType);
  res.end(await register.metrics());
});
```

**Custom Business Metrics:**

```typescript
// business-metrics.ts
const orderProcessingTime = new Histogram({
  name: "order_processing_duration_seconds",
  help: "Time to process an order",
  labelNames: ["payment_method", "status"],
  buckets: [0.5, 1, 2, 5, 10, 30, 60],
});

const cartValue = new Histogram({
  name: "cart_value_dollars",
  help: "Shopping cart value at checkout",
  buckets: [10, 25, 50, 100, 250, 500, 1000],
});

const concurrentUsers = new Gauge({
  name: "concurrent_authenticated_users",
  help: "Number of currently authenticated users",
});
```

### 7. Define Performance Budgets

**Web Performance Budgets:**

```typescript
// performance-budget.ts
interface PerformanceBudget {
  metric: string;
  budget: number;
  unit: string;
}

const webBudgets: PerformanceBudget[] = [
  // Timing metrics
  { metric: "First Contentful Paint", budget: 1800, unit: "ms" },
  { metric: "Largest Contentful Paint", budget: 2500, unit: "ms" },
  { metric: "Time to Interactive", budget: 3800, unit: "ms" },
  { metric: "Total Blocking Time", budget: 300, unit: "ms" },
  { metric: "Cumulative Layout Shift", budget: 0.1, unit: "" },

  // Resource budgets
  { metric: "JavaScript bundle size", budget: 300, unit: "KB" },
  { metric: "CSS bundle size", budget: 100, unit: "KB" },
  { metric: "Total page weight", budget: 1500, unit: "KB" },
  { metric: "Image weight", budget: 500, unit: "KB" },

  // Request budgets
  { metric: "Total requests", budget: 50, unit: "requests" },
  { metric: "Third-party requests", budget: 10, unit: "requests" },
];
```

**Lighthouse CI Configuration:**

```javascript
// lighthouserc.js
module.exports = {
  ci: {
    collect: {
      url: ["http://localhost:3000/", "http://localhost:3000/products"],
      numberOfRuns: 3,
    },
    assert: {
      assertions: {
        "first-contentful-paint": ["error", { maxNumericValue: 1800 }],
        "largest-contentful-paint": ["error", { maxNumericValue: 2500 }],
        interactive: ["error", { maxNumericValue: 3800 }],
        "total-blocking-time": ["error", { maxNumericValue: 300 }],
        "cumulative-layout-shift": ["error", { maxNumericValue: 0.1 }],
        "resource-summary:script:size": ["error", { maxNumericValue: 300000 }],
        "resource-summary:total:size": ["error", { maxNumericValue: 1500000 }],
      },
    },
    upload: {
      target: "temporary-public-storage",
    },
  },
};
```

**API Performance Budgets:**

```yaml
# api-budgets.yml
endpoints:
  GET /api/products:
    p50_latency_ms: 50
    p95_latency_ms: 200
    p99_latency_ms: 500
    error_rate_percent: 0.1
    throughput_rps: 1000

  POST /api/orders:
    p50_latency_ms: 200
    p95_latency_ms: 800
    p99_latency_ms: 2000
    error_rate_percent: 0.01
    throughput_rps: 100

  GET /api/search:
    p50_latency_ms: 100
    p95_latency_ms: 500
    p99_latency_ms: 1500
    error_rate_percent: 0.5
    throughput_rps: 500
```

### 8. Identify and Resolve Bottlenecks

**Systematic Bottleneck Analysis:**

```typescript
// bottleneck-analyzer.ts
interface BottleneckReport {
  category: "cpu" | "memory" | "io" | "network" | "database";
  severity: "low" | "medium" | "high" | "critical";
  description: string;
  recommendation: string;
  metrics: Record<string, number>;
}

async function analyzeBottlenecks(): Promise<BottleneckReport[]> {
  const reports: BottleneckReport[] = [];

  // CPU analysis
  const cpuUsage = process.cpuUsage();
  if (cpuUsage.user / 1000000 > 80) {
    reports.push({
      category: "cpu",
      severity: "high",
      description: "High CPU utilization detected",
      recommendation: "Profile CPU usage, optimize hot paths, consider caching",
      metrics: { userCpuPercent: cpuUsage.user / 1000000 },
    });
  }

  // Memory analysis
  const memUsage = process.memoryUsage();
  const heapUsedPercent = (memUsage.heapUsed / memUsage.heapTotal) * 100;
  if (heapUsedPercent > 85) {
    reports.push({
      category: "memory",
      severity: "high",
      description: "High memory pressure detected",
      recommendation: "Check for memory leaks, reduce object retention",
      metrics: { heapUsedPercent, heapUsedMB: memUsage.heapUsed / 1024 / 1024 },
    });
  }

  // Event loop lag
  const lagStart = Date.now();
  await new Promise((resolve) => setImmediate(resolve));
  const eventLoopLag = Date.now() - lagStart;
  if (eventLoopLag > 100) {
    reports.push({
      category: "cpu",
      severity: "medium",
      description: "Event loop blocking detected",
      recommendation: "Move CPU-intensive work to worker threads",
      metrics: { eventLoopLagMs: eventLoopLag },
    });
  }

  return reports;
}
```

**Common Bottlenecks and Solutions:**

| Bottleneck       | Symptoms                     | Diagnosis                | Solution                          |
| ---------------- | ---------------------------- | ------------------------ | --------------------------------- |
| N+1 Queries      | Linear latency increase      | Query logging            | Eager loading, batching           |
| Missing Index    | Slow queries on large tables | EXPLAIN ANALYZE          | Add appropriate indexes           |
| Connection Pool  | Timeouts under load          | Pool metrics             | Increase pool size, add queueing  |
| Synchronous I/O  | High event loop lag          | Profiling                | Use async operations              |
| Memory Leak      | Growing heap over time       | Heap snapshots           | Fix object retention              |
| Unoptimized JSON | High CPU on serialization    | CPU profiling            | Stream parsing, schema validation |
| Large Payloads   | High network latency         | Response size monitoring | Pagination, compression           |

**Database Optimization Checklist:**

```sql
-- Check for missing indexes
SELECT
  schemaname, tablename,
  seq_scan, seq_tup_read,
  idx_scan, idx_tup_fetch
FROM pg_stat_user_tables
WHERE seq_scan > idx_scan
ORDER BY seq_tup_read DESC;

-- Check for slow queries
SELECT query, calls, total_time, mean_time, rows
FROM pg_stat_statements
ORDER BY mean_time DESC
LIMIT 20;

-- Check for lock contention
SELECT blocked_locks.pid AS blocked_pid,
       blocking_locks.pid AS blocking_pid,
       blocked_activity.query AS blocked_query
FROM pg_catalog.pg_locks blocked_locks
JOIN pg_catalog.pg_locks blocking_locks
  ON blocking_locks.locktype = blocked_locks.locktype
WHERE NOT blocked_locks.granted;
```

### 9. Test Data Generation and Realistic Load Patterns

**Generate Realistic Test Data:**

```typescript
// test-data-generator.ts
import { faker } from '@faker-js/faker';

interface TestUser {
  id: number;
  email: string;
  name: string;
  createdAt: Date;
  preferences: Record<string, any>;
}

function generateUsers(count: number): TestUser[] {
  return Array.from({ length: count }, (_, i) => ({
    id: i + 1,
    email: faker.internet.email(),
    name: faker.person.fullName(),
    createdAt: faker.date.past({ years: 2 }),
    preferences: {
      theme: faker.helpers.arrayElement(['light', 'dark']),
      notifications: faker.datatype.boolean(),
      language: faker.helpers.arrayElement(['en', 'es', 'fr', 'de']),
    },
  }));
}

// Generate data with realistic distributions
function generateOrdersWithDistribution(userCount: number, orderCount: number) {
  const users = generateUsers(userCount);
  const orders = [];

  // 80/20 rule: 20% of users make 80% of orders
  const powerUsers = users.slice(0, Math.floor(userCount * 0.2));
  const regularUsers = users.slice(Math.floor(userCount * 0.2));

  const powerUserOrders = Math.floor(orderCount * 0.8);
  const regularUserOrders = orderCount - powerUserOrders;

  // Power users
  for (let i = 0; i < powerUserOrders; i++) {
    const user = faker.helpers.arrayElement(powerUsers);
    orders.push(generateOrder(user.id, i + 1));
  }

  // Regular users
  for (let i = 0; i < regularUserOrders; i++) {
    const user = faker.helpers.arrayElement(regularUsers);
    orders.push(generateOrder(user.id, powerUserOrders + i + 1));
  }

  return { users, orders };
}

function generateOrder(userId: number, orderId: number) {
  const itemCount = faker.number.int({ min: 1, max: 10 });

  return {
    id: orderId,
    userId,
    items: Array.from({ length: itemCount }, () => ({
      productId: faker.number.int({ min: 1, max: 1000 }),
      quantity: faker.number.int({ min: 1, max: 5 }),
      price: parseFloat(faker.commerce.price()),
    })),
    status: faker.helpers.arrayElement(['pending', 'processing', 'shipped', 'delivered']),
    createdAt: faker.date.recent({ days: 90 }),
  };
}
```

**Realistic Traffic Patterns:**

```javascript
// k6 realistic traffic patterns
import http from 'k6/http';
import { sleep } from 'k6';

export const options = {
  scenarios: {
    // Morning traffic spike (9am)
    morning_spike: {
      executor: 'ramping-arrival-rate',
      startRate: 10,
      timeUnit: '1s',
      preAllocatedVUs: 50,
      maxVUs: 200,
      stages: [
        { duration: '5m', target: 50 },   // Ramp up
        { duration: '10m', target: 50 },  // Sustained
        { duration: '5m', target: 10 },   // Ramp down
      ],
      startTime: '0s',
    },
    // Lunch traffic (12pm)
    lunch_traffic: {
      executor: 'constant-arrival-rate',
      rate: 30,
      timeUnit: '1s',
      duration: '30m',
      preAllocatedVUs: 100,
      maxVUs: 150,
      startTime: '20m',
    },
    // Evening spike (6pm)
    evening_spike: {
      executor: 'ramping-arrival-rate',
      startRate: 10,
      timeUnit: '1s',
      preAllocatedVUs: 50,
      maxVUs: 300,
      stages: [
        { duration: '5m', target: 100 },
        { duration: '15m', target: 100 },
        { duration: '5m', target: 10 },
      ],
      startTime: '50m',
    },
    // Background jobs (constant low load)
    background_jobs: {
      executor: 'constant-vus',
      vus: 5,
      duration: '2h',
      exec: 'backgroundJob',
    },
  },
};

export default function() {
  // Simulate different user behaviors
  const userType = Math.random();

  if (userType < 0.6) {
    // 60% - Browsers (fast, many requests)
    http.get(`${__ENV.API_URL}/api/products`);
    sleep(0.5);
    http.get(`${__ENV.API_URL}/api/products/${Math.floor(Math.random() * 100)}`);
    sleep(0.5);
  } else if (userType < 0.9) {
    // 30% - Regular users (moderate pace)
    http.get(`${__ENV.API_URL}/api/products`);
    sleep(2);
    http.get(`${__ENV.API_URL}/api/cart`);
    sleep(3);
  } else {
    // 10% - Power users (complex operations)
    http.post(`${__ENV.API_URL}/api/orders`, JSON.stringify({
      items: [{ productId: 1, quantity: 1 }],
    }), {
      headers: { 'Content-Type': 'application/json' },
    });
    sleep(5);
  }
}

export function backgroundJob() {
  // Simulate cron jobs, workers
  http.post(`${__ENV.API_URL}/internal/process-batch`, JSON.stringify({
    batchId: Math.floor(Math.random() * 1000),
  }), {
    headers: { 'Content-Type': 'application/json' },
  });
  sleep(60);  // Every minute
}
```

**Think Time and User Behavior:**

```javascript
import { sleep } from 'k6';
import { randomIntBetween } from 'https://jslib.k6.io/k6-utils/1.2.0/index.js';

// Human-like think time
function thinkTime() {
  // Normal distribution around 2 seconds
  const mean = 2;
  const stdDev = 0.5;
  const time = mean + stdDev * (Math.random() + Math.random() + Math.random() - 1.5);
  sleep(Math.max(0.5, time));
}

// Simulate user session
export default function() {
  // Login
  http.post(`${__ENV.API_URL}/auth/login`, /* ... */);
  thinkTime();

  // Browse products (3-7 pages)
  const pageViews = randomIntBetween(3, 7);
  for (let i = 0; i < pageViews; i++) {
    http.get(`${__ENV.API_URL}/api/products?page=${i}`);
    thinkTime();
  }

  // 30% add to cart
  if (Math.random() < 0.3) {
    http.post(`${__ENV.API_URL}/api/cart`, /* ... */);
    thinkTime();

    // 50% of those who add to cart complete checkout
    if (Math.random() < 0.5) {
      http.post(`${__ENV.API_URL}/api/orders`, /* ... */);
      sleep(3);  // Checkout takes longer
    }
  }

  // Logout (20% of users explicitly logout)
  if (Math.random() < 0.2) {
    http.post(`${__ENV.API_URL}/auth/logout`);
  }
}
```

**Edge Cases and Error Scenarios:**

```javascript
import http from 'k6/http';
import { check } from 'k6';

export default function() {
  const scenarios = [
    // Happy path (70%)
    () => {
      const res = http.get(`${__ENV.API_URL}/api/products`);
      check(res, { 'status is 200': (r) => r.status === 200 });
    },
    // Large payload (10%)
    () => {
      const res = http.get(`${__ENV.API_URL}/api/products?limit=1000`);
      check(res, { 'handles large response': (r) => r.status === 200 });
    },
    // Invalid input (10%)
    () => {
      const res = http.get(`${__ENV.API_URL}/api/products/-1`);
      check(res, { 'handles invalid ID': (r) => r.status === 400 });
    },
    // Not found (5%)
    () => {
      const res = http.get(`${__ENV.API_URL}/api/products/999999`);
      check(res, { 'handles not found': (r) => r.status === 404 });
    },
    // Timeout scenario (3%)
    () => {
      const res = http.get(`${__ENV.API_URL}/api/slow-endpoint`, {
        timeout: '5s',
      });
      check(res, { 'handles timeout': (r) => r.status === 200 || r.error });
    },
    // Unauthorized (2%)
    () => {
      const res = http.get(`${__ENV.API_URL}/api/admin/users`);
      check(res, { 'enforces auth': (r) => r.status === 401 || r.status === 403 });
    },
  ];

  // Weighted random scenario selection
  const weights = [0.7, 0.8, 0.9, 0.95, 0.98, 1.0];
  const random = Math.random();

  for (let i = 0; i < weights.length; i++) {
    if (random < weights[i]) {
      scenarios[i]();
      break;
    }
  }
}
```

## Best Practices

### Test Environment and Setup

1. **Test in Production-like Environments**
   - Match hardware specifications (CPU, RAM, disk I/O)
   - Use realistic data volumes (production-scale databases)
   - Simulate actual traffic patterns and user distributions
   - Include network latency if testing distributed systems
   - Test with production-like configuration (caching, CDN, load balancers)

2. **Establish Baselines First**
   - Measure current performance before any optimization
   - Document baseline metrics for all critical endpoints
   - Track changes over time to detect regressions
   - Create performance baseline reports for stakeholder communication

3. **Use Realistic Test Data**
   - Volume should match production scale (not just small samples)
   - Include edge cases (large records, unicode, special characters, malformed data)
   - Test with cold and warm caches to measure both scenarios
   - Apply realistic data distributions (80/20 rule, Pareto principle)
   - Include timezone, locale, and internationalization variations

### Test Execution Strategy

4. **Test Early and Often**
   - Include performance tests in CI/CD pipeline
   - Run smoke tests on every commit (quick baseline check)
   - Run full load tests nightly or on pull requests
   - Catch regressions before deployment, not in production
   - Monitor trends, not just pass/fail thresholds

5. **Implement Progressive Load Testing**
   - Start with smoke tests (minimal load, validate functionality)
   - Progress to load tests (expected normal load)
   - Execute stress tests (find breaking points)
   - Run spike tests (validate autoscaling and recovery)
   - Finish with soak tests (detect memory leaks and degradation over time)

6. **Test Critical User Journeys**
   - Identify top 3-5 most important user flows
   - Prioritize testing revenue-generating paths
   - Include authentication, payment, and checkout flows
   - Test admin and privileged operations separately
   - Validate graceful degradation under partial failures

### Analysis and Reporting

7. **Analyze Percentiles, Not Averages**
   - Always report p50, p95, p99 latencies (not just average)
   - Use p95/p99 for SLOs and alerting thresholds
   - Understand that outliers matter for user experience
   - Track maximum latency to identify worst-case scenarios

8. **Monitor in Production**
   - Performance testing complements, not replaces production monitoring
   - Use APM tools (Datadog, New Relic, etc.) for real user metrics
   - Implement synthetic monitoring for critical paths
   - Alert on performance degradation before users complain
   - Correlate load test results with production behavior

9. **Document and Share Results**
   - Keep performance test reports in version control
   - Create visual dashboards for trend analysis
   - Share findings with team in regular reviews
   - Track optimization improvements with before/after metrics
   - Document infrastructure changes and their performance impact

### Optimization and Iteration

10. **Follow Scientific Method**
    - Form hypothesis before optimization ("I think X is slow because Y")
    - Change one variable at a time
    - Measure impact with load tests before and after
    - Document failed attempts (what didn't work and why)
    - Validate optimizations don't break functionality

11. **Set and Enforce Performance Budgets**
    - Define acceptable latency targets per endpoint
    - Set throughput requirements (RPS, concurrent users)
    - Establish error rate thresholds
    - Block deployments that violate budgets
    - Review and adjust budgets quarterly based on business needs

12. **Test Database Performance Separately**
    - Isolate database bottlenecks from application issues
    - Benchmark queries under load (not just in dev)
    - Test connection pool exhaustion scenarios
    - Validate index effectiveness with production data volumes
    - Monitor slow query logs during load tests

## Examples

### Example: Complete k6 Load Test Suite

```javascript
// k6/scenarios/api-load-test.js
import http from "k6/http";
import { check, group, sleep } from "k6";
import { Rate, Trend, Counter } from "k6/metrics";

// Custom metrics
const errorRate = new Rate("errors");
const orderLatency = new Trend("order_latency");
const ordersCreated = new Counter("orders_created");

// Test configuration
export const options = {
  scenarios: {
    // Constant load for baseline
    baseline: {
      executor: "constant-vus",
      vus: 10,
      duration: "5m",
      tags: { scenario: "baseline" },
    },
    // Ramping load for stress test
    stress: {
      executor: "ramping-vus",
      startVUs: 0,
      stages: [
        { duration: "2m", target: 50 },
        { duration: "5m", target: 50 },
        { duration: "2m", target: 100 },
        { duration: "5m", target: 100 },
        { duration: "2m", target: 0 },
      ],
      startTime: "5m",
      tags: { scenario: "stress" },
    },
    // Spike test
    spike: {
      executor: "ramping-vus",
      startVUs: 0,
      stages: [
        { duration: "10s", target: 200 },
        { duration: "1m", target: 200 },
        { duration: "10s", target: 0 },
      ],
      startTime: "20m",
      tags: { scenario: "spike" },
    },
  },
  thresholds: {
    http_req_duration: ["p(95)<500", "p(99)<1500"],
    errors: ["rate<0.01"],
    order_latency: ["p(95)<2000"],
  },
};

const BASE_URL = __ENV.BASE_URL || "http://localhost:3000";

export function setup() {
  // Login and get auth token
  const loginRes = http.post(
    `${BASE_URL}/api/auth/login`,
    JSON.stringify({
      email: "loadtest@example.com",
      password: "loadtest123",
    }),
    {
      headers: { "Content-Type": "application/json" },
    },
  );

  return { token: loginRes.json("token") };
}

export default function (data) {
  const headers = {
    "Content-Type": "application/json",
    Authorization: `Bearer ${data.token}`,
  };

  group("Browse Products", () => {
    const productsRes = http.get(`${BASE_URL}/api/products`, { headers });
    check(productsRes, {
      "products status 200": (r) => r.status === 200,
      "products returned": (r) => r.json("data").length > 0,
    });
    errorRate.add(productsRes.status !== 200);
    sleep(1);
  });

  group("View Product Detail", () => {
    const productRes = http.get(`${BASE_URL}/api/products/1`, { headers });
    check(productRes, {
      "product status 200": (r) => r.status === 200,
    });
    errorRate.add(productRes.status !== 200);
    sleep(0.5);
  });

  group("Create Order", () => {
    const start = Date.now();
    const orderRes = http.post(
      `${BASE_URL}/api/orders`,
      JSON.stringify({
        items: [{ productId: 1, quantity: 1 }],
      }),
      { headers },
    );

    orderLatency.add(Date.now() - start);

    const success = check(orderRes, {
      "order status 201": (r) => r.status === 201,
      "order has id": (r) => r.json("id") !== undefined,
    });

    if (success) ordersCreated.add(1);
    errorRate.add(!success);
    sleep(2);
  });
}

export function teardown(data) {
  // Cleanup test data if needed
  console.log("Test completed");
}
```

### Example: Performance Monitoring Dashboard Config

```yaml
# grafana/dashboards/performance.json (simplified)
panels:
  - title: "Request Latency (p95)"
    type: graph
    targets:
      - expr: histogram_quantile(0.95, rate(http_request_duration_seconds_bucket[5m]))

  - title: "Throughput (RPS)"
    type: graph
    targets:
      - expr: rate(http_requests_total[1m])

  - title: "Error Rate"
    type: graph
    targets:
      - expr: rate(http_requests_total{status_code=~"5.."}[5m]) / rate(http_requests_total[5m])

  - title: "Active Connections"
    type: gauge
    targets:
      - expr: active_connections

alerts:
  - name: HighLatency
    expr: histogram_quantile(0.95, rate(http_request_duration_seconds_bucket[5m])) > 0.5
    for: 5m
    labels:
      severity: warning

  - name: HighErrorRate
    expr: rate(http_requests_total{status_code=~"5.."}[5m]) / rate(http_requests_total[5m]) > 0.01
    for: 2m
    labels:
      severity: critical
```
