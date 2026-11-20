# Brief: watch-progress (progress age in the subagent watch)

Tier: `loom-software-engineer` (sonnet). Read `doc/plans/briefs/open-github-issues/common.md` first.

## Owns

`loom/src/commands/subagents/wait/mod.rs` (contract scaffold line only, already added), `loom/src/commands/subagents/wait/native.rs`, `loom/src/commands/subagents/wait/output.rs`, `loom/src/commands/subagents/wait/native_tests.rs`, `loom/src/commands/subagents/wait/tests.rs`.

Frozen, read-only: `loom/src/commands/subagents/wait/progress_contract_tests.rs`.

## Facts (verified)

- `wait/native.rs` `outcome_for` maps a native record: Unverified ownership -> Unknown; non-terminal and quiet past the stall budget -> Stalled; non-terminal otherwise -> Active; terminal with Held ownership -> Active; then the terminal outcome. Progress is only a timestamp (`last_progress_at`) on the record.
- `wait/output.rs:98` renders the per-worker lines.

## Change

1. The Active outcome carries `progress_age_secs: Option<u64>` (now minus `last_progress_at`; None before the first progress event).
2. `output.rs`: an active codex worker line ends with `progress <n>s ago` or `no progress yet`. Stalled, failed, completed and unknown lines are unchanged.
3. Exit codes (0, 2, 3, 4, 5, 6) are unchanged.

## Tests

`native_tests.rs`: Active with an age; Active before any progress. `tests.rs`: the rendered line for both.

## Done

`cargo test --manifest-path loom/Cargo.toml --lib commands::subagents::wait::` passes with the frozen contract.
