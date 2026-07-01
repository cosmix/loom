---
name: loom-rate-limiting
description: API rate limiting and quota management. Use when implementing request throttling, API quotas, backpressure handling, or abuse protection. Covers token bucket, leaky bucket, sliding/fixed window algorithms, and distributed rate limiting with Redis.
triggers:
  - rate limiting
  - rate limit
  - throttle
  - throttling
  - token bucket
  - leaky bucket
  - sliding window
  - fixed window
  - quota
  - 429
  - too many requests
  - DDoS
  - abuse prevention
  - API quota
  - burst
  - Redis rate limiting
  - distributed rate limiting
  - API gateway
  - per-user limits
  - per-IP limits
  - concurrent requests
  - request limiting
---

# Rate Limiting

## Overview

Control the request rate a client can make: protect from abuse, enforce fair usage, shed load. Two decisions dominate correctness: **which algorithm** (burst tolerance vs accuracy vs memory) and **how to make the counter atomic** in a distributed setting. Everything else is headers and policy.

## Algorithm Selection

| Algorithm | Burst behavior | Accuracy | Memory/key | Use when |
| --- | --- | --- | --- | --- |
| **Fixed window** | Allows 2× limit at window boundary | Poor | 1 counter | Cheap, coarse limits where boundary burst is acceptable |
| **Sliding window log** | Exact, no boundary burst | Exact | O(limit) timestamps | Low limits needing precision (e.g. 5 login attempts) |
| **Sliding window counter** | Smooths boundary, small over/under | ~99% | 2 counters | General-purpose distributed limiting (best default) |
| **Token bucket** | Allows configurable burst up to capacity | Rate-exact avg | 2 numbers (tokens, ts) | APIs that should tolerate bursts (most public APIs) |
| **Leaky bucket** | No burst; smooths to constant output | Shapes traffic | Queue | Protecting a fragile downstream at fixed throughput |
| **GCRA** | Burst = capacity, single value | Exact | 1 timestamp (TAT) | High-throughput distributed limiting; token-bucket equivalent, cheaper |

⚠ **Fixed-window boundary burst** is the classic footgun: with limit=100/min, a client can send 100 at 00:59.9 and 100 at 01:00.1 — 200 requests in ~0.2s while never violating either window. If bursts matter, use sliding-window or token-bucket.

⚠ **Token bucket ≈ leaky bucket (as a meter) ≈ GCRA** — mathematically equivalent rate meters differing in burst allowance and storage. Don't reimplement all three; pick token bucket for app code, GCRA for a single-value distributed limiter.

### Token Bucket

Allows bursts up to `capacity`, refills at `refillRate` tokens/sec. Refill is computed lazily on access (no background timer needed).

```typescript
class TokenBucket {
  private tokens: number;
  private lastRefill = Date.now();
  constructor(private capacity: number, private refillRate: number) {
    this.tokens = capacity;
  }
  consume(n = 1): boolean {
    const now = Date.now();
    this.tokens = Math.min(this.capacity, this.tokens + ((now - this.lastRefill) / 1000) * this.refillRate);
    this.lastRefill = now;
    if (this.tokens >= n) { this.tokens -= n; return true; }
    return false;
  }
}
// 100 req/min sustained, burst of 10:
const bucket = new TokenBucket(10, 100 / 60);
```

- `capacity` = max burst; `refillRate` = sustained rate. These are independent knobs — that's the point.
- In-process instance is per-node only. For multi-node, store `{tokens, lastRefill}` in Redis and refill inside a Lua script (below).

### Sliding Window Log vs Counter

**Log** keeps every timestamp in the window — exact, but memory grows with the limit and it's the heaviest to store/GC. **Counter** keeps the current + previous window counts and interpolates:

```typescript
// weighted = prevCount * (1 - elapsedIntoCurrentWindow) + currentCount
const weighted = prev * (1 - progress) + curr;
if (weighted < limit) { curr++; /* allow */ }
```

The counter is the pragmatic distributed default: 2 integers/key, no boundary burst, ~99% accurate. Use the log only when the limit is small and exactness is required (auth attempts, payment retries).

### Leaky Bucket

Queue requests, drain at a fixed rate; reject when the queue is full. Use to **shape** traffic into a fragile downstream, not to meter clients. Downside: adds latency (requests wait in queue) and needs a real queue/worker — don't reach for it unless constant output rate is the actual requirement.

## Distributed Rate Limiting (the hard part)

The naive distributed limiter is **broken by a race**:

```typescript
const count = await redis.incr(key);   // node A and B both read/return 1... 100
if (count === 1) await redis.expire(key, 60);  // ⚠ two problems below
```

⚠ **Two real bugs in the INCR-then-EXPIRE pattern:**

1. **Lost TTL** — if the process crashes (or the connection drops) between `INCR` and `EXPIRE`, the key is created with **no expiry** and the client is rate-limited *forever*. Always set expiry atomically.
2. **TTL reset / sliding drift** — calling `EXPIRE` on every request (not just `count===1`) turns a fixed window into an accidental sliding one and can let counts never expire under sustained load.

**Fix: do it in one atomic Lua script.** Redis executes scripts atomically, eliminating the read-modify-write race across nodes.

```typescript
// Sliding-window-log limiter, atomic. Returns [allowed, remaining, resetAtMs].
const LUA = `
  local key = KEYS[1]
  local now, window_start, limit, window_s = tonumber(ARGV[1]), tonumber(ARGV[2]), tonumber(ARGV[3]), tonumber(ARGV[4])
  redis.call('ZREMRANGEBYSCORE', key, '-inf', window_start)
  local count = redis.call('ZCARD', key)
  if count < limit then
    redis.call('ZADD', key, now, now .. '-' .. math.random())
    redis.call('EXPIRE', key, window_s)
    return {1, limit - count - 1}
  end
  local oldest = redis.call('ZRANGE', key, 0, 0, 'WITHSCORES')
  local reset = oldest[2] and (oldest[2] + window_s * 1000) or (now + window_s * 1000)
  return {0, 0, reset}
`;
const [allowed, remaining, resetAt] = await redis.eval(
  LUA, 1, `ratelimit:${id}`, Date.now(), Date.now() - windowS * 1000, limit, windowS,
);
```

⚠ **Sorted-set log gotcha:** `now .. '-' .. math.random()` is the member; two requests in the same millisecond need distinct members or one silently overwrites the other. Prefer a monotonic counter or a request UUID over `math.random()` for high concurrency.

**GCRA** (Generic Cell Rate Algorithm) is the storage-cheapest exact limiter: store a single `theoretical arrival time` (TAT) per key, updated atomically. This is what `redis-cell` (the `CL.THROTTLE` module command) and many library limiters implement — reach for it at high key cardinality where storing timestamp sets is too expensive.

### Redis Cluster

Multi-key operations (including a Lua script touching >1 key) must resolve to **one slot**. Use hash tags — the substring in `{...}` is what's hashed:

```typescript
const key = `{ratelimit:${userId}}:counter`; // all keys for this user → same slot
```

Without the tag, `EVAL` across a user's minute+hour keys throws `CROSSSLOT`.

### Fail-open vs Fail-closed

When the limiter backend (Redis) is **down**, you must choose:

- **Fail-open** (allow) — availability over protection. Correct for most public APIs: a limiter outage shouldn't take down the whole API. Risk: no protection during the outage.
- **Fail-closed** (deny) — protection over availability. Correct for abuse-critical or cost-critical paths (login, payment, expensive LLM calls) where an unmetered flood is worse than downtime.

Decide **per endpoint**, log every fallback, and add a local in-process fallback limiter so fail-open still has *some* ceiling. `console.error` the Redis failure — a silent catch that always `next()`s is an unmonitored open door.

## Quotas (multi-window)

Tiered plans typically enforce several windows at once (per-minute burst + per-day quota). Check the **coarsest/cheapest first isn't right — check the one most likely to reject first**, but always increment all atomically to avoid partial counting:

```typescript
// Per-tier: enforce minute AND day. Increment both, then evaluate.
const p = redis.pipeline();
p.incr(minKey); p.expire(minKey, 60);
p.incr(dayKey); p.expire(dayKey, 86400);
const [[, min], , [, day]] = await p.exec();
if (min > tier.perMinute || day > tier.perDay) return { allowed: false };
```

⚠ Pipeline is **not** atomic (commands can interleave with other clients). For strict multi-window correctness use one Lua script; a pipeline is usually fine for quotas where small over-count is acceptable.

### Per-key layering (IP + user)

Check **IP limits first** (DDoS / pre-auth abuse), then user/anonymous limits. An unauthenticated flood should die at the IP gate before touching per-user logic. Key hierarchy: `ip → api-key → user → global`. Apply the *most restrictive* that matches.

## The 429 Contract

Return `429 Too Many Requests` with headers so clients can self-throttle. There are two header families — emit both during the migration period:

```typescript
res.setHeader("RateLimit-Limit", limit);          // draft IETF (draft-ietf-httpapi-ratelimit-headers)
res.setHeader("RateLimit-Remaining", remaining);
res.setHeader("RateLimit-Reset", secondsUntilReset); // delta-seconds in the draft
res.setHeader("X-RateLimit-Limit", limit);        // de-facto legacy (many clients still read these)
res.setHeader("X-RateLimit-Remaining", remaining);
res.setHeader("X-RateLimit-Reset", unixTimestamp); // legacy uses absolute unix ts
res.setHeader("Retry-After", secondsUntilReset);  // ⚠ REQUIRED on 429; seconds or HTTP-date
```

⚠ **`Reset` ambiguity:** the IETF draft uses **delta-seconds**; the legacy `X-RateLimit-Reset` convention often uses an **absolute Unix timestamp**. Clients get this wrong constantly — document which you emit and be consistent. `Retry-After` is the unambiguous, standardized one; always send it on a 429.

⚠ Set rate-limit headers on **successful** responses too (so clients see `Remaining` drop and back off *before* hitting 429), not only on the 429.

### Express middleware shape

```typescript
function rateLimiter(opts: { windowMs: number; max: number; keyGen?: (r) => string; skip?: (r) => boolean }) {
  return async (req, res, next) => {
    if (opts.skip?.(req)) return next();
    const key = opts.keyGen?.(req) ?? req.ip;
    let r;
    try { r = await limiter.isAllowed(key, opts.max, opts.windowMs / 1000); }
    catch (e) { console.error("ratelimit backend down", e); return next(); } // fail-open, logged
    res.setHeader("RateLimit-Limit", opts.max);
    res.setHeader("RateLimit-Remaining", r.remaining);
    res.setHeader("RateLimit-Reset", Math.ceil((r.resetAt - Date.now()) / 1000));
    if (!r.allowed) {
      res.setHeader("Retry-After", Math.ceil((r.resetAt - Date.now()) / 1000));
      return res.status(429).json({ error: "RATE_LIMIT_EXCEEDED", retryAfter: ... });
    }
    next();
  };
}
```

⚠ **`req.ip` behind a proxy/LB is the proxy's IP** unless you set `app.set('trust proxy', ...)` correctly. Get this wrong and you either rate-limit the whole world as one IP, or trust a spoofable `X-Forwarded-For`. Trust only your own proxy hops.

## Client-Side Handling

- On 429, honor `Retry-After` exactly; do **not** immediately retry (that's the abuse the server is defending against).
- For non-429 errors use exponential backoff **with jitter** (`base * 2^n * random()`) — synchronized retries from many clients recreate the thundering herd.
- Track `RateLimit-Remaining` and pre-emptively slow down before hitting 0.

## Gateway/Infra Options

Prefer offloading to the edge when a gateway already fronts your services:

- **Nginx** — `limit_req_zone` + `limit_req ... burst=N nodelay`. `burst` without `nodelay` queues (leaky-bucket-like, adds latency); with `nodelay` allows the burst immediately then enforces rate. Per-node only unless fronted by shared state.
- **Kong** — `rate-limiting` plugin, `policy: redis` for cluster-wide counting; `fault_tolerant: true` = fail-open.
- **Envoy** — global RLS via external gRPC ratelimit service; `failure_mode_deny: false` = fail-open.
- **AWS API Gateway** — account/stage `throttlingRateLimit`+`throttlingBurstLimit` (token bucket) and per-key **usage plans** with `quota` (month/week/day). Note the account-level default (10k rps) can throttle before your per-method limits.

Edge limiting stops abuse before it costs you app compute; app-level limiting gives per-user/business-logic granularity. Real systems use both.

## Advanced Patterns

- **Adaptive limiting** — adjust the limit from observed success rate (AIMD: multiplicatively decrease on errors, additively/slowly increase when healthy). Backpressure that reacts to actual downstream health.
- **Priority shedding** — under high load, reject low-priority classes first (batch/background) while protecting critical (health, auth). Combine with a per-class token bucket.
- **Circuit breaker** — orthogonal to rate limiting: opens on *downstream failures* (not client rate) to stop hammering a broken dependency. Compose them; don't conflate them.

## Gotchas Checklist

- [ ] Limiter is **atomic** (Lua/GCRA), not read-then-write across nodes — no lost-TTL, no cross-node race
- [ ] Chosen algorithm matches burst policy (fixed-window boundary burst understood/accepted)
- [ ] Fail-open vs fail-closed decided **per endpoint**, backend failures logged, not silently swallowed
- [ ] `Retry-After` sent on every 429; `RateLimit-*` sent on success responses too
- [ ] `Reset` semantics (delta-seconds vs absolute) documented and consistent
- [ ] `trust proxy` configured; client IP is the real client, not the LB, and `X-Forwarded-For` isn't blindly trusted
- [ ] Redis keys have TTLs; Cluster deployments use hash tags for multi-key/Lua ops
- [ ] Client retries use `Retry-After` + jittered backoff, not tight-loop retry
- [ ] Multi-window quotas incremented together (Lua) or accept small over-count (pipeline)
- [ ] Limits load-tested at the window boundary and at Redis-down (both failure modes exercised)
