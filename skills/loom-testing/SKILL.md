---
name: loom-testing
description: Test implementation across unit, integration, e2e, security, infrastructure, data pipeline, and ML domains. Use for writing tests, debugging flaky tests, improving coverage, and following TDD/BDD workflows with pytest, jest, vitest, mocha, junit, or testify.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Edit
  - Write
  - Bash
triggers:
  - test
  - testing
  - spec
  - assert
  - expect
  - mock
  - stub
  - spy
  - fake
  - fixture
  - snapshot
  - coverage
  - TDD
  - BDD
  - red-green
  - regression
  - unit test
  - integration test
  - e2e
  - end-to-end
  - test suite
  - test case
  - table-driven
  - pytest
  - jest
  - vitest
  - mocha
  - junit
  - testify
  - test framework
---

# Testing

## Overview

Writing tests: unit/integration/e2e plus data-pipeline, ML, and infrastructure domains. This file owns **test-double taxonomy, AAA, and framework patterns**. For pyramid ratios, coverage targets, risk prioritization, and flaky-test diagnosis, see `loom-test-strategy`; for browser/Playwright/Cypress, see `loom-e2e-testing`.

## Workflow

1. **Map the unit** — public interface, dependencies/side effects, invariants, error paths, boundary values.
2. **Pick the altitude** — unit for logic/branches; integration for real collaborators (DB, HTTP) at a boundary; e2e for user journeys. Push detail down the pyramid (`loom-test-strategy`).
3. **Write failing test first** when practical (TDD red→green→refactor): a red test proves the test actually exercises the code; a test that never failed asserts nothing.
4. **Arrange-Act-Assert**, one logical assertion per test, deterministic inputs.

### AAA and naming

- **Arrange** state/doubles → **Act** (one call to the unit) → **Assert** outcome. Blank-line separate the three; more than one Act means split the test.
- Name `test_<unit>_<scenario>_<expected>` — e.g. `test_cart_add_duplicate_increases_quantity`. The name is the spec; a failing name should tell you what broke without reading the body.
- **One logical assert per test.** Multiple physical asserts on *one* behavior are fine (`status`, then `body`); asserting two unrelated behaviors is "assertion roulette" — the first failure masks the rest. Prefer one composite assert (`assert_eq!(got, expected_struct)`) over many field asserts.

## Test Doubles — taxonomy and misuse

Precision matters: "mock" is colloquially any double, but the kind you choose decides whether the test is brittle.

| Double | Does | Verifies | Reach for when |
| ------ | ---- | -------- | -------------- |
| **Dummy** | Fills a parameter, never used | nothing | Satisfy a signature |
| **Stub** | Returns canned values for indirect *inputs* | state (result) | Drive a branch from a dependency's return |
| **Spy** | Stub that records how it was called | state + calls, *after* | Assert an effect happened, loosely |
| **Mock** | Pre-set call *expectations*, self-verifying | interaction, *strict* | The interaction **is** the contract (e.g. "charge called once with amount") |
| **Fake** | Lightweight working impl (in-memory DB, fake clock) | state | Collaborator too slow/awkward for real, but behavior matters |

**When each is wrong:**

- **Mock used where a stub belongs** → the test asserts *how* the code works (which methods it calls), not *what* it produces. Refactoring the internals breaks green tests. Assert the observable outcome; mock only when the call itself is the observable behavior (payment charged, email sent, event published).
- **Over-mocking** → every collaborator faked; the suite is green while real integration is broken. Mock only at architectural seams you don't control: **network, clock, filesystem, randomness, external services**. Do not mock the type under test or pure internal collaborators.
- **Fakes drift** from the real thing (in-memory store enforces no FK constraints the real DB does). Pair fakes with a thin layer of integration/contract tests against the real implementation.
- **Mocking a value object** instead of constructing a real one — always cheaper and truer to build the real value.

## Determinism (non-negotiable)

Flaky tests are worse than no tests — they train the team to ignore red. Eliminate every non-deterministic input:

- **No `sleep`/`waitForTimeout`.** Poll/await a *condition* (element visible, job status == done). A fixed sleep is either flaky (too short) or slow (too long).
- **Freeze the clock.** Inject a clock or use a fake-timer lib; never assert against `Date.now()`/`SystemTime::now()`.
- **Seed RNG** and any faker/UUID source used in assertions; log the seed on failure so you can reproduce.
- **Isolate shared state** — reset DB (transaction rollback or truncate), globals, singletons, env vars, and filesystem between tests. Unique keys (UUID) per test so parallel runs don't collide.
- Order-independent: tests must pass in any order and in isolation (`--shuffle` / random seed in CI).

Diagnosis workflow and the full flakiness cause table live in `loom-test-strategy`.

## Coverage (floor, not target)

Coverage tells you what's **un**tested, not what's tested well. Chasing a number invites assertion-free tests that execute lines without checking anything.

- Track **branch** coverage, not just line — line coverage hides untested `else`/error paths.
- **Mutation testing** (`mutmut`, `cargo-mutants`, StrykerJS) is the real signal: it perturbs code and checks a test fails. Surviving mutants = weak/missing assertions that line coverage rated 100%.
- Cover behavior and error paths, not getters/framework code. Targets by component: see `loom-test-strategy`.

## Framework Patterns

### Python (pytest)

```python
import pytest
from unittest.mock import patch

@pytest.fixture
def db():
    d = Database(); d.connect()
    yield d
    d.disconnect()                       # teardown runs even on failure

@pytest.mark.parametrize("n,expected", [(0, 1), (1, 1), (5, 120)])
def test_factorial(n, expected):         # table-driven: one row per case
    assert factorial(n) == expected

@patch("payments.stripe")                # patch where it's *used*, not defined
def test_charge_declined_raises(mock_stripe):
    mock_stripe.Charge.create.side_effect = CardError("declined")
    with pytest.raises(PaymentError, match="declined"):
        PaymentProcessor().charge(1000, "tok_x")
```

⚠ `@patch` target is the name in the module under test (`payments.stripe`), not `stripe` itself — patching the wrong path is the #1 mock no-op. Prefer `freezegun`/`time-machine` for the clock and `pytest.raises(match=...)` to assert the message.

### JavaScript/TypeScript (Vitest/Jest)

```javascript
import { describe, it, expect, vi, beforeEach } from "vitest";

describe("UserService", () => {
  let db;
  beforeEach(() => { db = { query: vi.fn() }; });

  it("finds user by email", async () => {
    db.query.mockResolvedValue([{ id: 1, email: "a@b.com" }]);
    const user = await new UserService(db).findByEmail("a@b.com");
    expect(user.id).toBe(1);
    expect(db.query).toHaveBeenCalledWith(expect.stringContaining("WHERE email"), ["a@b.com"]);
  });
});
```

⚠ Use `vi.useFakeTimers()` for time; `await expect(promise).rejects.toThrow()` for async errors. `toEqual` deep-compares, `toBe` is `Object.is` — mixing them up passes/fails silently on objects. Snapshot tests (`toMatchSnapshot`) rot into rubber-stamps — reserve for stable serialized output, review every snapshot diff, never `--updateSnapshot` blindly.

### Rust (built-in + mockall)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use mockall::predicate::eq;

    #[test]
    fn fetches_user_from_db() {
        let mut db = MockDatabase::new();
        db.expect_get_user()
            .with(eq(1))
            .times(1)
            .return_once(|_| Ok(User { id: 1, name: "Alice".into() }));
        assert_eq!(UserService::new(db).get_user(1).unwrap().name, "Alice");
    }
}
```

⚠ Use `#[should_panic(expected = "...")]` sparingly — prefer returning `Result` and asserting the `Err`. Tests needing serialized access to shared state use `#[serial]` (`serial_test` crate — this repo relies on it). `assert_eq!` prints both sides on failure; hand-rolled `assert!(a == b)` does not.

## Domain Examples

Concrete assertions per domain; strategy/priority layering is in `loom-test-strategy`.

**Data pipeline** — assert on invariants, not exact output:

```python
def test_etl_preserves_rows_and_keys():
    out = etl.transform(load_fixture("sales_1000.csv"))
    assert len(out) == 1000                          # no silent drops
    assert out["customer_id"].notna().all()          # required field intact
    assert out["order_id"].is_unique                  # no dup fan-out
```

Also test **idempotency** (run twice → same result), incremental (only new rows processed), and partial-failure recovery.

**ML model** — behavioral tests, not just aggregate accuracy:

```python
def test_sentiment_invariant_to_punctuation():
    m = load_model("sentiment")
    assert abs(m.predict("great product") - m.predict("great product!!!")) < 0.1
```

Cover invariance (irrelevant change → stable output), directional expectation (adding a positive word raises score), and a minimum-functionality set the model must never miss. Guard train/test split for **leakage**.

**Infrastructure (IaC)** — assert on the plan, not a live deploy:

```python
def test_no_public_s3_buckets():
    for r in load_plan("main.tfplan.json").resources("aws_s3_bucket"):
        assert r.get("acl", "private") != "public-read", f"{r['name']} is public"
```

## Anti-Patterns

- **Testing implementation** — private methods, internal call counts. Test through the public interface; assert outcomes.
- **Interdependent tests** — one test's writes are another's fixture. Each test self-contains its setup.
- **Logic in tests** — loops/conditionals/computed expected values re-implement the code and hide bugs. Hard-code expected values; use parametrization for variants.
- **Excessive mocking** — see taxonomy above; brittle and false-green.
- **Slow unit tests** — real I/O in a "unit" test. Push to integration or fake the boundary.
- **Assertion roulette / no message** — a bare `assert result` failure tells you nothing.

## Test Organization

```text
tests/
├── unit/          # fast, isolated, no I/O
├── integration/   # real DB/HTTP at a boundary
├── e2e/           # user journeys (see loom-e2e-testing)
├── fixtures/      # shared data / factories
└── helpers/       # setup, custom assertions
```

CI altitude: unit on every commit, integration on PR, e2e pre-deploy — details in `loom-test-strategy`.

## Verify before done

- [ ] Each test has one clear reason to fail (one behavior); name states scenario + expected outcome
- [ ] AAA structure; expected values hard-coded, not computed
- [ ] Doubles chosen by role (stub for inputs, mock only where the interaction is the contract); no over-mocking of internals
- [ ] Deterministic: clock frozen, RNG seeded, state reset, no `sleep` — passes shuffled and in isolation
- [ ] Error/edge paths asserted (types **and** messages), not just the happy path
- [ ] New test **fails** when the code under test is broken (verify red before green; spot-check with a mutant)
- [ ] No test-only logic in production code; no leftover `.only`/`fdescribe`/`#[ignore]`
