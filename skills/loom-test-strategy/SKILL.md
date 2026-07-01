---
name: loom-test-strategy
description: Test strategy guidance — test pyramid design, coverage goals, test categorization, altitude/cost tradeoffs, flaky-test diagnosis, infrastructure architecture, and risk-based prioritization. Use when planning testing approaches, optimizing test suites, or designing test architecture across APIs, data pipelines, ML models, and infrastructure.
triggers:
  - test strategy
  - test pyramid
  - ice cream cone
  - test plan
  - what to test
  - how to test
  - test architecture
  - test infrastructure
  - coverage goals
  - branch coverage
  - mutation testing
  - test organization
  - CI/CD testing
  - test prioritization
  - risk based testing
  - testing approach
  - flaky test
  - test optimization
  - test parallelization
  - test sharding
  - API testing strategy
  - data pipeline testing
  - ML model testing
  - infrastructure testing
---

# Test Strategy

## Overview

How much to test, at what altitude, and in what order — balancing signal against cost. This file owns the **pyramid, coverage goals, risk prioritization, and flaky-test diagnosis**. For writing tests and test-double choice see `loom-testing`; for browser E2E see `loom-e2e-testing`.

## The Pyramid and the Altitude Tradeoff

Push tests **down** to the cheapest altitude that still exercises the risk. Each layer up costs more to write, run, and debug, and fails for more unrelated reasons.

| Layer | Share | Scope | Speed | Signal on failure |
| ----- | ----- | ----- | ----- | ----------------- |
| Unit | 65-80% | one function/branch, no I/O | ms | precise — points at the defect |
| Integration | 15-25% | real boundary (DB, HTTP, queue) | 10ms-1s | wiring/contract broke |
| E2E | 5-10% | full user journey in a browser | seconds | *something* broke, somewhere |

⚠ **Ice-cream cone anti-pattern:** inverted pyramid — many slow E2E tests, few unit tests. Symptoms: hours-long CI, chronic flakiness, "just re-run it" culture, hours to localize a failure. Cause: E2E is easy to *start* (record-and-play) but the suite becomes unmaintainable. Fix: for every E2E failure, ask "what unit/integration test *should* have caught this?" and push it down.

⚠ **Testing Trophy** (Kent C. Dodds) is a legitimate variant for UI/front-end-heavy apps: fewer isolated units, a fat **integration** middle (component + real collaborators via Testing Library), thin E2E. Choose by where your risk and refactor-churn live — do not cargo-cult 70/20/10.

## Coverage — a floor, never a target

Coverage measures what is **un**tested; it says nothing about assertion quality. Mandating 100% breeds assertion-free tests that run lines to hit the number.

| Component | Line | Branch | Note |
| --------- | ---- | ------ | ---- |
| Business logic / domain | 90%+ | 85%+ | the point of the suite |
| API handlers | 80%+ | 75%+ | every endpoint + error shape |
| Utilities / pure fns | 95%+ | 90%+ | cheap and high-value |
| UI components | 70%+ | 60%+ | behavior over markup |
| Infrastructure | 60%+ | 50%+ | prefer integration/smoke |

- Track **branch**, not just line — line coverage passes while `else`/error arms stay untested.
- **Mutation testing** (`mutmut`/`cosmic-ray`, `cargo-mutants`, StrykerJS) is the real quality gate: it mutates code and asserts a test fails. Surviving mutants expose weak assertions that 100% line coverage hid. Run periodically on core logic, not every commit (it's slow).
- Set a coverage **ratchet** (fail if coverage drops) rather than a fixed high bar — prevents rot without incentivizing junk tests.

## What to Test vs Skip

**Always:** business/domain rules, input validation and error handling, security-sensitive ops, data transformations, state transitions, boundary/edge cases, and a regression test for every fixed bug.

**Usually skip:** trivial getters/setters, framework-generated or third-party code, config constants, pure pass-throughs, logging (unless the log is a contract).

```typescript
// BAD — asserts nothing meaningful
test("getName returns name", () => expect(new User("Jo").getName()).toBe("Jo"));
// GOOD — asserts a rule that can actually regress
test("name cannot be blank", () =>
  expect(() => new User("Jo").setName("")).toThrow(ValidationError));
```

## Risk-Based Prioritization

Test effort follows **impact × likelihood**, not uniform coverage.

| Impact ↓ / Likelihood → | Low | Medium | High |
| ----------------------- | --- | ------ | ---- |
| High | Medium | High | **Critical** |
| Medium | Low | Medium | High |
| Low | Skip/manual | Low | Medium |

Risk drivers: revenue/compliance/data-integrity impact, code complexity, change frequency (hot files), historical bug density, dependence on flaky externals.

Map priority to cadence so fast feedback stays fast:

- **P0 (every commit):** auth/authz, payments, data integrity.
- **P1 (PR merge):** core workflows, API contract tests.
- **P2 (nightly):** edge cases, performance.
- **P3 (weekly):** back-compat, deprecated paths.

## Categorization & CI Wiring

Tag suites so CI can select by layer/priority:

```typescript
describe("[unit][fast] UserService", () => {});
describe("[integration][slow] UserRepo", () => {});
describe("[e2e][critical] Checkout", () => {});
// npm test -- --grep="\[unit\]"    (or vitest --project unit / jest projects)
```

Fastest-first, fail-fast pipeline:

```yaml
jobs:
  unit:                       # every push — seconds
    steps: [{ run: "npm test -- --grep='\\[unit\\]' --coverage" }]
  integration:               # needs services
    needs: unit
    services: { postgres: { image: postgres:15 } }
    steps: [{ run: "npm test -- --grep='\\[integration\\]'" }]
  e2e:                       # slowest, last
    needs: integration
    steps: [{ run: "npm run test:e2e" }]
```

CI hygiene: cache deps/build, shard large suites across machines (`--shard=1/4`), run only affected tests locally (`--changedSince=origin/main`), and treat a coverage drop as a failure.

## Domain Strategies (layered priorities)

Concrete assertion examples for each domain live in `loom-testing`; here is *what* to cover at *which* priority.

**API:** contract tests (schema, status codes, error shapes, authz) P0 → business logic P0 → integration (DB, external svc, rollback) P1 → perf/rate-limit P2. Contract tests are the cheapest guard against silent breaking changes — pin request/response schemas.

**Data pipeline:** data-quality (schema, types, nulls, dupes) P0 → transformation correctness + no data loss P0 → source/sink integration + **idempotency** P1 → throughput/memory on large sets P2.

**ML model:** data validation + **leakage detection** + split integrity P0 → behavioral (invariance, directional, min-functionality) P0 → quality thresholds + fairness across groups + regression vs baseline P1 → serving/contract/versioning P1. Assert on behavior and threshold floors, not a single accuracy number.

**Infrastructure:** IaC validation (syntax, security policy, naming, cost) P0 → deploy smoke + health checks P1 → observability (metrics/logs/alerts fire) P1 → resilience/chaos (restart, partition, exhaustion) P2. Assert against the **plan** (e.g. no `0.0.0.0/0` ingress) before spending on live deploys.

## Flaky-Test Diagnosis (canonical)

A flaky test destroys trust in the whole suite. Quarantine it (don't delete), then fix the root cause — never paper over with a blind retry.

| Cause | Symptom | Fix |
| ----- | ------- | --- |
| Race condition | intermittent on timing | await the condition, add synchronization |
| Async not awaited | "element/handle not found" | explicit wait for state, not `sleep` |
| Shared state | fails only with siblings | isolate/reset data between tests |
| External dependency | fails when service down | fake/stub the boundary |
| Time-dependent | fails at date/DST boundaries | inject a frozen clock |
| Nondeterministic data | fails on some random values | seed RNG; log the seed |
| Leaked resources | fails after a certain order | teardown in `finally`/`afterEach` |
| Env drift | passes local, fails CI | containerize; pin versions |
| Parallel collision | fails only when parallelized | unique keys (UUID) per test |

**Diagnose:**

```bash
# reproduce: loop until it fails, then bisect the cause
for i in $(seq 1 200); do npm test -- TestName || { echo "fail at $i"; break; }; done
npm test -- --shuffle           # order dependence?
npm test -- --maxWorkers=4      # parallel race?
```

Always-fails-at-same-point ⇒ it's a real bug, not flaky. Then instrument (verbose logs, timing, failure artifacts), fix the root cause, and **verify by running 1000×** locally + several CI runs before un-quarantining.

**Prevention checklist:**

- [ ] Deterministic data (seeded, no bare `random()`/`Date.now()`)
- [ ] Async uses explicit condition waits, never `sleep`/`waitForTimeout`
- [ ] Unique resource names per test (UUID)
- [ ] Teardown always runs (`try/finally`, `afterEach`)
- [ ] No hardcoded timing (`sleep(100)` is a smell)
- [ ] External services mocked/faked
- [ ] Clocks injected/frozen
- [ ] Order-independent (passes shuffled)
- [ ] Shared state reset between tests
- [ ] Environment reproducible (containerized, pinned)

## Test Infrastructure

**Ephemeral, isolated environments.** Spin dependencies per-run; use `tmpfs`/in-memory volumes for speed.

```yaml
# docker-compose.test.yml — DB on tmpfs, throwaway
services:
  test-db:
    image: postgres:15
    environment: { POSTGRES_DB: test, POSTGRES_PASSWORD: test }
    tmpfs: ["/var/lib/postgresql/data"]   # RAM-backed → fast, no cleanup
```

**Test data via factories**, not shared fixtures — each test mints unique data (sequence/UUID), avoiding cross-test coupling:

```typescript
class UserFactory {
  private seq = 0;
  create(o: Partial<User> = {}): User {
    const n = ++this.seq;
    return { id: `user-${n}`, email: `user${n}@test.com`, role: "user", ...o };
  }
}
```

**Parallelization** — pick isolation to match:

| Strategy | Use when | Config |
| -------- | -------- | ------ |
| File-level workers | files independent | `--maxWorkers=50%` |
| DB-per-worker | tests hit a DB | schema/db per worker id |
| Sharding | multiple CI machines | `--shard=1/4` |
| Affected-only | local dev loop | `--changedSince=origin/main` |

⚠ In CI cap workers (`--maxWorkers=50%`); over-subscribing CPUs makes timing-sensitive tests flake. Longer `testTimeout` in CI than local.

## Best Practices

- **Behavior over implementation** — a passing suite should survive any refactor that preserves behavior.
- **Independent** — each test owns its setup; any order; no shared mutable state.
- **Right double for the job** — stub inputs, mock only contractual interactions, fake awkward collaborators (taxonomy in `loom-testing`).
- **Fast feedback** — optimize the local unit loop; slow tests move to nightly, not the commit path.
- **Same quality bar as prod code** — refactor tests, delete obsolete ones, keep intent in the name.

## Verify before done

- [ ] Shape justified (pyramid vs trophy) by where risk/refactor-churn lives — not cargo-culted
- [ ] Every layer covers something the layer below *can't* — no redundant E2E for logic testable as a unit
- [ ] Coverage is a ratchet/floor with branch tracked; core logic spot-checked with mutation testing
- [ ] P0 set (auth, payments, data integrity) runs on every commit and is fast
- [ ] Each regression-worthy bug has a dedicated test
- [ ] Flaky tests quarantined + root-caused, not blind-retried; prevention checklist applied
- [ ] CI runs fastest-first, fail-fast, with coverage-drop gate
