---
name: loom-debugging
description: "Systematically diagnoses and resolves software bugs, test failures, data quality issues, and performance problems. Use when the developer needs to debug crashes, flaky tests, data pipeline errors, ML model degradation, or trace root causes across application code and infrastructure."
allowed-tools: Read, Grep, Glob, Bash, Edit
trigger-keywords: debug, bug, error, exception, crash, issue, troubleshoot, fix, stack trace, diagnosis, diagnose, investigate, root cause, why, failing, broken, not working, unexpected, flaky, intermittent, regression, performance degradation
---

# Debugging

## Overview

This skill provides systematic approaches to finding and fixing bugs across all domains: application code, tests, data pipelines, ML models, and infrastructure. It covers debugging strategies, tool usage, and techniques for crashes, flaky tests, data quality problems, and model performance degradation.

## Workflow

The agent follows these numbered steps when debugging any issue:

1. **Understand the Problem** — Reproduce the issue consistently. Gather error messages and stack traces. Identify when the bug was introduced. Determine expected vs actual behavior.
2. **Isolate the Issue** — Create a minimal reproduction case. Use binary search (git bisect) to narrow the cause. Check recent changes. Verify environment and dependencies.
3. **Diagnose Root Cause** — Add strategic logging. Use a debugger to step through code. Analyze stack traces. Check for common patterns (off-by-one, race conditions, null references).
4. **Fix and Verify** — Implement a targeted fix. Add a regression test. Verify the fix does not introduce new issues. Document the root cause.

## Best Practices

1. **Reproduce First**: The agent never attempts a fix without first reproducing the problem.
2. **Read Error Messages**: They often contain the answer directly.
3. **Check Recent Changes**: Most bugs are recently introduced — git log and git bisect are powerful.
4. **Question Assumptions**: The agent verifies what it thinks it knows before acting.
5. **Isolate Variables**: Change one thing at a time to confirm causality.
6. **Write Tests**: Prove the bug exists with a failing test, then prove it is fixed.

## Specialized Debugging Domains

### Flaky Tests

Flaky tests pass and fail non-deterministically. The agent checks these root causes in order:

- **Timing issues** — Race conditions in async code, insufficient waits for UI elements, network timeouts, background jobs not completing.
- **Non-deterministic state** — Random data without seeds, unordered collections, floating-point precision, timestamp-based logic.
- **Test isolation failures** — Shared global state, database not cleaned between runs, file resources not cleaned up, test order dependencies.

Debugging commands:

```bash
# Run test repeatedly to reproduce flakiness
for i in {1..100}; do cargo test test_name || break; done

# Force serial execution to check for shared state
cargo test -- --test-threads=1

# Verbose logging to expose timing
RUST_LOG=debug cargo test test_name
```

Fixes include explicit waits instead of sleeps, seeded random generators, proper teardown in fixtures, and test isolation patterns (transactions, temp directories).

### Data Pipelines

Data pipeline bugs manifest as incorrect results, crashes on specific data, or performance issues. Common causes include schema mismatches between stages, null/missing value handling, data type conversions (precision loss, overflow), and encoding issues.

The agent validates data at pipeline stage boundaries, logs sample records at each transformation, and tests with edge cases (nulls, empty strings, extreme values).

### ML Models

ML debugging involves both code bugs and model behavior issues. The agent checks:

- **Training issues** — Loss not decreasing (learning rate), loss exploding (gradient issues), overfitting, underfitting.
- **Inference issues** — Prediction distribution shift, performance degradation over time, train/inference inconsistency, memory leaks in serving.

Root cause analysis includes verifying data preprocessing matches training, checking for label leakage, validating feature distributions between training and production, and using explainability tools (SHAP, attention weights).

## Examples

### Example 1: Systematic Null-Check Debugging

```python
# Step 1: Understand the error
# Error: TypeError: Cannot read property 'name' of undefined
# at processUser (src/users.py:45)

# Step 2: Add diagnostic logging
def process_user(user_id: str) -> dict:
    logger.debug(f"Processing user_id: {user_id}")
    user = get_user(user_id)
    logger.debug(f"Retrieved user: {user}")  # <-- User is None!
    return {"name": user.name}  # Crashes here

# Step 3: Fix with proper null handling
def process_user(user_id: str) -> dict:
    user = get_user(user_id)
    if user is None:
        logger.warning(f"User not found: {user_id}")
        raise UserNotFoundError(f"User {user_id} not found")
    return {"name": user.name}

# Step 4: Add regression test
def test_process_user_not_found():
    with pytest.raises(UserNotFoundError):
        process_user("nonexistent-id")
```

### Example 2: Git Bisect for Finding Bug Introduction

```bash
# Start bisect session
git bisect start

# Mark current commit as bad (has the bug)
git bisect bad

# Mark known good commit (before bug existed)
git bisect good v1.2.0

# Git checks out a middle commit — run the test
npm test

# Mark result and repeat
git bisect good  # or git bisect bad

# Git identifies the first bad commit
# Output: "abc123 is the first bad commit"
git show abc123

# End bisect session
git bisect reset
```

### Example 3: Common Bug Patterns

```python
# Pattern 1: Off-by-one errors
for i in range(len(items) - 1):  # Wrong — misses last element
    process(items[i])
for i in range(len(items)):      # Fixed
    process(items[i])

# Pattern 2: Race conditions
if not file.exists():
    file.create()  # Another thread may create between check and create
file.create_if_not_exists()  # Fixed — atomic operation

# Pattern 3: Floating point comparison
if 0.1 + 0.2 == 0.3:          # False due to IEEE 754
    do_something()
if abs((0.1 + 0.2) - 0.3) < 1e-9:  # Fixed — approximate comparison
    do_something()

# Pattern 4: Mutable default arguments
def add_item(item, items=[]):      # Bug — same list reused across calls
    items.append(item)
    return items
def add_item(item, items=None):    # Fixed — None default
    if items is None:
        items = []
    items.append(item)
    return items
```

### Example 4: Debugging a Flaky Test

```python
# Flaky test — fails intermittently due to shared database state
def test_user_registration():
    user = create_user(email="test@example.com")
    assert user.id is not None  # Sometimes: "User already exists"

# Fix: Add proper teardown for test isolation
@pytest.fixture(autouse=True)
def clean_database():
    yield
    User.query.delete()
    db.session.commit()

# Alternative fix: Use unique data per test run
def test_user_registration():
    email = f"test-{uuid.uuid4()}@example.com"
    user = create_user(email=email)
    assert user.id is not None
```
