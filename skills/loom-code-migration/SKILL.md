---
name: loom-code-migration
description: Strategies and patterns for safe code migrations and upgrades. Use when upgrading frameworks, migrating between technologies, handling deprecations, planning incremental rollouts, or applying automated codemods.
triggers:
  - migration
  - migrate
  - upgrade
  - version upgrade
  - breaking change
  - deprecation
  - codemod
  - codemods
  - AST transformation
  - jscodeshift
  - ts-morph
  - comby
  - ast-grep
  - framework migration
  - database migration
  - schema migration
  - expand contract
  - parallel change
  - legacy code
  - modernize
  - modernization
  - rollback
  - strangler fig
  - blue-green deployment
  - canary release
  - shadow mode
  - parallel run
---

# Code Migration

## Overview

Move a codebase between versions, frameworks, technologies, or schemas while keeping it shippable and reversible at every step. Distinct from `/loom-refactoring` (behavior-preserving, internal) — a migration deliberately *changes* what runs, so it needs a parity harness and a rollback path.

## The mandate: never big-bang

A big-bang cutover (rewrite in a branch, flip everything at once) fails because the blast radius equals the whole system, the parity gap is unmeasured until launch, and rollback means reverting weeks of work under fire. **Every migration step must be independently deployable and independently reversible.** If you can't ship the current step to prod and roll just it back, the step is too big.

## Canonical incremental playbook

Every strategy below is an instance of this loop. Internalize the loop, not the individual recipes.

```text
1. PARITY HARNESS   Pin current behavior: characterization/snapshot tests, or a
                    live shadow-compare of old vs new. You cannot migrate safely
                    what you cannot measure.
2. SEAM             Insert an indirection point you can route through:
                    branch-by-abstraction (interface), adapter, or strangler proxy.
3. BUILD NEW        Implement the new path behind the seam. Old path still serves.
4. ROUTE            Shift traffic incrementally behind a flag:
                    shadow → canary(1%) → gradual(5→25→50→100%). Fallback to old on error.
5. VERIFY           Compare old vs new continuously (results, latency, error rate,
                    business metrics). Gate each ramp on match rate + health.
6. CUT OVER         New path is sole path at 100%.
7. CONTRACT         Delete the old path, the seam, flags, and shims. Migration isn't
                    done until the scaffolding is gone.
```

Keep a rollback point before each irreversible-ish action (§Rollback). Reversibility is the property that makes the loop safe.

## Core patterns

### Strangler fig — replace a system route-by-route

A façade/proxy sits in front; requests for migrated routes hit the new system, everything else proxies to the legacy one. Migrated surface grows until the legacy system is fully "strangled," then removed.

```python
# New app owns migrated routes; unmigrated paths fall through to legacy.
@app.api_route("/{path:path}", methods=["GET", "POST", "PUT", "DELETE"])
async def gateway(request: Request, path: str):
    if is_migrated(f"/{path}"):        # set of cut-over route prefixes
        raise HTTPException(404)       # let the real new-system route handle it
    return await proxy_to_legacy(request)   # forward method/headers/body/query verbatim
```

Use for: monolith→services, or replacing a whole subsystem behind a stable URL surface.

### Branch by abstraction — swap an implementation in place

Introduce an interface over the thing being replaced, adapt the *old* impl to it (no behavior change yet — that part is a refactor), add the *new* impl, route by flag, then delete the loser and optionally the abstraction.

```typescript
interface PaymentProcessor { pay(amount: number, token: string): Promise<Result>; }
class StripeAdapter implements PaymentProcessor { /* wraps legacy */ }
class BraintreeAdapter implements PaymentProcessor { /* new */ }

getProcessor(userId): PaymentProcessor {
  return flags.isEnabled("use_braintree", userId) ? braintree : stripe;  // route
}
```

Use for: swapping a library/service/algorithm used from many call sites, without touching those call sites.

### Expand–contract (parallel change) — the pattern for schema & API

The safe way to make a *breaking* change to a shared contract (DB column, API field, function signature). Never mutate in place; run old and new **in parallel** across deploys:

```text
EXPAND    Add the new alongside the old, additive-only (nullable column, new
          endpoint/field, overloaded signature). Old readers/writers untouched.
MIGRATE   Dual-write to both; backfill existing data; move readers to the new;
          verify equivalence.
CONTRACT  Once nothing reads/writes the old, drop it — in a LATER deploy.
```

Critical rule: **expand and contract are separate deploys.** Dropping the old thing in the same release that adds the new one reintroduces the big-bang failure — an old app instance (mid rolling-deploy) or an un-migrated client hits the removed contract and breaks.

**Database column rename via expand–contract:**

```sql
-- Deploy 1 (EXPAND): add new column, nullable/defaulted. App dual-writes both.
ALTER TABLE users ADD COLUMN email_address text;
-- Deploy 2 (MIGRATE): backfill in batches to avoid long locks / replica lag.
UPDATE users SET email_address = email WHERE email_address IS NULL
  AND id BETWEEN :lo AND :hi;             -- loop over id ranges, throttled
-- App now reads new, still writes both; verify counts/checksums match.
-- Deploy 3 (CONTRACT): stop writing old, then drop it.
ALTER TABLE users DROP COLUMN email;
```

DB gotchas: ⚠ `ALTER`/backfill can take table locks or blow up replication lag — batch backfills, add columns nullable (adding `NOT NULL DEFAULT` rewrites the table on many engines), build indexes concurrently (`CREATE INDEX CONCURRENTLY` in Postgres). ⚠ Make each forward migration paired with a tested `down`/reversal until the contract phase.

### Shadow mode / dark launch — validate before serving

Run the new path in production traffic but **don't use its result** — serve the old one, log/compare the new. Zero user risk, real production inputs. Shadow = compare-and-record; dark launch = run new async and log errors only.

```python
async def execute(*a, **k):
    result = await legacy(*a, **k)                 # this is what the user gets
    if shadow_enabled and sample():
        asyncio.create_task(_compare(legacy_result=result, *a, **k))  # non-blocking
    return result
```

Ramp shadow to full traffic; only promote to canary once match rate clears your bar (e.g. ≥99% on non-flaky fields).

### Feature-flagged rollout

The routing mechanism for steps 4-5. Deterministic per-user bucketing so a user's experience is stable across requests:

```python
def in_rollout(flag, user_id, pct):          # stable % rollout
    h = int(hashlib.sha256(f"{flag}:{user_id}".encode()).hexdigest(), 16)
    return (h % 10000) < pct * 10000
```

Ramp: `shadow → 1% canary → 5 → 25 → 50 → 100% → remove flag`. Always support a per-user override (for testing) and a kill switch. Details: `/loom-feature-flags`.

## Deprecation shims

When you can't force all callers to move at once, keep the old surface working while steering usage to the new one:

```python
@deprecated(replacement="authenticate_v2", removal_version="3.0.0")
def authenticate(user, pw):
    return authenticate_v2(Credentials(user, pw))   # delegate, don't duplicate
```

- Emit a `DeprecationWarning` (Python), `@deprecated` (JS/TS/Java), `#[deprecated]` (Rust) — with replacement + removal timeline in the message.
- **Track real usage** (log call sites / counter) so you know when it's safe to remove — don't remove on a guessed date.
- Old impl should *delegate* to new, never fork logic (forked shims drift and rot).

## Automated codemods

Mechanically apply a repetitive transform across the codebase. **AST-based > regex** — regex misses multiline forms, comments, and string-literal false positives; AST tools respect syntax.

| Tool                    | Ecosystem      | Invocation / note                                              |
| ----------------------- | -------------- | -------------------------------------------------------------- |
| **jscodeshift**         | JS/TS          | `jscodeshift -t transform.ts src/` — Facebook's runner; `--dry` first |
| **ts-morph**            | TS             | Library (typed AST); scripting complex TS refactors            |
| **@codemod / codemod**  | JS/TS          | Shareable community codemods runner                            |
| **comby**               | polyglot       | Structural find/replace, language-aware, no AST scripting      |
| **ast-grep** (`sg`)     | polyglot       | `ast-grep -p 'old($A)' -r 'new($A)' -l python` — fast, pattern+rewrite |
| **LibCST / Bowler**     | Python         | LibCST preserves formatting+comments (Bowler builds on it); `2to3` for py2→3 |
| **Rust: `cargo fix`**   | Rust           | `cargo fix --edition` for edition bumps; `syn`+`quote` for custom |
| **Go: `gofmt -r`**      | Go             | `gofmt -r 'old(a) -> new(a)' -w .`; `go fix`; gopls rename      |

```bash
# comby: rename a call across a repo, syntax-aware (:[x] = a hole)
comby 'authenticate(:[args])' 'authenticate_v2(:[args])' .py -i

# ast-grep: same intent, typed pattern + rewrite, dry-run then apply
ast-grep -p 'authenticate($ARGS)' -r 'authenticate_v2($ARGS)' -l py    # preview
ast-grep -p 'authenticate($ARGS)' -r 'authenticate_v2($ARGS)' -l py -U # write
```

Codemod discipline:

- Land the codemod result as **one dedicated commit**, separate from hand edits, so review can trust "generated + spot-checked."
- Always dry-run and eyeball a sample of the diff — codemods misfire on shadowed names, re-exports, and dynamic access.
- Run formatter + full test suite after; a codemod that compiles can still be semantically wrong.
- For 90%-mechanical transforms: codemod the bulk, then hand-fix the residual by hand rather than over-engineering the transform.

## Testing during migration

- **Comparison / differential testing:** feed identical inputs to old and new, assert equal outputs (or a domain-specific comparator — e.g. compare result *sets*, ignore ordering/timestamps). This is the shadow-mode assertion, run offline over recorded inputs.
- **Golden-master / snapshot:** snapshot legacy output for many real inputs, assert new matches. Best when behavior is broad and under-specified.
- **Contract tests:** for API/service migrations, pin the consumer-visible contract so both sides stay compatible during the parallel window.
- Keep the parity harness running through the whole ramp, not just at the start.

## Rollback

Reversibility is a design requirement, not an afterthought.

- **Rollback point before each phase:** record git sha, DB migration version, config/flag snapshot. The fastest rollback is flipping the flag to 0% — design for that first.
- **Auto-rollback:** watch health (error rate, latency, business metric); trip back to the last good point past a threshold. Never fully automate a destructive DB rollback — page a human.
- **DB is the hard part:** code rolls back instantly; data does not. Expand–contract is what makes DB rollback possible — as long as you're pre-contract, the old column/table still exists, so reverting code is safe. Once you `DROP`, rollback means restore-from-backup. Delay contract until you're certain.

## Framework-specific gotchas

The playbook is universal; these are the traps per migration. Consult upstream migration guides + official codemods first.

| Migration                     | Watch out for                                                                 | Tooling                                  |
| ----------------------------- | ----------------------------------------------------------------------------- | ---------------------------------------- |
| React class → function/hooks  | lifecycle→effect semantics differ (`componentDidUpdate` ≠ `useEffect` deps); `this` binding | react codemods; jscodeshift              |
| React 17 → 18 / 19            | StrictMode double-invoke effects; automatic batching changes timing; new root API | `react-codemod`                          |
| Vue 2 → 3                     | Options→Composition API; global API (`Vue.x`) moved to app instance; reactivity via Proxy | `@vue/compat` build, official codemods   |
| AngularJS → Angular           | full rewrite territory — use strangler (`ngUpgrade` hybrid), route-by-route    | ng upgrade                               |
| Python 2 → 3                  | str/bytes split, division, dict ordering; `2to3`/`futurize` under-handle unicode | `python-modernize`, LibCST               |
| Node CJS → ESM                | `require`↔`import`, `__dirname` gone, top-level await, dual-package hazard      | `cjstoesm`, `.mjs`/`"type":"module"`     |
| ORM/DB driver major bump      | query-builder API + generated SQL changes silently; run differential SQL tests | expand–contract on schema                |
| Java 8 → 17+ / Spring Boot 2→3 | `javax.*`→`jakarta.*` namespace; removed internal APIs                         | OpenRewrite recipes                      |
| Test framework swap           | assertion/mocking semantics differ; migrate file-by-file behind both runners   | codemod + parallel run                   |

## Verify before done

- [ ] No step is a big-bang: each is independently deployable AND reversible
- [ ] Parity harness (shadow/snapshot/comparison) established BEFORE building new
- [ ] Breaking contract changes done via expand–contract in SEPARATE deploys (never drop-with-add)
- [ ] DB: additive/nullable first, batched backfill, `CONCURRENTLY` indexes, `down` tested
- [ ] Rollout ramped behind a flag with kill switch + fallback-to-old on error
- [ ] Rollback point per phase; DB contract phase delayed until confidence is high
- [ ] Codemods landed as a separate reviewed commit; formatter + tests green after
- [ ] Deprecation shims delegate (not duplicate) and log usage; removal has a tracked date
- [ ] Old path, seam, flags, and shims removed in the CONTRACT phase — migration fully closed
