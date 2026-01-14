---
name: debugging
description: Systematically diagnoses and resolves software bugs, test failures, data quality issues, and performance problems using various debugging techniques and tools. Trigger keywords: debug, bug, error, exception, crash, issue, troubleshoot, fix, stack trace, diagnosis, diagnose, investigate, root cause, why, failing, broken, not working, unexpected, flaky, intermittent, regression, performance degradation.
allowed-tools: Read, Grep, Glob, Bash, Edit
---

# Debugging

## Overview

This skill provides systematic approaches to finding and fixing bugs across all domains: application code, tests, data pipelines, ML models, and infrastructure. It covers debugging strategies, tool usage, and techniques for various types of issues including crashes, flaky tests, data quality problems, and model performance degradation.

## Instructions

### 1. Understand the Problem

- Reproduce the issue consistently
- Gather error messages and stack traces
- Identify when the bug was introduced
- Determine the expected vs actual behavior

### 2. Isolate the Issue

- Create minimal reproduction case
- Use binary search to narrow down cause
- Check recent changes (git bisect)
- Verify environment and dependencies

### 3. Diagnose Root Cause

- Add strategic logging
- Use debugger to step through code
- Analyze stack traces
- Check for common patterns

### 4. Fix and Verify

- Implement targeted fix
- Add regression test
- Verify fix doesn't introduce new issues
- Document the root cause

## Best Practices

1. **Reproduce First**: Never fix what you can't reproduce
2. **Read Error Messages**: They often contain the answer
3. **Check Recent Changes**: Most bugs are recently introduced
4. **Question Assumptions**: Verify what you think you know
5. **Isolate Variables**: Change one thing at a time
6. **Use Source Control**: Git bisect is powerful
7. **Write Tests**: Prove the bug exists, then prove it's fixed

## Specialized Debugging Domains

### Debugging Flaky Tests

Flaky tests pass/fail non-deterministically. Common root causes:

**Timing Issues:**

- Race conditions in async code
- Insufficient wait times for UI elements
- Network request timeouts
- Background jobs not completing

**Non-Deterministic State:**

- Random data generation without seeds
- Unordered collections (sets, map iteration)
- Floating-point precision issues
- Timestamp-based logic in tests

**Test Isolation Failures:**

- Shared global state between tests
- Database not cleaned between runs
- Files/resources not properly cleaned up
- Test execution order dependencies

**Debugging Techniques:**

```bash
# Run test multiple times to reproduce flakiness
for i in {1..100}; do cargo test test_name || break; done

# Run with verbose logging to expose timing
RUST_LOG=debug cargo test test_name

# Check for shared state issues
cargo test -- --test-threads=1  # Force serial execution

# Identify timing-dependent tests
cargo test -- --nocapture | grep -i "timeout\|sleep\|wait"
```

**Fixes:**

- Add explicit waits instead of arbitrary sleeps
- Seed random generators: `rand::thread_rng().seed(42)`
- Clean up state in test fixtures/teardown
- Use test isolation patterns (transactions, temp directories)
- Mock time-dependent code

### Debugging Data Pipelines

Data pipeline bugs manifest as incorrect results, crashes on specific data, or performance issues.

**Common Issues:**

- Schema mismatches between stages
- Null/missing value handling
- Data type conversions (precision loss, overflow)
- Encoding issues (UTF-8, special characters)
- Memory issues with large datasets

**Debugging Techniques:**

```python
# Sample problematic data for local debugging
df_sample = df.filter("problematic_condition").limit(1000)
df_sample.write.parquet("debug_sample.parquet")

# Add data quality assertions
assert df.filter(col("user_id").isNull()).count() == 0, "Null user_ids found"
assert df.filter(col("amount") < 0).count() == 0, "Negative amounts found"

# Profile memory and performance
df.explain()  # Show execution plan
df.cache()    # Materialize for profiling

# Check schema evolution issues
df.printSchema()
df.dtypes  # Verify expected types
```

**Root Cause Analysis:**

- Check upstream data sources for schema changes
- Validate data at pipeline stage boundaries
- Log sample records at each transformation
- Use data profiling tools to find anomalies
- Test with edge cases: nulls, empty strings, extreme values

### Debugging ML Models

ML debugging involves both code bugs and model behavior issues.

**Training Issues:**

- Loss not decreasing (learning rate, gradient flow)
- Loss exploding (gradient explosion, numerical instability)
- Overfitting (model memorizes training data)
- Underfitting (model too simple for data)

**Inference Issues:**

- Prediction distribution shift
- Performance degradation over time
- Inconsistent results between training/inference
- Memory leaks in model serving

**Debugging Techniques:**

```python
# Check gradient flow
for name, param in model.named_parameters():
    if param.grad is not None:
        print(f"{name}: grad norm = {param.grad.norm()}")
    else:
        print(f"{name}: NO GRADIENT")  # Dead layer!

# Detect numerical issues
torch.autograd.set_detect_anomaly(True)  # Catch NaN/Inf

# Validate data preprocessing
print("Training data stats:", train_data.mean(), train_data.std())
print("Inference data stats:", inference_data.mean(), inference_data.std())
# If stats differ significantly, preprocessing mismatch!

# Profile model performance
import torch.autograd.profiler as profiler
with profiler.profile(use_cuda=True) as prof:
    model(input_data)
print(prof.key_averages().table())

# Test on single example
model.eval()
with torch.no_grad():
    output = model(single_input)
    print(f"Input: {single_input}, Output: {output}")
```

**Root Cause Analysis:**

- Verify data preprocessing matches training
- Check for label leakage or data contamination
- Validate feature distributions (training vs production)
- Test model on known examples with expected outputs
- Use explainability tools (SHAP, attention weights)

## Examples

### Example 1: Systematic Debugging Process

```python
# Step 1: Understand the error
"""
Error: TypeError: Cannot read property 'name' of undefined
at processUser (src/users.py:45)
at handleRequest (src/server.py:123)
"""

# Step 2: Add diagnostic logging
def process_user(user_id: str) -> dict:
    logger.debug(f"Processing user_id: {user_id}")

    user = get_user(user_id)
    logger.debug(f"Retrieved user: {user}")  # <-- User is None!

    # Bug: No null check before accessing properties
    return {"name": user.name}  # Crashes here

# Step 3: Fix with proper null handling
def process_user(user_id: str) -> dict:
    logger.debug(f"Processing user_id: {user_id}")

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

# Git checks out middle commit, test it
# Run your test
npm test

# Mark result
git bisect good  # or git bisect bad

# Repeat until git identifies the first bad commit
# Git will output: "abc123 is the first bad commit"

# View the problematic commit
git show abc123

# End bisect session
git bisect reset
```

### Example 3: Common Bug Patterns

```python
# Pattern 1: Off-by-one errors
# Bug: Missing last element
for i in range(len(items) - 1):  # Wrong!
    process(items[i])
# Fix:
for i in range(len(items)):
    process(items[i])

# Pattern 2: Race conditions
# Bug: Check-then-act without synchronization
if not file.exists():
    file.create()  # Another thread might create between check and create
# Fix: Use atomic operations
file.create_if_not_exists()

# Pattern 3: Floating point comparison
# Bug: Direct equality comparison
if 0.1 + 0.2 == 0.3:  # This is False!
    do_something()
# Fix: Use approximate comparison
if abs((0.1 + 0.2) - 0.3) < 1e-9:
    do_something()

# Pattern 4: Mutable default arguments
# Bug: Shared mutable default
def add_item(item, items=[]):  # Same list instance reused!
    items.append(item)
    return items
# Fix: Use None default
def add_item(item, items=None):
    if items is None:
        items = []
    items.append(item)
    return items

# Pattern 5: Silent failures
# Bug: Swallowing exceptions
try:
    risky_operation()
except Exception:
    pass  # Bug hidden!
# Fix: Handle or re-raise appropriately
try:
    risky_operation()
except SpecificException as e:
    logger.error(f"Operation failed: {e}")
    raise
```

### Example 4: Debugging Tools Usage

```bash
# Python debugging
python -m pdb script.py  # Interactive debugger
python -m trace --trace script.py  # Trace execution

# Node.js debugging
node --inspect script.js  # Chrome DevTools
node --inspect-brk script.js  # Break on first line

# Rust debugging
RUST_BACKTRACE=1 cargo run  # Full stack trace
RUST_LOG=debug cargo run    # Verbose logging

# Memory profiling (Python)
python -m memory_profiler script.py

# CPU profiling (Python)
python -m cProfile -o output.prof script.py
python -m pstats output.prof

# Strace for system calls (Linux)
strace -f -e trace=file python script.py

# Network debugging
tcpdump -i any port 8080
curl -v http://localhost:8080/api/health
```

### Example 5: Debugging Flaky Test

```python
# Flaky test - fails intermittently
def test_user_registration():
    user = create_user(email="test@example.com")
    # Sometimes fails: "User already exists"
    assert user.id is not None

# Diagnosis: Test isolation failure - database not cleaned between runs

# Fix: Add proper teardown
@pytest.fixture(autouse=True)
def clean_database():
    yield
    # Clean up after each test
    User.query.delete()
    db.session.commit()

def test_user_registration():
    user = create_user(email="test@example.com")
    assert user.id is not None

# Alternative fix: Use unique data per test run
def test_user_registration():
    email = f"test-{uuid.uuid4()}@example.com"
    user = create_user(email=email)
    assert user.id is not None
```

### Example 6: Debugging Data Pipeline

```python
# Data pipeline crashes on production data but works on test data
def transform_orders(df):
    # Bug: Crashes when discount column has nulls
    df["final_price"] = df["price"] * (1 - df["discount"])
    return df

# Diagnosis: Add assertions to catch bad data early
def transform_orders(df):
    # Check assumptions about input data
    assert "price" in df.columns, "Missing price column"
    assert "discount" in df.columns, "Missing discount column"

    # Expose the bug
    null_discounts = df[df["discount"].isnull()]
    if len(null_discounts) > 0:
        print(f"Found {len(null_discounts)} orders with null discounts")
        print(null_discounts.head())

    # Fix: Handle nulls explicitly
    df["discount"] = df["discount"].fillna(0.0)
    df["final_price"] = df["price"] * (1 - df["discount"])
    return df

# Add data validation test
def test_transform_orders_handles_nulls():
    df = pd.DataFrame({
        "price": [100, 200],
        "discount": [0.1, None]  # Null discount
    })
    result = transform_orders(df)
    assert result["final_price"].tolist() == [90.0, 200.0]
```

### Example 7: Debugging ML Model Performance

```python
# Model accuracy dropped from 95% to 75% in production

# Step 1: Compare training vs production data distributions
train_stats = train_df.describe()
prod_stats = production_df.describe()
print("Feature drift detected:")
for col in train_stats.columns:
    train_mean = train_stats.loc["mean", col]
    prod_mean = prod_stats.loc["mean", col]
    drift = abs(prod_mean - train_mean) / train_mean
    if drift > 0.1:
        print(f"{col}: {drift*100:.1f}% drift")

# Step 2: Test on individual examples to find pattern
test_cases = [
    {"input": [...], "expected": 1, "predicted": model.predict([...])},
    {"input": [...], "expected": 0, "predicted": model.predict([...])},
]
for case in test_cases:
    if case["expected"] != case["predicted"]:
        print(f"Misprediction: {case}")

# Step 3: Root cause - feature preprocessing changed
# Training: features normalized with StandardScaler fit on training data
# Production: features normalized with different scaler parameters!

# Fix: Save and load scaler with model
import joblib
joblib.dump(scaler, "scaler.pkl")
# In production:
scaler = joblib.load("scaler.pkl")
features_scaled = scaler.transform(features)
predictions = model.predict(features_scaled)
```
