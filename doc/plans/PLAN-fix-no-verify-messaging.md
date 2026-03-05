# Fix `loom stage complete --no-verify` misleading messaging

GitHub Issue: #11

## Context

`loom stage complete --no-verify` prints:

```
⚠️  Note: Merge was skipped. Stage NOT marked as merged.
⚠️  Dependent stages will NOT be automatically triggered.
```

This is incorrect. The `--no-verify` path marks the stage as Completed (with `merged=false`), but the orchestrator daemon's `completion_handler` then auto-merges the branch and triggers dependents. The feature works correctly — only the messaging and comments are wrong.

## Fix

**File:** `loom/src/commands/stage/complete.rs` (lines 392-403)

1. Update the comments (lines 393-397) to reflect that the daemon handles merge
2. Replace the two warning `println!` lines (400-402) with accurate messaging, e.g.:
   ```
   Stage '{stage_id}' completed (skipped verification).
   The orchestrator will handle merge and dependent triggering.
   ```

No behavioral changes needed.

## Verification

```bash
cd loom
cargo build
cargo test
cargo clippy -- -D warnings
```
