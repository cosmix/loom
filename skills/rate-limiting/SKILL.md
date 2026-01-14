---
name: rate-limiting
description: API rate limiting and quota management implementation. Use when implementing request throttling, API quotas, backpressure handling, or protection against abuse. Keywords: rate limiting, rate limit, throttle, throttling, token bucket, leaky bucket, sliding window, fixed window, quota, 429, too many requests, DDoS, abuse prevention, API quota, burst, Redis rate limiting, distributed rate limiting, API gateway, per-user limits, per-IP limits, concurrent requests, request limiting.
---

# Rate Limiting

## Overview

Rate limiting is a technique to control the rate of requests a client can make to an API. It protects services from abuse, ensures fair usage, and maintains system stability. This skill covers algorithms, implementation patterns, and best practices for distributed rate limiting.

## Key Concepts

### Rate Limiting Algorithms

**Token Bucket Algorithm:**

The token bucket allows bursts while maintaining an average rate.

```typescript
class TokenBucket {
  private tokens: number;
  private lastRefill: number;

  constructor(
    private capacity: number, // Maximum tokens
    private refillRate: number, // Tokens per second
  ) {
    this.tokens = capacity;
    this.lastRefill = Date.now();
  }

  private refill(): void {
    const now = Date.now();
    const elapsed = (now - this.lastRefill) / 1000;
    const tokensToAdd = elapsed * this.refillRate;

    this.tokens = Math.min(this.capacity, this.tokens + tokensToAdd);
    this.lastRefill = now;
  }

  consume(tokens: number = 1): boolean {
    this.refill();

    if (this.tokens >= tokens) {
      this.tokens -= tokens;
      return true;
    }
    return false;
  }

  getState(): { tokens: number; capacity: number } {
    this.refill();
    return { tokens: this.tokens, capacity: this.capacity };
  }
}

// Usage: 100 requests/minute with burst of 10
const bucket = new TokenBucket(10, 100 / 60);
```

**Sliding Window Log Algorithm:**

Precise rate limiting by tracking individual request timestamps.

```typescript
class SlidingWindowLog {
  private requests: number[] = [];

  constructor(
    private windowMs: number, // Window size in milliseconds
    private maxRequests: number, // Max requests per window
  ) {}

  isAllowed(): boolean {
    const now = Date.now();
    const windowStart = now - this.windowMs;

    // Remove expired entries
    this.requests = this.requests.filter((ts) => ts > windowStart);

    if (this.requests.length < this.maxRequests) {
      this.requests.push(now);
      return true;
    }

    return false;
  }

  getRemainingRequests(): number {
    const now = Date.now();
    const windowStart = now - this.windowMs;
    this.requests = this.requests.filter((ts) => ts > windowStart);
    return Math.max(0, this.maxRequests - this.requests.length);
  }

  getResetTime(): number {
    if (this.requests.length === 0) return 0;
    return this.requests[0] + this.windowMs;
  }
}
```

**Sliding Window Counter Algorithm:**

Memory-efficient approximation using weighted counters.

```typescript
class SlidingWindowCounter {
  private previousCount: number = 0;
  private currentCount: number = 0;
  private windowStart: number;

  constructor(
    private windowMs: number,
    private maxRequests: number,
  ) {
    this.windowStart = Date.now();
  }

  isAllowed(): boolean {
    const now = Date.now();
    const elapsed = now - this.windowStart;

    // Check if we've moved to a new window
    if (elapsed >= this.windowMs) {
      const windowsPassed = Math.floor(elapsed / this.windowMs);
      if (windowsPassed === 1) {
        this.previousCount = this.currentCount;
      } else {
        this.previousCount = 0;
      }
      this.currentCount = 0;
      this.windowStart = now - (elapsed % this.windowMs);
    }

    // Calculate weighted count
    const windowProgress = (now - this.windowStart) / this.windowMs;
    const weightedCount =
      this.previousCount * (1 - windowProgress) + this.currentCount;

    if (weightedCount < this.maxRequests) {
      this.currentCount++;
      return true;
    }

    return false;
  }
}
```

**Leaky Bucket Algorithm:**

Smooths out bursts by processing requests at a constant rate.

```typescript
class LeakyBucket {
  private queue: Array<() => void> = [];
  private processing: boolean = false;

  constructor(
    private capacity: number, // Queue size
    private leakRate: number, // Requests processed per second
  ) {}

  async add(request: () => Promise<void>): Promise<boolean> {
    if (this.queue.length >= this.capacity) {
      return false; // Queue full, reject
    }

    return new Promise((resolve) => {
      this.queue.push(async () => {
        await request();
        resolve(true);
      });

      this.processQueue();
    });
  }

  private async processQueue(): Promise<void> {
    if (this.processing || this.queue.length === 0) return;

    this.processing = true;

    while (this.queue.length > 0) {
      const request = this.queue.shift();
      if (request) {
        await request();
        await this.delay(1000 / this.leakRate);
      }
    }

    this.processing = false;
  }

  private delay(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }
}
```

### API Quota Management

**Tiered Quota System:**

```typescript
interface QuotaTier {
  name: string;
  limits: {
    requestsPerMinute: number;
    requestsPerDay: number;
    burstSize: number;
  };
  features: string[];
}

const quotaTiers: Record<string, QuotaTier> = {
  free: {
    name: "Free",
    limits: {
      requestsPerMinute: 10,
      requestsPerDay: 1000,
      burstSize: 5,
    },
    features: ["basic-api"],
  },
  pro: {
    name: "Professional",
    limits: {
      requestsPerMinute: 100,
      requestsPerDay: 50000,
      burstSize: 20,
    },
    features: ["basic-api", "advanced-api", "webhooks"],
  },
  enterprise: {
    name: "Enterprise",
    limits: {
      requestsPerMinute: 1000,
      requestsPerDay: 1000000,
      burstSize: 100,
    },
    features: ["basic-api", "advanced-api", "webhooks", "bulk-api"],
  },
};

class QuotaManager {
  constructor(private redis: Redis) {}

  async checkQuota(
    userId: string,
    tier: string,
  ): Promise<{ allowed: boolean; remaining: number; resetAt: number }> {
    const config = quotaTiers[tier];
    if (!config) throw new Error(`Unknown tier: ${tier}`);

    const minuteKey = `quota:${userId}:minute`;
    const dayKey = `quota:${userId}:day`;

    const [minuteCount, dayCount] = await Promise.all([
      this.redis.get(minuteKey),
      this.redis.get(dayKey),
    ]);

    const currentMinute = parseInt(minuteCount || "0");
    const currentDay = parseInt(dayCount || "0");

    if (currentMinute >= config.limits.requestsPerMinute) {
      const ttl = await this.redis.ttl(minuteKey);
      return { allowed: false, remaining: 0, resetAt: Date.now() + ttl * 1000 };
    }

    if (currentDay >= config.limits.requestsPerDay) {
      const ttl = await this.redis.ttl(dayKey);
      return { allowed: false, remaining: 0, resetAt: Date.now() + ttl * 1000 };
    }

    // Increment counters
    const pipeline = this.redis.pipeline();
    pipeline.incr(minuteKey);
    pipeline.expire(minuteKey, 60);
    pipeline.incr(dayKey);
    pipeline.expire(dayKey, 86400);
    await pipeline.exec();

    return {
      allowed: true,
      remaining: config.limits.requestsPerMinute - currentMinute - 1,
      resetAt: Date.now() + 60000,
    };
  }
}
```

### Per-User vs Per-IP Limiting

**Combined Strategy:**

```typescript
interface RateLimitConfig {
  authenticated: {
    requestsPerMinute: number;
    requestsPerHour: number;
  };
  anonymous: {
    requestsPerMinute: number;
    requestsPerHour: number;
  };
  ipBased: {
    requestsPerMinute: number;
    maxConnectionsPerIP: number;
  };
}

class HybridRateLimiter {
  constructor(
    private redis: Redis,
    private config: RateLimitConfig,
  ) {}

  async check(
    ip: string,
    userId?: string,
  ): Promise<{ allowed: boolean; retryAfter?: number }> {
    // Always check IP-based limits first (DDoS protection)
    const ipResult = await this.checkIPLimit(ip);
    if (!ipResult.allowed) {
      return ipResult;
    }

    // Then check user or anonymous limits
    if (userId) {
      return this.checkUserLimit(userId);
    } else {
      return this.checkAnonymousLimit(ip);
    }
  }

  private async checkIPLimit(
    ip: string,
  ): Promise<{ allowed: boolean; retryAfter?: number }> {
    const key = `ratelimit:ip:${ip}`;
    const count = await this.redis.incr(key);

    if (count === 1) {
      await this.redis.expire(key, 60);
    }

    if (count > this.config.ipBased.requestsPerMinute) {
      const ttl = await this.redis.ttl(key);
      return { allowed: false, retryAfter: ttl };
    }

    return { allowed: true };
  }

  private async checkUserLimit(
    userId: string,
  ): Promise<{ allowed: boolean; retryAfter?: number }> {
    const minuteKey = `ratelimit:user:${userId}:minute`;
    const hourKey = `ratelimit:user:${userId}:hour`;

    const [minuteCount, hourCount] = await Promise.all([
      this.incrementWithExpiry(minuteKey, 60),
      this.incrementWithExpiry(hourKey, 3600),
    ]);

    if (minuteCount > this.config.authenticated.requestsPerMinute) {
      const ttl = await this.redis.ttl(minuteKey);
      return { allowed: false, retryAfter: ttl };
    }

    if (hourCount > this.config.authenticated.requestsPerHour) {
      const ttl = await this.redis.ttl(hourKey);
      return { allowed: false, retryAfter: ttl };
    }

    return { allowed: true };
  }

  private async incrementWithExpiry(
    key: string,
    expiry: number,
  ): Promise<number> {
    const count = await this.redis.incr(key);
    if (count === 1) {
      await this.redis.expire(key, expiry);
    }
    return count;
  }
}
```

## Redis-Based Rate Limiting

Redis provides distributed rate limiting with atomic operations and high performance across multiple application instances.

### Distributed Rate Limiting with Redis

**Redis-Based Sliding Window:**

```typescript
import Redis from "ioredis";

class DistributedRateLimiter {
  private redis: Redis;

  constructor(redisUrl: string) {
    this.redis = new Redis(redisUrl);
  }

  async isAllowed(
    key: string,
    limit: number,
    windowSeconds: number,
  ): Promise<{ allowed: boolean; remaining: number; resetAt: number }> {
    const now = Date.now();
    const windowStart = now - windowSeconds * 1000;
    const redisKey = `ratelimit:${key}`;

    // Use Lua script for atomic operation
    const luaScript = `
      local key = KEYS[1]
      local now = tonumber(ARGV[1])
      local window_start = tonumber(ARGV[2])
      local limit = tonumber(ARGV[3])
      local window_seconds = tonumber(ARGV[4])

      -- Remove old entries
      redis.call('ZREMRANGEBYSCORE', key, '-inf', window_start)

      -- Count current entries
      local count = redis.call('ZCARD', key)

      if count < limit then
        -- Add new entry
        redis.call('ZADD', key, now, now .. '-' .. math.random())
        redis.call('EXPIRE', key, window_seconds)
        return {1, limit - count - 1}
      else
        -- Get oldest entry for reset time
        local oldest = redis.call('ZRANGE', key, 0, 0, 'WITHSCORES')
        local reset_at = oldest[2] and (oldest[2] + window_seconds * 1000) or (now + window_seconds * 1000)
        return {0, 0, reset_at}
      end
    `;

    const result = (await this.redis.eval(
      luaScript,
      1,
      redisKey,
      now.toString(),
      windowStart.toString(),
      limit.toString(),
      windowSeconds.toString(),
    )) as number[];

    return {
      allowed: result[0] === 1,
      remaining: result[1],
      resetAt: result[2] || now + windowSeconds * 1000,
    };
  }
}
```

**Redis Cluster Support:**

```typescript
class ClusterRateLimiter {
  private cluster: Redis.Cluster;

  constructor(nodes: { host: string; port: number }[]) {
    this.cluster = new Redis.Cluster(nodes, {
      redisOptions: {
        password: process.env.REDIS_PASSWORD,
      },
      scaleReads: "slave",
    });
  }

  async checkLimit(
    identifier: string,
    limit: number,
    windowMs: number,
  ): Promise<boolean> {
    // Use hash tags to ensure all keys for a user go to same slot
    const key = `{ratelimit:${identifier}}:counter`;

    const count = await this.cluster.incr(key);
    if (count === 1) {
      await this.cluster.pexpire(key, windowMs);
    }

    return count <= limit;
  }
}
```

## API Gateway Rate Limiting

API gateways provide centralized rate limiting across microservices.

### Kong Rate Limiting

```yaml
# Kong configuration
plugins:
  - name: rate-limiting
    config:
      minute: 100
      hour: 10000
      policy: redis
      redis_host: redis.example.com
      redis_port: 6379
      fault_tolerant: true
      hide_client_headers: false
```

### Nginx Rate Limiting

```nginx
# Nginx configuration
limit_req_zone $binary_remote_addr zone=api:10m rate=10r/s;
limit_req_zone $http_authorization zone=user:10m rate=100r/s;

server {
    location /api/ {
        # Burst allows 20 requests, then enforces rate
        limit_req zone=api burst=20 nodelay;
        limit_req_status 429;

        # Custom rate limit headers
        add_header X-RateLimit-Limit $limit_req_rate always;
        add_header X-RateLimit-Remaining $limit_req_remaining always;

        proxy_pass http://backend;
    }

    location /api/authenticated/ {
        limit_req zone=user burst=50;
        proxy_pass http://backend;
    }
}
```

### AWS API Gateway Throttling

```typescript
// AWS CDK configuration
import * as apigateway from 'aws-cdk-lib/aws-apigateway';

const api = new apigateway.RestApi(this, 'MyApi', {
  deployOptions: {
    throttlingRateLimit: 1000,  // Requests per second
    throttlingBurstLimit: 2000, // Burst capacity
  },
});

// Per-method throttling
const resource = api.root.addResource('users');
resource.addMethod('GET', integration, {
  methodResponses: [
    { statusCode: '200' },
    { statusCode: '429' },
  ],
  throttling: {
    rateLimit: 100,
    burstLimit: 200,
  },
});

// Per-client API key throttling
const plan = api.addUsagePlan('BasicPlan', {
  throttle: {
    rateLimit: 10,
    burstLimit: 20,
  },
  quota: {
    limit: 10000,
    period: apigateway.Period.MONTH,
  },
});
```

### Envoy Rate Limiting

```yaml
# Envoy proxy configuration
static_resources:
  listeners:
    - name: listener_0
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 8080
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                http_filters:
                  - name: envoy.filters.http.ratelimit
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.ratelimit.v3.RateLimit
                      domain: api_domain
                      rate_limit_service:
                        grpc_service:
                          envoy_grpc:
                            cluster_name: rate_limit_cluster
                      failure_mode_deny: false
```

## Distributed Systems Considerations

### Multi-Region Rate Limiting

**Global Rate Limiter with Redis Cluster:**

```typescript
class GlobalRateLimiter {
  private regions: Map<string, Redis.Cluster>;
  private localCache: Map<string, { count: number; expires: number }>;

  constructor(redisConfig: Record<string, Redis.ClusterNode[]>) {
    this.regions = new Map();
    this.localCache = new Map();

    for (const [region, nodes] of Object.entries(redisConfig)) {
      this.regions.set(region, new Redis.Cluster(nodes));
    }
  }

  async checkLimit(
    userId: string,
    limit: number,
    windowSeconds: number,
  ): Promise<boolean> {
    // Check local cache first (reduces Redis calls)
    const cached = this.localCache.get(userId);
    if (cached && cached.expires > Date.now()) {
      if (cached.count >= limit) {
        return false;
      }
      cached.count++;
      return true;
    }

    // Aggregate counts across all regions
    const promises = Array.from(this.regions.values()).map((redis) =>
      redis.get(`ratelimit:${userId}`).then((v) => parseInt(v || "0")),
    );

    const counts = await Promise.all(promises);
    const totalCount = counts.reduce((sum, c) => sum + c, 0);

    if (totalCount >= limit) {
      return false;
    }

    // Increment in current region
    const currentRegion = this.regions.get(process.env.REGION!)!;
    const key = `ratelimit:${userId}`;
    await currentRegion.incr(key);
    await currentRegion.expire(key, windowSeconds);

    // Update local cache
    this.localCache.set(userId, {
      count: totalCount + 1,
      expires: Date.now() + 1000, // Cache for 1 second
    });

    return true;
  }
}
```

### Eventual Consistency Trade-offs

Rate limiting in distributed systems involves trade-offs:

- **Strong Consistency**: Accurate limits, higher latency (global coordination)
- **Eventual Consistency**: Lower latency, potential over-limiting (regional independence)
- **Hybrid Approach**: Local caching with periodic synchronization

```typescript
interface RateLimitStrategy {
  consistency: 'strong' | 'eventual' | 'hybrid';
  tolerancePercent: number; // How much over-limit is acceptable
}

class HybridDistributedLimiter {
  private localCount: Map<string, number> = new Map();
  private globalSync: Redis;
  private syncInterval: number = 5000; // 5 seconds

  constructor(
    redis: Redis,
    private strategy: RateLimitStrategy,
  ) {
    this.globalSync = redis;
    this.startSyncLoop();
  }

  async checkLimit(userId: string, limit: number): Promise<boolean> {
    const local = this.localCount.get(userId) || 0;

    if (this.strategy.consistency === 'strong') {
      // Always check global state
      const global = await this.getGlobalCount(userId);
      return global < limit;
    }

    // Eventual consistency: allow local buffer
    const buffer = Math.ceil(limit * this.strategy.tolerancePercent / 100);
    const effectiveLimit = limit + buffer;

    if (local >= effectiveLimit) {
      // Over local buffer, check global
      const global = await this.getGlobalCount(userId);
      if (global >= limit) {
        return false;
      }
    }

    this.localCount.set(userId, local + 1);
    return true;
  }

  private async getGlobalCount(userId: string): Promise<number> {
    const count = await this.globalSync.get(`global:${userId}`);
    return parseInt(count || "0");
  }

  private startSyncLoop(): void {
    setInterval(async () => {
      for (const [userId, count] of this.localCount.entries()) {
        await this.globalSync.incrby(`global:${userId}`, count);
        this.localCount.set(userId, 0);
      }
    }, this.syncInterval);
  }
}
```

### Backpressure Patterns

**Circuit Breaker with Rate Limiting:**

```typescript
enum CircuitState {
  CLOSED = "CLOSED",
  OPEN = "OPEN",
  HALF_OPEN = "HALF_OPEN",
}

class CircuitBreaker {
  private state: CircuitState = CircuitState.CLOSED;
  private failures: number = 0;
  private lastFailure: number = 0;
  private successCount: number = 0;

  constructor(
    private failureThreshold: number = 5,
    private resetTimeout: number = 30000,
    private halfOpenSuccessThreshold: number = 3,
  ) {}

  async execute<T>(fn: () => Promise<T>): Promise<T> {
    if (this.state === CircuitState.OPEN) {
      if (Date.now() - this.lastFailure >= this.resetTimeout) {
        this.state = CircuitState.HALF_OPEN;
        this.successCount = 0;
      } else {
        throw new Error("Circuit breaker is OPEN");
      }
    }

    try {
      const result = await fn();
      this.onSuccess();
      return result;
    } catch (error) {
      this.onFailure();
      throw error;
    }
  }

  private onSuccess(): void {
    if (this.state === CircuitState.HALF_OPEN) {
      this.successCount++;
      if (this.successCount >= this.halfOpenSuccessThreshold) {
        this.state = CircuitState.CLOSED;
        this.failures = 0;
      }
    } else {
      this.failures = 0;
    }
  }

  private onFailure(): void {
    this.failures++;
    this.lastFailure = Date.now();

    if (this.failures >= this.failureThreshold) {
      this.state = CircuitState.OPEN;
    }
  }

  getState(): CircuitState {
    return this.state;
  }
}
```

**Adaptive Rate Limiting:**

```typescript
class AdaptiveRateLimiter {
  private currentLimit: number;
  private successRate: number = 1;
  private window: { success: boolean; timestamp: number }[] = [];

  constructor(
    private minLimit: number,
    private maxLimit: number,
    private targetSuccessRate: number = 0.95,
    private windowSize: number = 100,
  ) {
    this.currentLimit = maxLimit;
  }

  recordResult(success: boolean): void {
    const now = Date.now();
    this.window.push({ success, timestamp: now });

    // Keep window size manageable
    if (this.window.length > this.windowSize) {
      this.window.shift();
    }

    this.adjustLimit();
  }

  private adjustLimit(): void {
    if (this.window.length < 10) return;

    const successes = this.window.filter((r) => r.success).length;
    this.successRate = successes / this.window.length;

    if (this.successRate < this.targetSuccessRate) {
      // Reduce limit when success rate drops
      this.currentLimit = Math.max(
        this.minLimit,
        Math.floor(this.currentLimit * 0.9),
      );
    } else if (this.successRate > this.targetSuccessRate + 0.02) {
      // Slowly increase limit when stable
      this.currentLimit = Math.min(
        this.maxLimit,
        Math.ceil(this.currentLimit * 1.05),
      );
    }
  }

  getCurrentLimit(): number {
    return this.currentLimit;
  }

  getSuccessRate(): number {
    return this.successRate;
  }
}
```

### Rate Limit Headers and Client Communication

**Standard Headers:**

```typescript
interface RateLimitInfo {
  limit: number;
  remaining: number;
  reset: number; // Unix timestamp
  retryAfter?: number; // Seconds
}

function setRateLimitHeaders(res: Response, info: RateLimitInfo): void {
  // Standard headers
  res.setHeader("X-RateLimit-Limit", info.limit.toString());
  res.setHeader("X-RateLimit-Remaining", info.remaining.toString());
  res.setHeader("X-RateLimit-Reset", info.reset.toString());

  // Draft IETF standard headers
  res.setHeader("RateLimit-Limit", info.limit.toString());
  res.setHeader("RateLimit-Remaining", info.remaining.toString());
  res.setHeader("RateLimit-Reset", info.reset.toString());

  if (info.retryAfter !== undefined) {
    res.setHeader("Retry-After", info.retryAfter.toString());
  }
}

// Response body for 429 errors
interface RateLimitErrorResponse {
  error: {
    code: "RATE_LIMIT_EXCEEDED";
    message: string;
    retryAfter: number;
    limit: number;
    resetAt: string;
  };
}

function createRateLimitError(info: RateLimitInfo): RateLimitErrorResponse {
  return {
    error: {
      code: "RATE_LIMIT_EXCEEDED",
      message: `Rate limit exceeded. Please retry after ${info.retryAfter} seconds.`,
      retryAfter: info.retryAfter!,
      limit: info.limit,
      resetAt: new Date(info.reset * 1000).toISOString(),
    },
  };
}
```

**Express Middleware:**

```typescript
import { Request, Response, NextFunction } from "express";

interface RateLimiterOptions {
  windowMs: number;
  max: number;
  keyGenerator?: (req: Request) => string;
  skip?: (req: Request) => boolean;
  handler?: (req: Request, res: Response) => void;
}

function createRateLimiter(options: RateLimiterOptions) {
  const limiter = new DistributedRateLimiter(process.env.REDIS_URL!);

  return async (req: Request, res: Response, next: NextFunction) => {
    // Skip if configured
    if (options.skip?.(req)) {
      return next();
    }

    const key = options.keyGenerator?.(req) || req.ip;
    const result = await limiter.isAllowed(
      key,
      options.max,
      options.windowMs / 1000,
    );

    const info: RateLimitInfo = {
      limit: options.max,
      remaining: result.remaining,
      reset: Math.ceil(result.resetAt / 1000),
    };

    setRateLimitHeaders(res, info);

    if (!result.allowed) {
      info.retryAfter = Math.ceil((result.resetAt - Date.now()) / 1000);

      if (options.handler) {
        return options.handler(req, res);
      }

      return res.status(429).json(createRateLimitError(info));
    }

    next();
  };
}

// Usage
app.use(
  "/api/",
  createRateLimiter({
    windowMs: 60 * 1000, // 1 minute
    max: 100,
    keyGenerator: (req) => req.user?.id || req.ip,
    skip: (req) => req.path === "/api/health",
  }),
);
```

### Graceful Degradation

**Priority-Based Degradation:**

```typescript
enum RequestPriority {
  CRITICAL = 1, // Health checks, auth
  HIGH = 2, // User-initiated actions
  NORMAL = 3, // Regular API calls
  LOW = 4, // Background tasks
  BATCH = 5, // Bulk operations
}

class PriorityRateLimiter {
  private limiters: Map<RequestPriority, TokenBucket> = new Map();
  private systemLoad: number = 0;

  constructor() {
    // Different limits per priority
    this.limiters.set(RequestPriority.CRITICAL, new TokenBucket(1000, 100));
    this.limiters.set(RequestPriority.HIGH, new TokenBucket(500, 50));
    this.limiters.set(RequestPriority.NORMAL, new TokenBucket(200, 20));
    this.limiters.set(RequestPriority.LOW, new TokenBucket(50, 5));
    this.limiters.set(RequestPriority.BATCH, new TokenBucket(10, 1));
  }

  setSystemLoad(load: number): void {
    this.systemLoad = Math.max(0, Math.min(1, load));
  }

  canProcess(priority: RequestPriority): boolean {
    // Under high load, reject low-priority requests
    if (this.systemLoad > 0.8 && priority > RequestPriority.HIGH) {
      return false;
    }
    if (this.systemLoad > 0.9 && priority > RequestPriority.CRITICAL) {
      return false;
    }

    const limiter = this.limiters.get(priority);
    return limiter?.consume() ?? false;
  }
}
```

**Feature Flag Integration:**

```typescript
class DegradationManager {
  private degradedFeatures: Set<string> = new Set();

  async checkAndDegrade(
    feature: string,
    fallback: () => Promise<any>,
  ): Promise<any> {
    if (this.degradedFeatures.has(feature)) {
      return fallback();
    }
    return null; // Continue with normal execution
  }

  enableDegradedMode(feature: string): void {
    this.degradedFeatures.add(feature);
    console.log(`Degraded mode enabled for: ${feature}`);
  }

  disableDegradedMode(feature: string): void {
    this.degradedFeatures.delete(feature);
    console.log(`Degraded mode disabled for: ${feature}`);
  }

  isDegraded(feature: string): boolean {
    return this.degradedFeatures.has(feature);
  }
}
```

## Best Practices

### Algorithm Selection

- **Token Bucket**: Best for allowing bursts while maintaining average rate
- **Sliding Window**: Best for precise rate limiting without bursts
- **Leaky Bucket**: Best for smoothing out traffic to downstream services

### Redis Configuration

- Use Redis Cluster for high availability
- Set appropriate memory limits and eviction policies
- Use Lua scripts for atomic operations
- Monitor Redis latency and connection pool

### Client Experience

- Always return rate limit headers
- Provide clear error messages with retry timing
- Consider implementing client-side rate limiting
- Document rate limits in API documentation

### Monitoring

- Track rate limit hits by endpoint and user
- Alert on sudden spikes in rate limiting
- Monitor for distributed attack patterns
- Log rate limit events for debugging

### Security

- Implement IP-based limits as first line of defense
- Use authenticated rate limits for legitimate users
- Consider geographic rate limiting for region-specific abuse
- Implement CAPTCHA for suspicious patterns

## Examples

### Complete Express Rate Limiting Setup

```typescript
import express from "express";
import Redis from "ioredis";

const app = express();
const redis = new Redis(process.env.REDIS_URL);

// Rate limiter factory
function rateLimiter(config: {
  points: number;
  duration: number;
  keyPrefix: string;
}) {
  return async (
    req: express.Request,
    res: express.Response,
    next: express.NextFunction,
  ) => {
    const key = `${config.keyPrefix}:${req.user?.id || req.ip}`;

    try {
      const current = await redis.incr(key);

      if (current === 1) {
        await redis.expire(key, config.duration);
      }

      const ttl = await redis.ttl(key);
      const remaining = Math.max(0, config.points - current);

      res.set({
        "X-RateLimit-Limit": config.points.toString(),
        "X-RateLimit-Remaining": remaining.toString(),
        "X-RateLimit-Reset": (Math.floor(Date.now() / 1000) + ttl).toString(),
      });

      if (current > config.points) {
        res.set("Retry-After", ttl.toString());
        return res.status(429).json({
          error: "Too Many Requests",
          retryAfter: ttl,
        });
      }

      next();
    } catch (error) {
      // Fail open if Redis is unavailable
      console.error("Rate limiter error:", error);
      next();
    }
  };
}

// Apply different limits to different endpoints
app.use(
  "/api/auth",
  rateLimiter({ points: 5, duration: 60, keyPrefix: "auth" }),
);
app.use(
  "/api/search",
  rateLimiter({ points: 30, duration: 60, keyPrefix: "search" }),
);
app.use("/api", rateLimiter({ points: 100, duration: 60, keyPrefix: "api" }));

app.listen(3000);
```

### Client-Side Rate Limit Handling

```typescript
class APIClient {
  private retryAfter: number = 0;

  async request<T>(url: string, options?: RequestInit): Promise<T> {
    // Check if we're in a rate-limited state
    if (this.retryAfter > Date.now()) {
      const waitTime = this.retryAfter - Date.now();
      throw new Error(
        `Rate limited. Retry after ${Math.ceil(waitTime / 1000)}s`,
      );
    }

    const response = await fetch(url, options);

    // Update rate limit state from headers
    const remaining = response.headers.get("X-RateLimit-Remaining");
    const reset = response.headers.get("X-RateLimit-Reset");

    if (response.status === 429) {
      const retryAfter = response.headers.get("Retry-After");
      this.retryAfter = Date.now() + parseInt(retryAfter || "60") * 1000;

      const body = await response.json();
      throw new RateLimitError(body.error, parseInt(retryAfter || "60"));
    }

    if (!response.ok) {
      throw new Error(`HTTP ${response.status}`);
    }

    return response.json();
  }

  async requestWithRetry<T>(
    url: string,
    options?: RequestInit,
    maxRetries: number = 3,
  ): Promise<T> {
    let lastError: Error | null = null;

    for (let i = 0; i < maxRetries; i++) {
      try {
        return await this.request<T>(url, options);
      } catch (error) {
        lastError = error as Error;

        if (error instanceof RateLimitError) {
          // Wait for the retry-after period
          await this.delay(error.retryAfter * 1000);
        } else {
          // Exponential backoff for other errors
          await this.delay(Math.pow(2, i) * 1000);
        }
      }
    }

    throw lastError;
  }

  private delay(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }
}

class RateLimitError extends Error {
  constructor(
    message: string,
    public retryAfter: number,
  ) {
    super(message);
    this.name = "RateLimitError";
  }
}
```
