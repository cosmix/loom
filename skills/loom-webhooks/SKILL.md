---
name: loom-webhooks
description: Webhook implementation and consumption patterns. Use when building webhook endpoints, receivers, or senders — covering HMAC signature verification, retry with exponential backoff, idempotency keys, delivery guarantees, replay protection, dead letter queues, payload design, and monitoring.
triggers:
  - webhook
  - webhooks
  - callback
  - callbacks
  - HTTP callback
  - event notification
  - push notification
  - signature verification
  - HMAC
  - hmac
  - crypto signature
  - retry
  - exponential backoff
  - idempotency
  - idempotent
  - delivery guarantee
  - at-least-once delivery
  - webhook receiver
  - webhook sender
  - webhook security
  - webhook authentication
  - replay attack
  - dead letter queue
  - webhook monitoring
  - SSRF
  - thin events
---

# Webhooks

## Overview

HTTP callbacks that push events to external systems instead of polling. The hard parts are all correctness/security: signing over the right bytes, timing-safe verification, replay protection, at-least-once delivery + idempotent consumers, and SSRF on the sender. This skill is organized sender-side vs receiver-side, with the traps called out.

## The non-negotiables (read first)

- **Sign/verify over the RAW request body**, not re-serialized JSON. `JSON.parse` → `JSON.stringify` reorders keys and changes whitespace, breaking the HMAC. Receiver must read raw bytes *before* any JSON body parser runs.
- **Timing-safe compare** for signatures — never `===`. And guard length first: `crypto.timingSafeEqual` **throws** on unequal-length buffers.
- **Timestamped signatures + a tolerance window** to blunt replay; dedup by event id for the rest.
- **At-least-once delivery is the only realistic guarantee** ⇒ duplicates *will* arrive ⇒ **consumers must be idempotent.**
- **Verify before you trust or parse.** For security-critical actions, treat the payload as a *hint* and refetch canonical state from your API.
- **Sender makes outbound requests to user-supplied URLs ⇒ SSRF surface.** Validate destinations; block internal ranges.

## Event & subscription model

```typescript
interface WebhookEvent {
  id: string;            // stable, unique → idempotency + replay key
  type: string;          // "order.created" — resource.action
  created: number;       // Unix seconds
  apiVersion: string;    // payload schema version
  data: {
    object: Record<string, unknown>;
    previousAttributes?: Record<string, unknown>; // deltas on updates
  };
}

interface WebhookEndpoint {
  id: string;
  url: string;
  secret: string;        // per-endpoint, rotatable
  events: string[];      // ["order.*", "payment.completed", "*"]
  status: "active" | "disabled";
}
```

Naming: `resource.action` (`order.created`, `payment.failed`). Support wildcards (`order.*`, `*`).

### Thin vs fat events (design decision)

| | Fat (embed full object) | Thin (id + type only) |
| --- | --- | --- |
| Extra fetch | none | receiver GETs canonical state |
| Freshness | can be **stale/out-of-order** at delivery | always current (fetch at process time) |
| Ordering | receiver must handle reordering via `created`/version | naturally tolerant — fetch reflects latest |
| PII/size | more data in transit/logs; size caps (<256KB) | minimal exposure |
| Reliability | works if your API is down | needs your API up to process |

⚠ **Don't trust a fat payload for security/authorization decisions** — an attacker who forges or replays a delivery controls its contents. Verify signature, then for money/permissions **refetch** by id. Thin events sidestep this and out-of-order delivery, at the cost of a round-trip.

## Sender side

### HMAC signing (over raw payload + timestamp)

```typescript
import crypto from "crypto";

function sign(secret: string, payload: string, ts: number): string {
  return crypto.createHmac("sha256", secret).update(`${ts}.${payload}`).digest("hex");
}

function signedHeaders(secret: string, payload: string): Record<string, string> {
  const ts = Math.floor(Date.now() / 1000);
  return {
    "Content-Type": "application/json",
    "X-Webhook-Timestamp": String(ts),
    "X-Webhook-Signature": `v1=${sign(secret, payload, ts)}`, // scheme-versioned for rotation
  };
}
```

Signing the timestamp *inside* the HMAC (Stripe-style `t=...,v1=...`) prevents an attacker from replaying a captured body with a fresh timestamp. Version the scheme (`v1=`) so you can add `v2=` and support both during rotation.

### Retry with exponential backoff + jitter

```typescript
const retry = { maxAttempts: 5, initialDelay: 1000, maxDelay: 3_600_000, factor: 2,
                retryable: [408, 429, 500, 502, 503, 504] };

function nextDelay(attempt: number): number {           // attempt starts at 0
  const base = Math.min(retry.initialDelay * retry.factor ** attempt, retry.maxDelay);
  return base + base * Math.random() * 0.25;            // 0–25% jitter → no thundering herd
}
```

- Retry only network errors + `retryable` statuses. **Don't retry 4xx** (400/401/422) — the request is broken; retrying just hammers the receiver.
- Honor the receiver's `Retry-After` on 429/503 over your computed backoff.
- 30s delivery timeout. Deliver **asynchronously via a queue** — never block the event-producing transaction on HTTP to a third party.

### At-least-once delivery

```typescript
async function dispatch(event: WebhookEvent, endpoints: WebhookEndpoint[]) {
  await db.events.create(event);                         // persist FIRST (durability)
  for (const ep of endpoints) {
    if (ep.status !== "active" || !matches(event.type, ep.events)) continue;
    await queue.add("deliver", { eventId: event.id, endpointId: ep.id },
      { attempts: 5, backoff: { type: "exponential", delay: 1000 }, removeOnFail: false });
  }
}
function matches(type: string, filters: string[]): boolean {
  return filters.some(f => f === "*" || (f.endsWith(".*") ? type.startsWith(f.slice(0, -2)) : f === type));
}
```

Persist the event before enqueuing so a crash can't lose it. Keep failed jobs (`removeOnFail: false`) for inspection/replay. Include the immutable `event.id` in every delivery so receivers can dedup.

### SSRF: webhook URLs are attacker-controlled

Users register the destination URL, so your sender becomes an SSRF vector into your own network.

- Reject non-HTTPS and non-standard ports at registration.
- Resolve the hostname and **block private/loopback/link-local/metadata ranges**: `127.0.0.0/8`, `10/8`, `172.16/12`, `192.168/16`, `169.254.0.0/16` (incl. `169.254.169.254` cloud metadata), `::1`, `fc00::/7`, `fe80::/10`.
- ⚠ **DNS rebinding**: validate the IP you actually connect to, not just the one resolved at registration — resolve again at request time and pin, or use an egress proxy / allowlist.
- Disable HTTP redirects (or re-validate each hop) — a redirect to `http://169.254.169.254/` bypasses the initial check.
- Cap response body size and timeout; you don't need the receiver's response body.

### DLQ + auto-disable

```typescript
async function onExhausted(delivery: WebhookDelivery) {
  await db.deadLetter.create({ ...delivery, movedAt: new Date() });
  const failures1h = await db.deadLetter.count({ endpointId: delivery.endpointId, since: hoursAgo(1) });
  if (failures1h >= 10) await alerts.warn("Webhook endpoint failing", delivery.endpointId);
  const failures24h = await db.deadLetter.count({ endpointId: delivery.endpointId, since: hoursAgo(24) });
  if (failures24h >= 100) await db.endpoints.disable(delivery.endpointId, "too many failures");
}
```

After max attempts, move to a DLQ (never silently drop). Alert on rising failure rates; auto-disable chronically dead endpoints to stop wasting deliveries, and expose a manual replay path.

## Receiver side

### Verification middleware (raw body first!)

```typescript
import express from "express";

function verify(secret: string, rawBody: Buffer, sigHeader: string, tsHeader: string,
                toleranceSec = 300): void {
  const ts = parseInt(tsHeader, 10);
  if (!ts || Math.abs(Math.floor(Date.now() / 1000) - ts) > toleranceSec)
    throw new WebhookError("timestamp outside tolerance", "TIMESTAMP_EXPIRED"); // replay guard

  const provided = sigHeader.split(",").find(p => p.startsWith("v1="))?.slice(3);
  if (!provided) throw new WebhookError("no v1 signature", "INVALID_SIGNATURE");

  const expected = sign(secret, rawBody.toString("utf8"), ts);
  const a = Buffer.from(provided), b = Buffer.from(expected);
  if (a.length !== b.length || !crypto.timingSafeEqual(a, b))  // length guard: timingSafeEqual throws otherwise
    throw new WebhookError("signature mismatch", "INVALID_SIGNATURE");
}

// express.raw keeps the body as bytes — a global express.json() would corrupt the HMAC
app.post("/webhooks", express.raw({ type: "application/json" }), (req, res) => {
  try {
    verify(process.env.WEBHOOK_SECRET!, req.body,
           req.header("x-webhook-signature")!, req.header("x-webhook-timestamp")!);
  } catch { return res.status(401).json({ error: "invalid signature" }); }

  const event = JSON.parse(req.body.toString()) as WebhookEvent;
  res.status(200).json({ received: true });   // ACK fast (<1s)…
  enqueueForProcessing(event).catch(logger.error); // …then process out of band
});
```

Traps: (1) any body parser that runs before this handler replaces `req.body` and the raw bytes are gone — mount `express.raw` on this route only, or capture raw via a `verify` hook. (2) `timingSafeEqual` throws `RangeError` on length mismatch — compare lengths first (also prevents a length-based side channel). (3) verify **before** `JSON.parse`.

### Idempotent processing (dedup by event id)

```typescript
async function process(event: WebhookEvent) {
  // NX set → true only for the first arrival of this id
  const first = await redis.set(`wh:${event.id}`, "1", "EX", 86400, "NX");
  if (!first) return;                          // duplicate delivery → no-op
  try {
    await handlers[event.type]?.(event.data.object);
  } catch (e) {
    await redis.del(`wh:${event.id}`);         // allow retry to reprocess
    throw e;                                    // 5xx → sender retries
  }
}
```

Redis `SET key val EX ttl NX` is the atomic dedup primitive (no read-then-write race). ⚠ Redis dedup is best-effort — for money-movement use a DB unique constraint on `event_id` (or an inbox table) so dedup is transactional with the side effect. Delete the key on failure so a legitimate retry can reprocess.

### Ordering & staleness

Delivery is **not ordered**. `order.updated` can arrive before `order.created`, or a stale update after a newer one. Defend with the event `created`/a monotonically increasing version: ignore an update whose version ≤ the version you already applied. Thin events dodge this by always fetching current state.

### Receiver status-code contract

| Status | Meaning to sender |
| ------ | ----------------- |
| 2xx | Received (and durably queued) — do not retry |
| 400 | Malformed payload — permanent, do not retry |
| 401 | Bad/missing signature — permanent, do not retry |
| 409 | Duplicate (optional) — do not retry |
| 5xx / timeout | Transient — sender retries |

Return 2xx **only after** you've durably stored/queued the event — a 2xx before persistence means a crash loses it and the sender won't retry. Store the raw payload before processing for replay/debugging.

## Monitoring

Track per endpoint: total deliveries, success rate, avg/p95/p99 latency, error breakdown by status, DLQ depth, retry backlog. Alert on success-rate drop and DLQ growth; auto-disable after a threshold. Give customers a delivery log + self-serve replay for failed events.

```typescript
function p95(values: number[]): number {
  if (!values.length) return 0;
  const s = [...values].sort((a, b) => a - b);
  return s[Math.ceil(s.length * 0.95) - 1];
}
```

## Reference values

- Retries: 5 attempts, 1s initial, 1h max, factor 2, 0–25% jitter; retryable `408, 429, 500, 502, 503, 504`.
- Delivery timeout 30s; receiver should ACK <1s and process async.
- Signature tolerance ~300s; idempotency/dedup TTL ~24h; payload cap <256KB.
- Rotate secrets periodically; support two active secrets during rotation (accept either).

## Checklists

**Sender — verify before shipping:**

- [ ] Event persisted before enqueue; delivery async via durable queue
- [ ] HMAC over raw payload with timestamp inside the signed string; scheme versioned (`v1=`)
- [ ] Backoff + jitter; retries limited to network errors + retryable 5xx/429/408; `Retry-After` honored; 4xx not retried
- [ ] SSRF guard: HTTPS-only, private/link-local/metadata IPs blocked, redirects disabled/re-validated, DNS-rebinding handled at connect time
- [ ] DLQ on exhaustion (nothing silently dropped); failure alerting + auto-disable + manual replay
- [ ] Per-endpoint rotatable secrets; monitoring (success rate, p95, DLQ depth)

**Receiver — verify before shipping:**

- [ ] Raw body captured before any JSON parser; signature verified before `JSON.parse`
- [ ] Timing-safe compare **with length guard**; timestamp tolerance enforced (replay window)
- [ ] Consumer idempotent — dedup by `event.id` (DB unique constraint for state-changing/financial ops)
- [ ] ACK 2xx only after durable persist/enqueue; then process out of band
- [ ] Out-of-order tolerated via `created`/version; security-critical actions refetch canonical state, don't trust payload
- [ ] Correct status codes: 2xx received, 400/401 permanent, 5xx retryable
