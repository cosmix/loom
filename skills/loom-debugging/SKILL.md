---
name: loom-debugging
description: Systematic diagnosis and resolution of software bugs, test failures, data quality issues, and performance problems. Use for root-cause analysis, stack trace investigation, flaky/intermittent tests, regressions, crash triage, and "passes locally, fails in CI".
allowed-tools:
  - Read
  - Grep
  - Glob
  - Bash
  - Edit
triggers:
  - debug
  - bug
  - error
  - exception
  - crash
  - issue
  - troubleshoot
  - fix
  - stack trace
  - diagnosis
  - diagnose
  - investigate
  - root cause
  - why
  - failing
  - broken
  - not working
  - unexpected
  - flaky
  - intermittent
  - heisenbug
  - passes locally fails in CI
  - git bisect
  - regression
  - performance degradation
---

# Debugging

## Overview

Find the true cause of a defect and prevent its recurrence — across app code, tests, data pipelines, ML, and infra. The failure mode to avoid is *symptom-patching*: changing code until the symptom disappears without understanding why, which moves the bug rather than fixing it.

## The Root-Cause Loop

Run this loop; don't skip steps. Most wasted time comes from hypothesizing before reproducing, or fixing before localizing.

1. **Reproduce** — deterministically. If you can't reproduce it, you can't verify a fix. Capture exact inputs, env, versions, and the full error/stack. For intermittent bugs, first make it *more* frequent (loop it, add load, shrink timeouts) before anything else.
2. **Minimize** — shrink to the smallest input/code that still fails. Delete half, re-run, repeat (delta-debugging). A 5-line repro localizes faster than a 5000-line one and often reveals the cause outright.
3. **Localize** — bound *where* it happens before asking *why*. Bisect in space (comment out / binary-search modules) and in time (`git bisect`). Read the stack trace top frame first, then the first frame in *your* code.
4. **Hypothesize** — state a specific, falsifiable cause ("X is null because Y returns None when Z"). Vague hypotheses ("something with async") aren't testable.
5. **Test the hypothesis** — one variable at a time; change something that should confirm/refute it. If the experiment can't distinguish two causes, design a better one.
6. **Fix** — the root cause, minimally. Verify the repro now passes AND that you understand *why* the fix works (else you may have masked it).
7. **Prevent** — add a regression test that fails without the fix. No regression test = the bug is not done. Then generalize: are there sibling instances of the same class elsewhere?

## Core Discipline

- **Reproduce before fixing.** Never "fix" what you can't observe failing.
- **Read the error fully** — message, type, and every stack frame. The answer is often literally in it (wrong frame, unexpected value, swallowed cause).
- **Check recent changes.** Most new failures are recently introduced → `git log`, `git bisect`.
- **Question your assumptions.** The bug lives in what you're *sure* is correct. Verify it (print it, assert it) rather than believing it.
- **Preserve evidence.** Save the failing input, logs, core dump, and seed before you start mutating code.

## Bisection Discipline

`git bisect run` automates finding the introducing commit — far faster than manual marking.

```bash
git bisect start
git bisect bad                 # current commit fails
git bisect good v1.2.0         # last known-good ref
git bisect run ./repro.sh      # script: exit 0 = good, 1..124 = bad, 125 = skip (can't test)
# ...git converges to "abc123 is the first bad commit"
git bisect reset
```

- The **repro script must be reliable** — a flaky script sends bisect down the wrong branch. Make the repro deterministic first (see flaky section), or bisect will lie.
- Exit codes: `0` good, `1–124` bad, **`125` = untestable** (skip — e.g. commit doesn't build), `>127` aborts.
- Bisect the *behavior*, not the test file — if the test itself changed, `git bisect run` a script that applies today's test to the old code, or pin the test.
- Works on any monotonic property: performance regressions (`exit 1 if bench > threshold`), binary-size, output diffs — not just pass/fail.

## Differential Debugging

When it works *here* but fails *there* (or worked yesterday), **diff the two worlds** instead of reading code blind. Enumerate and compare, one axis at a time:

- **Code/version:** `git diff good..bad`, dependency lockfile diff (transitive upgrades!), runtime/compiler version, OS/arch.
- **Input/data:** exact request, dataset, encoding, ordering, size, null/edge presence.
- **Environment:** env vars, config files, feature flags, secrets, locale/timezone, `PATH`, working dir.
- **State:** DB contents, cache, filesystem, clock, warm vs cold, concurrency level.

The first axis that, when copied from the working world to the broken one, flips the result is your root cause. `diff <(working_env) <(broken_env)` beats speculation.

### "Passes locally, fails in CI" playbook

Almost always an *environment or isolation* difference, not logic. Check in order:

- **Test ordering / parallelism:** CI runs a different order or in parallel → shared-state leak. Repro locally: run the full suite (not just the one test), randomize order, and force the CI thread count (`cargo test -- --test-threads=N`, `pytest -p xdist -n N`, `jest --runInBand`).
- **Ambient config leakage:** an env var / config file / global you have set locally but CI doesn't (or vice versa). Diff `env`. Your local DB has seed data CI lacks.
- **Wall-clock / timezone / locale:** CI in UTC exposes tests that assume your local TZ; date formatting, `LANG`/`LC_*`, number/decimal separators.
- **Filesystem:** case-sensitivity (macOS/Windows case-insensitive → Linux CI case-sensitive: `import Foo` vs `foo`), path separators, missing fixture files not committed, permissions, `/tmp` cleanup.
- **Resources/timing:** CI is slower/loaded → tight timeouts and `sleep`-based waits fail; less memory → OOM.
- **Hidden network:** a test hits a real service that's reachable locally but blocked/rate-limited in CI.
- **Uncommitted state:** the fix works locally because of an uncommitted file or a dirty DB. Reproduce in a clean checkout / fresh container.

## Flaky Tests — Root-Cause Classes

Flaky = non-deterministic pass/fail with no code change. Classify the cause, then fix the *class* (retrying is not a fix — it hides the defect, often a real production race):

| Class                     | Tell                                          | Fix                                                        |
| ------------------------- | --------------------------------------------- | --------------------------------------------------------- |
| **Shared mutable state**  | fails only with siblings / in a given order   | isolate: fresh fixtures, txn rollback, temp dirs; no globals |
| **Test-ordering dependence** | passes alone, fails in suite (or vice versa) | randomize order to expose; remove inter-test coupling     |
| **Time/clock**            | fails near midnight/DST/month-end, or in UTC  | inject a fake clock; freeze time; never assert on `now()` |
| **Randomness**            | fails ~1/N runs                               | seed the RNG; log the seed so failures are reproducible   |
| **Insufficient waits**    | fails under load / on slow CI                 | wait for a *condition* (poll/await), never `sleep(n)`     |
| **Async races**           | order of callbacks/promises varies            | await completion; deterministic scheduling; barriers      |
| **Network/external**      | fails when a service is slow/down             | mock/stub the boundary; hermetic tests                    |
| **Resource leaks**        | fails after many tests (FDs, ports, memory)   | close/cleanup in teardown; fresh port per test            |
| **Unordered collections** | assertion on set/map/dict iteration order     | sort before asserting; use ordered structures             |

```bash
# Reproduce flakiness: loop until it fails, capture the run
for i in $(seq 1 200); do cargo test flaky_name -- --nocapture 2>&1 | tee /tmp/run || break; done

cargo test -- --test-threads=1     # if this fixes it → shared-state/ordering isolation bug
RUST_LOG=debug cargo test flaky    # expose timing/ordering
pytest -p no:randomly / -p randomly --randomly-seed=N   # pin/vary order to bisect ordering deps
```

## Heisenbugs — when observation changes the outcome

The bug vanishes under a debugger or with a print added. This is diagnostic, not magic: **the observer changed timing or memory layout.**

- **Timing shift:** a `print`/breakpoint adds latency that hides a race or reorders async events. → The real bug is a race; use logging that doesn't alter timing (ring buffer, `perf`/tracepoints, post-mortem logs), or `ThreadSanitizer`/`--race`.
- **Optimization/UB:** works in debug, breaks in release (or vice versa) → undefined behavior, uninitialized memory, aliasing, or a compiler optimization exposing a latent bug. Reach for ASan/UBSan/Valgrind, `-Werror`, MIRI (Rust).
- **Memory/aliasing:** printing a variable forces it to memory (not a register), masking a corruption. → memory tooling, not more prints.
- **Rule:** if adding observation *fixes* it, you have a concurrency/UB bug — attack the timing/memory cause, don't ship the print.

## printf vs. debugger — pick deliberately

| Use **logging/print**                                   | Use a **debugger** (pdb/gdb/lldb/inspector)             |
| ------------------------------------------------------- | ------------------------------------------------------- |
| Concurrency/timing bugs (breakpoints alter timing)      | Single-threaded, deterministic control flow            |
| Distributed / remote / CI / prod (no interactive shell) | Reproducible locally, need to inspect rich live state   |
| Intermittent — need many runs' data                     | Complex object graphs, call stacks, conditional breaks  |
| Fast inner loops where stepping is impractical          | You don't yet know *where* to look (step to find it)    |

Structured logging beats scattered prints: include the value's *identity* (`user_id=%s got=%r expected=%r`), not just "here". Remove or gate debug logging before committing.

## Domain Notes

### Data pipelines

Bugs: schema drift between stages, null/missing handling, type coercion (precision loss/overflow), encoding (UTF-8), OOM on large data. Techniques:

```python
sample = df.filter("problematic_condition").limit(1000); sample.write.parquet("dbg.parquet")  # local repro set
assert df.filter(col("user_id").isNull()).count() == 0, "null user_ids"   # assert assumptions at boundaries
df.explain(); df.printSchema()   # execution plan + schema drift
```

Validate at every stage boundary; log sample rows per transform; test edge data (nulls, empty strings, extremes). Check upstream source for schema changes first.

### ML models

Distinguish code bugs from model-behavior issues. Training: loss not decreasing (LR/gradient flow), exploding loss (instability), over/underfit. Inference: distribution shift, train/serve skew.

```python
for n, p in model.named_parameters():             # dead layers / vanishing grads
    print(n, None if p.grad is None else p.grad.norm().item())
torch.autograd.set_detect_anomaly(True)           # locate NaN/Inf source
# Train/serve skew is the #1 silent accuracy killer:
print(train.mean(), train.std(), prod.mean(), prod.std())  # differ ⇒ preprocessing mismatch
```

⚠ **Train/serve skew:** most "accuracy dropped in prod" bugs are a preprocessing/scaler mismatch — the scaler/encoder fit at train time wasn't persisted and reused at serving. Save and load transforms *with* the model. Also check: feature drift, label leakage, and that eval data matches training preprocessing exactly.

## Worked Example — the full loop

```python
# SYMPTOM: TypeError: 'NoneType' object has no attribute 'name' at process_user (users.py:45), intermittent.

# 1 Reproduce: which user_ids? Loop the failing endpoint; log inputs → fails only for deleted users.
# 3 Localize: stack points at users.py:45 → user is None.
def process_user(user_id: str) -> dict:
    user = get_user(user_id)          # 4 Hypothesis: get_user returns None for deleted users
    return {"name": user.name}        # crash

# 6 Fix root cause (explicit contract), not `user.name if user else ""` (which hides the deletion)
def process_user(user_id: str) -> dict:
    user = get_user(user_id)
    if user is None:
        raise UserNotFoundError(user_id)   # caller decides 404 vs skip
    return {"name": user.name}

# 7 Prevent: regression test that fails without the guard
def test_process_user_missing():
    with pytest.raises(UserNotFoundError):
        process_user("deleted-id")
```

## Tooling quick reference

```bash
# Python
python -m pdb script.py                 # interactive; python -m cProfile -o out.prof for CPU
py-spy top --pid PID                     # sampling profiler, no code change, prod-safe

# Node
node --inspect-brk script.js            # break on first line → chrome://inspect
node --stack-trace-limit=100 ...        # deeper async traces

# Rust
RUST_BACKTRACE=full cargo run           # full trace; RUST_LOG=debug for logs
cargo test -- --test-threads=1          # serialize to expose shared-state flakes

# Systems
strace -f -e trace=file,network CMD     # syscalls (Linux) — find missing files, blocked I/O
gdb -p PID  / lldb -p PID               # attach to a live/hung process; `bt` all threads for deadlocks
tcpdump -i any port 8080                # wire-level; curl -v for HTTP
```

## Verification checklist

- [ ] Reproduced the failure deterministically before changing code
- [ ] Localized (stack + bisect) — you know the exact line and *why*, not just where the symptom shows
- [ ] Root cause identified and stated; fix addresses it, not the symptom
- [ ] Regression test added that **fails without** the fix and passes with it
- [ ] Checked for sibling instances of the same bug class elsewhere
- [ ] For flaky/CI-only: class identified (isolation/time/order/async/resource) and fixed — not retried away
- [ ] No debug prints / temporary logging / `set_detect_anomaly` left in the committed code
