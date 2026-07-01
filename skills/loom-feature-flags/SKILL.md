---
name: loom-feature-flags
description: Feature flag patterns for controlled rollouts, A/B testing, kill switches, and runtime configuration. Use for feature toggles, gradual/percentage/canary rollouts, dark launches, user targeting, experiments, emergency kill switches, and model/infrastructure flag switching.
triggers:
  - feature flag
  - feature toggle
  - feature gate
  - LaunchDarkly
  - Unleash
  - OpenFeature
  - split
  - canary
  - gradual rollout
  - percentage rollout
  - dark launch
  - A/B test
  - experiment
  - kill switch
  - circuit breaker
  - model switching
  - infrastructure flag
  - runtime config
---

# Feature Flags

## Overview

Runtime control over feature availability without redeploying. The hard parts are not the `if` check — they are: **bucketing correctness** (stickiness, monotonicity, cross-service consistency), **fail-safe evaluation** (an outage of the flag service must not take down the app), **experiment validity** (exposure logging), and **lifecycle/technical-debt** (stale flags are the #1 real-world feature-flag problem). Optimize for those.

## Flag Taxonomy — Type Drives Lifetime and Ownership

The single most useful classification (from Pete Hodgson / Martin Fowler). Type determines expected lifetime, who owns it, and dynamism. Mixing types under one abstraction is a common mistake.

| Type                | Purpose                                   | Lifetime          | Changes at runtime? | Owner              |
| ------------------- | ----------------------------------------- | ----------------- | ------------------- | ------------------ |
| **Release toggle**  | Ship incomplete/unproven code dark, ramp  | Days–weeks (SHORT)| Per deploy/ramp     | Dev team           |
| **Experiment**      | A/B/multivariate measurement              | Length of test    | Sticky per user     | PM / data science  |
| **Ops / kill-switch**| Disable a subsystem under load/incident  | Long-lived        | On demand (fast)    | Ops / SRE          |
| **Permission**      | Entitlements: plan tier, beta cohort      | Very long / permanent | Per user/segment | Product / billing  |

Consequences:

- **Release toggles must be removed** once the feature is stable — they are debt with a deadline. Track `createdAt + owner + removal ticket`.
- **Ops toggles are permanent** and must be evaluated with *zero* external calls in the hot path (see Kill Switches).
- **Permission toggles** are effectively long-lived config; don't route them through the "delete after rollout" cleanup process.
- Don't overload one flag to do two jobs (a release toggle that also gates a paid tier). Split them; they have different lifetimes.

## Deterministic Bucketing (Correctness Core)

Every percentage rollout, canary, and experiment reduces to: map a stable unit (usually `userId`) to a number in `[0,100)` and compare against a threshold. Get this wrong and users **flicker** in/out of the feature, or increasing the rollout % **reshuffles** everyone.

```typescript
import { createHash } from "crypto";

// Stable, sticky, side-effect-free. Same (unit, flagKey) -> same bucket, forever.
export function bucket(unitId: string, flagKey: string): number {
  // Salt with the FLAG KEY (not a single global salt) so flags are INDEPENDENT:
  // a user unlucky at bucket 2 isn't automatically in every low-% rollout.
  const h = createHash("sha1").update(`${flagKey}:${unitId}`).digest();
  // 32 bits -> [0,1). Prefer this over `int % 100`, which has modulo bias.
  const n = h.readUInt32BE(0) / 0x1_0000_0000;
  return n * 100; // 0.0 .. 100.0
}

// Percentage rollout: enabled iff bucket below the threshold.
export function enabledFor(unitId: string, flag: { key: string; percentage: number }): boolean {
  return bucket(unitId, flag.key) < flag.percentage;
}
```

Rules (violating any of these is a bug, not a style choice):

- **Deterministic, never random per call.** `Math.random() < 0.1` re-rolls on every evaluation → the same user sees the feature appear and vanish across page loads/requests. Always hash a stable unit.
- **Monotonic on ramp-up.** Because the bucket is fixed per `(unit, flag)`, raising `percentage` from 5→10 only *adds* users whose bucket lands in `[5,10)`; nobody already in loses access. **Never change the salt/seed to bump the percentage** — that reshuffles all buckets and yanks the feature from current users (and invalidates any running experiment).
- **Consistent across services.** For a user to see the same variant in web, mobile, and backend, every service must use the **same hash algorithm, same salt convention (flag key), and same unit id.** Pin these; a "harmless" swap of MD5→SHA1 or `userId`→`email` silently re-buckets everyone. (Vendor SDKs like LaunchDarkly guarantee cross-SDK consistency for you — one reason to use them.)
- **Pick the right bucketing unit.** Logged-in → `userId`. Anonymous → a persisted cookie/device id (not the session, or it flickers). Org-level features → `orgId` (so a whole team flips together). Document the unit per flag.
- **MD5/SHA1 are fine here** — this is bucketing, not security, so collision resistance is irrelevant; you only need uniform distribution. Don't reach for bcrypt.

Multivariate uses the same bucket against contiguous ranges:

```typescript
// variants: [{name,value,weight}], weights sum to 100
export function pickVariant<T>(unitId: string, flagKey: string, variants: Variant<T>[]): Variant<T> {
  const b = bucket(unitId, flagKey);
  let acc = 0;
  for (const v of variants) {
    acc += v.weight;
    if (b < acc) return v;
  }
  return variants[variants.length - 1]; // guard float rounding at the top edge
}
```

⚠ Changing a variant's weight re-slices the `[0,100)` line and moves users between variants. To grow one variant without disturbing others, **append** its new share at the top of the range rather than re-slicing from the start.

## Percentage Rollouts and Canary

Ramp `1% → 5% → 25% → 50% → 100%`, watching error rate / latency / business metrics at each step; pause or roll to 0 on regression. Automate the increments but keep a manual gate for the first steps.

```typescript
type Rollout = { flagKey: string; target: number; step: number; intervalMin: number; paused?: boolean };

async function advance(store: FlagStore, r: Rollout): Promise<void> {
  const flag = await store.get(r.flagKey);
  if (!flag || r.paused) return;
  flag.percentage = Math.min((flag.percentage ?? 0) + r.step, r.target); // additive => monotonic
  await store.set(flag);                                                 // same salt: no reshuffle
  if (flag.percentage < r.target) setTimeout(() => advance(store, r), r.intervalMin * 60_000);
}
```

- **Rollback = set percentage 0 (or flip `enabled=false`)**, not a code deploy. That is the entire point of the flag; a rollout without a fast rollback path is theater.
- **Guardrail metrics, not vanity metrics.** Ramp against error rate and latency, plus one business KPI; a 5% cohort with a broken checkout is easy to miss on aggregate dashboards.
- Ring/canary rollout = target internal users → beta cohort → % of general population, expressed as ordered targeting rules over `percentage`.

## Targeting and Evaluation Context

Evaluation is a pure function of `(flag rules, context)`. Rules are ordered; first match wins; fall through to the flag default.

```typescript
type Op = "in" | "notIn" | "equals" | "contains" | "startsWith" | "matches";
type Rule = { attribute: string; operator: Op; values: string[]; value: boolean };

function evaluateTargeting(rules: Rule[], ctx: Record<string, unknown>, def: boolean): boolean {
  for (const r of rules) {
    const attr = String(ctx[r.attribute] ?? "");
    const hit =
      r.operator === "in"         ? r.values.includes(attr)
    : r.operator === "notIn"      ? !r.values.includes(attr)
    : r.operator === "equals"     ? attr === r.values[0]
    : r.operator === "contains"   ? r.values.some((v) => attr.includes(v))
    : r.operator === "startsWith" ? r.values.some((v) => attr.startsWith(v))
    : r.operator === "matches"    ? r.values.some((v) => new RegExp(v).test(attr))
    :                               false;
    if (hit) return r.value;
  }
  return def;
}
```

- **Evaluation must be side-effect-free and cheap** — no DB/network call per flag check. It runs on hot paths, sometimes many flags per request. Load the ruleset once (SDK/cache) and evaluate in memory.
- **`matches` (regex) is a footgun:** rules are often admin-editable → treat as untrusted input. Cap pattern length and beware catastrophic backtracking (ReDoS); prefer `in`/`startsWith` where possible.
- Keep the context minimal and consistent across services (same attribute names). Don't ship secrets/PII into a hosted flag service inside evaluation context.

## Kill Switches (Fail-Safe, No External Calls)

An ops kill switch exists precisely for when things are on fire — including when the flag service itself is degraded. So it must be evaluable **locally**.

```typescript
// Hot path: NO await on a remote store. Read a locally-cached value with a safe default.
function paymentsKilled(cache: FlagCache): boolean {
  // On cache miss / stale / service down -> return the KNOWN-SAFE default,
  // never throw and never block on the network.
  return cache.getBool("kill.payments", /* default */ false);
}

async function processPayment(p: Payment): Promise<PaymentResult> {
  if (paymentsKilled(cache)) throw new ServiceUnavailableError("Payments temporarily disabled");
  return processor.process(p);
}
```

- **Fail to a safe, known state.** Decide per switch what "safe" means and hardcode it as the default: for most *new/risky* features safe = off (fall back to the proven path); for a load-shed switch safe = "not shedding" unless you'd rather shed. The default is a deliberate design decision, documented in code.
- **Never make the availability of a feature depend on the availability of the flag service.** If evaluating the switch requires a network round-trip and that call fails/hangs, you've coupled your app's uptime to the flag provider — the opposite of resilience. Cache aggressively (in-memory + local file/Redis), refresh in the background, tolerate staleness.
- **Propagate activation fast** (pub/sub / streaming), but the hot-path *read* is always local.
- Alert on-call and write an audit record on activate/deactivate; optionally support timed auto-recovery.

## Experiments / A/B Testing

Bucketing is the easy half; **valid measurement** is the hard half.

- **Randomize by hashing a per-experiment salt** so overlapping experiments are statistically independent (orthogonal). Reusing one salt across experiments correlates cohorts and confounds results.
- **Log exposure at evaluation time, only when the flag actually affects the user, exactly once per unit.** This is the crux of A/B validity: your analysis must compare *users who were actually exposed to each variant*, not "everyone we pre-assigned." Pre-assigning all users and counting them as exposed dilutes effects and biases results. Dedup exposures per `(user, experiment)` within the analysis window.
- Log **conversions** keyed to the *same* assigned variant. Compute significance with a two-proportion z-test (or your stats stack); don't eyeball rates — a 3% vs 3.2% gap on small N is noise.

```typescript
function assignAndExpose(exp: Experiment, userId: string, log: ExposureLog): string {
  const v = pickVariant(userId, exp.salt, exp.variants).name; // per-exp salt => orthogonal
  log.once(exp.id, userId, v); // exposure recorded HERE, at the code path that changes behavior
  return v;
}
```

- Don't stop an experiment the moment it crosses significance (peeking inflates false positives) — fix sample size / duration up front, or use a sequential-testing method.
- Guard against sample-ratio mismatch (observed split ≠ configured weights) — it signals a bucketing or logging bug and invalidates the test.

## Flag Lifecycle and Technical Debt

**Stale flags are the #1 feature-flag problem.** Every flag is a live branch in your code; N boolean flags imply up to 2^N reachable states. Untended, they rot into unremovable spaghetti.

- **Attach metadata at creation:** `createdAt`, `owner`, `type`, and — for short-lived types — a **removal ticket** and target date. A release toggle with no removal plan is a bug at birth.
- **Set expiry by type:** release/experiment toggles are short-lived (days–weeks); ops/permission toggles are long-lived. Alert when a *short-lived* flag outlives its expected life or its removal date passes.
- **When done, remove the flag AND the dead branch.** "Make permanent" = delete the flag and the losing code path, keep the winner. Leaving `if (true)` scaffolding is not cleanup.
- **Automate detection:** scan the codebase for flag references, flag stale ones, and file cleanup tickets. Code with no references to a still-"active" flag (or vice-versa) indicates drift.

```typescript
function isStale(f: FlagWithLifecycle, now = new Date()): boolean {
  const ageDays = (now.getTime() - f.createdAt.getTime()) / 86_400_000;
  if (f.plannedRemovalDate && f.plannedRemovalDate < now) return true;       // past removal date
  if (f.type === "release" && ageDays > 60) return true;                     // release toggle overstayed
  if (f.type === "experiment" && f.status === "completed") return true;      // decided, not cleaned up
  if (f.percentage === 100 || f.percentage === 0) return ageDays > 30;       // settled at a terminal %
  return false;
}
```

- **Avoid dependent/nested flags.** Flag B whose meaning depends on flag A creates implicit ordering, hidden coupling, and a combinatorial test space. Keep flags independent; if two must interact, encode it as one multivariate flag, not two coupled booleans.

## Delivery Modes: Local-Eval vs Streaming vs Polling

How the SDK gets flag state governs latency, staleness, load, and privacy. Know the trade-offs:

| Mode                  | Update latency         | Per-eval cost      | Staleness window     | Notes                                                        |
| --------------------- | ---------------------- | ------------------ | -------------------- | ----------------------------------------------------------- |
| **Streaming (SSE)**   | Seconds                | In-memory (0 net)  | ~Real-time           | Best for kill switches; needs a persistent connection       |
| **Polling**           | = poll interval        | In-memory (0 net)  | Up to poll interval  | Simplest; tune interval vs load; can be slow for incidents  |
| **Local evaluation**  | = ruleset refresh      | In-memory (0 net)  | Ruleset age          | SDK holds full ruleset; **no PII leaves your infra**        |
| **Remote per-eval**   | Real-time              | 1 network call/flag| None                 | ⚠ Anti-pattern for hot paths — couples uptime + adds latency|

- **Server SDKs → prefer local evaluation** (or streaming): the SDK downloads targeting rules and evaluates in-process, so no per-flag network call and no user attributes sent to the vendor. **Client/mobile SDKs → the server evaluates** and returns the user's flag set (never ship the full ruleset / other users' targeting to a browser).
- At scale, run a relay/daemon (LaunchDarkly Relay, Unleash Edge/Proxy) so thousands of instances don't each hit the vendor.
- **Every SDK read is against cache** — that's why the default value you pass to `variation(...)` matters: it's what you get during init, on error, or when the flag is missing. Make it the safe fallback.

## OpenFeature and Vendor SDKs

**OpenFeature** (CNCF) is the vendor-neutral standard: one evaluation API + a swappable **Provider** (LaunchDarkly, Flagsmith, Unleash, Split, or your own) + **hooks** for logging/telemetry. Prefer coding against OpenFeature so you can change vendors without touching call sites.

```typescript
import { OpenFeature } from "@openfeature/server-sdk";

OpenFeature.setProvider(new YourProvider());            // swap vendor here only
const client = OpenFeature.getClient();

// Always pass a SAFE default (used on init/error/missing flag) + evaluation context.
const showV2 = await client.getBooleanValue("checkout-v2", false, { targetingKey: userId, plan });
```

LaunchDarkly server SDK specifics worth remembering:

```typescript
import * as LD from "launchdarkly-node-server-sdk";
const ld = LD.init(process.env.LAUNCHDARKLY_SDK_KEY!);
await ld.waitForInitialization();                        // else first evals return defaults
const on = await ld.variation("flag-key", { key: userId }, /* default */ false);
const detail = await ld.variationDetail("flag-key", { key: userId }, false); // .value/.reason for debugging
```

- `targetingKey`/user `key` is the **bucketing unit** — pass a stable id, not a per-request value.
- `variationDetail().reason` tells you *why* a value was served (rule match, fallthrough, prerequisite failed) — essential for debugging "why is this user not seeing the feature."

## ML Model and Infrastructure Flags

Same flag machinery, different payload: route to a **model variant** or an **infra endpoint** instead of on/off. Two extra requirements:

- **Performance/health-based routing with fallback.** Beyond static percentage, route by live latency/error-rate and **always define a fallback** variant when nothing meets thresholds. Health checks run out-of-band; the request path reads the cached healthy set (same fail-safe rule as kill switches).
- **Log an inference/exposure record per call** (latency, success, tokens/cost) to feed routing decisions and cost tracking.

```typescript
function pickModel(flag: ModelFlag, userId: string): ModelVariant {
  const eligible = flag.variants.filter((v) => v.healthy &&
    v.latencyMs < flag.thresholds.latencyMs && v.errorRate < flag.thresholds.errorRate);
  if (eligible.length === 0) return flag.variants.find((v) => v.name === flag.fallback)!; // fail-safe
  return flag.routing === "performance"
    ? eligible.sort((a, b) => a.latencyMs - b.latencyMs)[0]
    : pickVariant(userId, flag.key, eligible);           // sticky percentage routing
}
```

Infra flags (DB/cache/CDN failover) follow the same shape: pick the highest-priority **healthy** variant, fall back deliberately when none are healthy, and reuse existing connections when the selected config is unchanged (don't reconnect every call).

## Gotchas

- ⚠ **`Math.random()` bucketing** → per-call flicker. Hash a stable unit instead.
- ⚠ **One global salt for all flags** → correlated cohorts; a user is in *every* low-% rollout. Salt with the flag key.
- ⚠ **Re-salting to bump a rollout %** → reshuffles all users, evicting current ones and breaking experiments. Only ever *raise the threshold*.
- ⚠ **`int % 100` from a hash** → modulo bias. Scale bytes to `[0,1)` instead.
- ⚠ **Remote call per flag evaluation** → adds latency and couples your uptime to the vendor. Evaluate against cache/local ruleset.
- ⚠ **Kill switch that needs the network to say "kill"** → useless during the outage it's meant to handle. Local read + safe default.
- ⚠ **Pre-assigning/exposure-logging all users** → biased A/B results. Log exposure only at the code path that actually affects the user, once per unit.
- ⚠ **Bucketing by session/request id** for anonymous users → flicker across requests. Use a persisted device/cookie id.
- ⚠ **Dependent/nested flags** → combinatorial state explosion and hidden coupling. Keep flags independent; collapse interactions into one multivariate flag.
- ⚠ **No default on SDK read** → undefined behavior on init/error/missing flag. Always pass the safe fallback.
- ⚠ **Flags that never get removed** → permanent branching debt. Attach owner + removal ticket at creation; automate stale detection.

## Testing With Flags

Flags multiply the reachable state space (2^N for N booleans) — test deliberately, not exhaustively:

- **Test both states of the flag under change** (on/off, and each variant that alters behavior). A flag you can't turn off safely isn't a safe rollout.
- **Pin all other flags to defaults** in most tests via a fake/in-memory provider; don't let the matrix explode. OpenFeature's in-memory provider or a test double makes this trivial and hermetic (no network).
- **Test the fallback path**: flag service unreachable / returns default → app still behaves safely.
- Test bucketing **stickiness** (same unit → same result across calls) and **monotonicity** (raising % never drops an already-enabled unit).
- Keep tests deterministic: inject the bucketing seed/unit; never call the real random source or real vendor in unit tests.

## Verification Checklists

Evaluation logic — verify before done:

- [ ] Bucketing hashes a **stable unit** + flag-key salt; no `Math.random` in the path
- [ ] Increasing rollout % is **additive/monotonic**; salt/seed is never changed to ramp
- [ ] Same hash algo + salt convention + unit across all services that must agree
- [ ] Evaluation is side-effect-free and reads from cache/local ruleset (no per-flag network call)
- [ ] Every SDK/lookup call passes a **safe default** for init/error/missing-flag
- [ ] Regex/targeting input is bounded (no ReDoS); no secrets/PII in evaluation context

Kill switch / ops toggle:

- [ ] Hot-path read is **local** (cached), never a blocking remote call
- [ ] Default on cache-miss/service-down is the documented **known-safe** state
- [ ] Activation broadcasts fast (streaming/pub-sub) and writes an audit record + on-call alert

Experiment:

- [ ] Per-experiment salt (orthogonal cohorts)
- [ ] Exposure logged **at evaluation, only when it affects the user, once per unit**
- [ ] Conversions keyed to the same assigned variant; significance computed, not eyeballed
- [ ] Sample-ratio checked; fixed sample size/duration (no peeking)

Lifecycle / debt:

- [ ] Flag created with `type`, `owner`, `createdAt`, and (short-lived types) a **removal ticket**
- [ ] Stale-flag detection automated; short-lived flags alert past expiry
- [ ] "Done" means flag **and** dead branch removed (no `if (true)` scaffolding)
- [ ] No dependent/nested flags introducing hidden coupling

Change management:

- [ ] Every flag change is audit-logged (who / when / old→new / why)
- [ ] Rollback = flip the flag (percentage→0 / enabled→false), no deploy required
