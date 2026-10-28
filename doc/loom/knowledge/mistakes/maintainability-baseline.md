# Maintainability Baseline

> cargo test stops early; maintainability baseline

## Cargo stops at the first failing test target and hides later ones

**What happened:** a subagent ran `cargo test`, hit a failure in `tests/integration`, reran only that target with `--no-fail-fast`, and reported one failure. `tests/maintainability.rs` runs after it and was also failing on the change (a re-export pushed `fs/work_dir.rs` past its baseline line count, and a match grew `models_value_of` to 52 lines).
**Why:** cargo skips the remaining test binaries after the first failing one, so a partial rerun never reaches them.
**Prevention:** after any failure, run the whole suite again with `cargo test --no-fail-fast` and read every `test result` line before reporting.
**Fix:** shrink the offending code until it is back under the baseline in `tests/maintainability`, rather than adding a baseline entry.
