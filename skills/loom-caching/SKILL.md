---
name: loom-caching
description: Caching strategies for performance optimization — cache-aside, write-through, write-behind, TTL policies, eviction, and stampede prevention. Use for Redis/Memcached, CDN caching, database query caching, ML model caching, and distributed cache design.
triggers:
  - cache
  - caching
  - Redis
  - Memcached
  - CDN
  - TTL
  - invalidation
  - eviction
  - LRU
  - LFU
  - FIFO
  - write-through
  - write-behind
  - cache-aside
  - read-through
  - cache stampede
  - distributed cache
  - local cache
  - memoization
  - query cache
  - result cache
  - edge cache
  - browser cache
  - HTTP cache
---

# Caching

## Overview

Store expensive-to-compute or frequently-read data closer to the consumer. The easy part is the read path; the hard parts are **invalidation** (keeping it correct) and **stampede** (surviving a mass miss). This skill is organized around those failure modes, not around toy get/set wrappers.

## Strategy Selection

| Strategy | Read path | Write path | Consistency | Failure mode to watch |
| --- | --- | --- | --- | --- |
| **Cache-aside** (lazy) | App checks cache → loads on miss → populates | App writes DB, then **invalidates** (don't update) cache | Eventual; brief staleness window | Stampede on hot-key miss; race between load and invalidate |
| **Read-through** | Cache library loads on miss | (paired with write-through) | Eventual | Same as aside; hides loader in the cache layer |
| **Write-through** | Read from cache | Write DB **and** cache synchronously | Strong-ish; cache always fresh after write | Write latency = DB + cache; cache churn for rarely-read keys |
| **Write-behind** (write-back) | Read from cache | Write cache now, flush DB async in batches | Weak; **data loss if node dies before flush** | Lost writes on crash; ordering; DB divergence |

**Default to cache-aside.** It's simple, resilient (cache down ≠ writes fail), and puts invalidation in your control. Reach for write-through only when you can't tolerate a post-write stale read; write-behind only for write-heavy, loss-tolerant data (metrics, counters, activity feeds).

⚠ **On write, invalidate — do not update — the cache** (cache-aside). Two concurrent writers updating the cache can commit their DB writes in one order and their cache writes in the opposite order, leaving the cache permanently wrong. Deleting the entry forces the next reader to reload the DB's truth. (This is the [Facebook "leases"] class of bug.)

⚠ **Write-behind loses data.** A crash between "wrote cache" and "flushed DB" silently drops the write. Never use it as the system of record for anything you can't recompute or afford to lose.

### Cache-aside (the workhorse)

```python
async def get_or_load(key, loader, ttl):
    cached = await cache.get(key)
    if cached is not None:
        return json.loads(cached)          # ⚠ distinguish "missing" from cached falsy/None
    value = await loader()
    await cache.setex(key, ttl_with_jitter(ttl), json.dumps(value))
    return value

# write:  await db.update(...);  await cache.delete(key)   # invalidate, not update
```

⚠ **Miss vs cached-null:** `if cached:` treats a cached `0`, `""`, `[]`, or `false` as a miss and re-loads every time. Check `is not None` and cache an explicit sentinel for negatives (see negative caching).

## Stampede / Thundering Herd

When a hot key expires, every concurrent request misses simultaneously and hammers the origin — often enough to knock over the DB the cache was protecting. Three mitigations, roughly in order of strength:

**1. Per-key lock (single-flight):** only one caller regenerates; others wait or serve stale. Double-check the cache *after* acquiring the lock.

```python
async with locks[key]:                 # per-key lock (in Redis: SET lock NX PX for cross-node)
    cached = await cache.get(key)      # double-check: someone may have filled it
    if cached is not None: return json.loads(cached)
    value = await loader()
    await cache.setex(key, ttl, json.dumps(value))
    return value
```

⚠ An in-process `asyncio.Lock` only serializes *one* process. For a multi-node stampede you need a **distributed lock** (`SET key val NX PX 30000`) with a TTL so a crashed holder can't wedge the key forever.

**2. Probabilistic early expiration (XFetch):** recompute *before* expiry with a probability that rises as expiry nears, so one lucky request refreshes while the entry is still warm and others keep hitting the cache. The canonical formula:

```python
# recompute if:  now - delta * beta * ln(random()) >= expiry
#   delta = last recompute cost (s), beta ~1.0 (raise to refresh earlier)
if time.time() - delta * beta * math.log(random.random()) >= expiry:
    ...recompute and reset expiry...
```

This is the best hands-off defense for a single hot key — no lock coordination, no full stampede, and no synchronized cliff.

**3. Stale-while-revalidate:** serve the stale value immediately and refresh in the background (guarded by a lock so only one refresh runs). Best UX (no request ever blocks on regeneration) when brief staleness is acceptable.

⚠ **TTL jitter is mandatory at scale.** If you cache 10k keys with `ttl=3600` in a warmup burst, they all expire in the same second → synchronized stampede. Add ±10% random jitter to every TTL so expirations spread out. This is the cheapest stampede prevention and it's one line.

## Negative Caching

Cache **misses and errors**, not just hits. A query for a non-existent user, a 404, or an empty result set is often *more* attackable — an attacker requesting random non-existent keys bypasses the cache entirely and floods the DB (cache-penetration attack).

- Cache the negative result under a short TTL (e.g. 30–60s, much shorter than positive TTL) with an explicit sentinel (`"__NULL__"`), so you can tell it apart from a miss.
- For high-cardinality penetration attacks, a **Bloom filter** of known-existing keys in front of the cache rejects definitely-absent keys in O(1) without a DB hit.

## TTL & Expiration

- **Tiered TTLs** by volatility: session 1d, profile 1h, catalog 5m, search results 1m, real-time 10s. Never one global TTL.
- **Jitter** every TTL (see stampede) — ±10% is standard.
- **Sliding TTL** (refresh expiry on access) keeps hot keys alive but risks *never* expiring stale data; cap it with an absolute max-age.
- Set an **eviction policy** deliberately: `allkeys-lru` (general cache), `allkeys-lfu` (skewed popularity — Redis LFU resists one-off scans polluting the cache), `volatile-ttl` (only evict keys with TTLs). Default `noeviction` will start **erroring writes** when `maxmemory` is hit — a nasty surprise if you treated Redis as a pure cache.

## Redis Patterns & Footguns

- **Right structure:** hashes for objects (partial field updates without re-serializing), sorted sets for rankings/time-ordering, lists for queues, plain strings for serialized blobs.
- **Atomic compound ops → Lua.** `GET`-then-`SET` across the network races; a Lua script runs atomically server-side.
- **Pipeline** to batch round-trips (not atomic); **`MULTI/EXEC`** for atomic batches.
- ⚠ **`KEYS` is O(N) and blocks the entire server** — never in production. Use `SCAN` (cursor, non-blocking) for pattern invalidation.

⚠ **Hot key problem:** one key (a celebrity user, a global config) gets a disproportionate share of traffic and saturates the single shard/node that owns it. Mitigate with a short-TTL **local (L1) cache** in front of Redis, or replicate the key across N suffixed copies (`key:{0..N}`) and read a random one.

⚠ **Big key problem:** a single huge value (a multi-MB blob, a million-element set) causes latency spikes (blocking (de)serialization, slow `DEL`), skewed memory, and slow cluster migration. Split big collections; use `UNLINK` (async delete) instead of `DEL` for large keys; watch for `O(N)` commands (`HGETALL`, `SMEMBERS`) on them.

## Invalidation (the actually-hard part)

> "There are only two hard things in Computer Science: cache invalidation and naming things." Budget accordingly.

- **Tag/group invalidation:** store a set of keys per tag (`SADD tag:user:42 <key>`); invalidating the tag deletes all members. Lets one write invalidate every derived entry.
- **Version-keying:** embed a version in the key (`user:42:v7`). "Invalidation" = bump the version; old entries age out via TTL. No delete needed, no stale reads, at the cost of extra memory until old versions expire. Excellent for content-addressed / immutable data.
- **Event-based:** publish invalidation messages (Redis pub/sub, or a change-data-capture stream) so every node drops the key. Necessary when nodes keep L1 caches that Redis-side deletes won't reach.
- **Pattern (`SCAN MATCH`)** is a fallback, not a design — O(keys) and racy. Prefer tags or versioning.

⚠ **Invalidate on write, and order it right.** Update DB first, then invalidate cache. If you invalidate first, a concurrent reader can repopulate the cache with the *old* DB value in the gap before your write commits. Even DB-first has a small race → short TTL is the safety net under any invalidation scheme.

## Cache Key Design

- **Deterministic & collision-free:** hash normalized inputs. For query caches, sort params before hashing (`md5(sql + sorted(params))`) so `?a=1&b=2` and `?b=2&a=1` share a key.
- **Namespaced & versioned:** `app:v3:user:42` — a prefix lets you bulk-invalidate a whole class (bump `v3`→`v4`) and avoids collisions across features sharing one Redis.
- Include everything that changes the value: locale, tenant, auth scope, API version. A key that omits `tenant_id` leaks data across tenants — a **security bug**, not just a correctness one.

## Layered Caching

Browser → CDN → app L1 (in-process LRU) → Redis L2 → DB. Each layer absorbs load and TTL-shortens as you go outward. **L1 (in-process) is the hot-key cure** but is per-node and can serve stale until its (short) TTL lapses — accept bounded staleness or wire event-based invalidation.

**In-process LRU** (thread-safe, TTL-aware) as L1:

```python
class LRUCache:                        # OrderedDict + RLock; move_to_end on hit
    def get(self, key):
        with self.lock:
            if key not in self.cache: return None
            value, expiry = self.cache[key]
            if expiry and time.time() > expiry:
                del self.cache[key]; return None
            self.cache.move_to_end(key); return value
    def put(self, key, value, ttl=None):
        with self.lock:
            self.cache[key] = (value, time.time() + ttl if ttl else None)
            self.cache.move_to_end(key)
            if len(self.cache) > self.capacity:
                self.cache.popitem(last=False)   # evict LRU
```

Prefer stdlib `functools.lru_cache` for pure-function memoization; hand-roll only when you need TTL/eviction hooks.

## HTTP / CDN Caching

Correctness lives in the headers:

- `Cache-Control: public, max-age=31536000, immutable` — for fingerprinted assets (`app.a1b2c3.js`). `immutable` tells the browser to skip revalidation entirely.
- `Cache-Control: public, max-age=0, must-revalidate` + `ETag` — content that changes; client revalidates with `If-None-Match` → cheap `304`.
- `Cache-Control: private` — per-user responses; keep them out of shared/CDN caches. ⚠ Caching a per-user response as `public` on a CDN leaks it to other users.
- `Cache-Control: no-store` — never cache (auth, PII).
- **`stale-while-revalidate=N`** and **`stale-if-error=N`** — serve stale for N seconds while refreshing / when origin errors. The HTTP-level version of the pattern above; huge availability win at the edge.
- **Cache-bust by URL** (`app.<hash>.js`), never by shortening TTL — fingerprinted URLs get `immutable` + 1-year TTL and change name when content changes.

## Domain Notes

- **DB query cache:** key on `hash(sql + sorted_params)`; invalidate by table tag on write (ORM `after_flush` hook → delete `query:*table*` via a tag set, not `KEYS`). Short TTL as backstop.
- **ML:** cache **predictions** keyed on a hash of normalized input (short TTL); cache **models** in memory keyed by `model:id:vN` (long TTL, compress large blobs); cache **embeddings** as raw bytes (`np.frombuffer` / `.tobytes()`, not JSON) with long TTL — embeddings are deterministic and expensive, ideal cache candidates.

## Security & Correctness Checklist

- [ ] Write path **invalidates** (not updates) the cache; DB written before invalidation
- [ ] Every TTL has jitter; no synchronized-expiry cliff
- [ ] Hot keys have an L1 / distributed stampede guard (lock, XFetch, or SWR)
- [ ] Negatives/404s cached with short TTL + sentinel (penetration attack considered)
- [ ] `is not None` used, so cached falsy values aren't treated as misses
- [ ] Keys namespaced + versioned; include tenant/locale/auth-scope (no cross-tenant leak)
- [ ] `maxmemory` + eviction policy set intentionally (not accidental `noeviction` write errors)
- [ ] No `KEYS` in prod; `SCAN`/tags for invalidation; `UNLINK` for big keys
- [ ] No secrets/PII/tokens cached unencrypted; per-user data never `public`/shared
- [ ] Cache treated as optional: origin still works (correctly, if slower) when cache is fully down
- [ ] Hit rate, latency, eviction rate, and memory monitored (a 5% hit rate is a bug, not a cache)
