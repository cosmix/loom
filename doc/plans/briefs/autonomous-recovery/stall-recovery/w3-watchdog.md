# stall-recovery W3: a stalled scheduler is restarted by the daemon itself

Tier: opus. Wave 2, after C1 and W2 (W1 runs in the same wave; you use W1's `Tick.operation`).
Read `doc/plans/briefs/autonomous-recovery/common.md` first. Frozen contracts:
`src/orchestrator/stall_contract_tests.rs`.

## Why

`orchestrator::tick::Tick::is_stalled` is only displayed (`scheduling_report.rs::stalled_alert`
tells the operator to run `loom stop`, then `loom run`). With every scheduler-thread wait now
bounded (W1) and resolved-merge verification moved off the thread (W2), a tick older than the
longest bounded operation means something unforeseen blocks. The daemon's accept loop is a
separate thread that keeps running; it can restart the daemon through spawn-recovery's restart
facility (`daemon/server/restart.rs`).

## Interface (pinned)

New `src/orchestrator/watchdog.rs` (declared `pub(crate) mod watchdog;` in
`src/orchestrator/mod.rs`):

```rust
/// Longer than every bounded wait on the scheduler thread: git network 300 s, git mutation
/// 120 s, a lock wait 30 s, so a healthy tick never reaches it.
pub(crate) const STALL_RESTART_SECS: i64 = 900;
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Stall { pub operation: String, pub age_secs: i64 }
/// `Some(Stall)` once `tick` is at least `STALL_RESTART_SECS` old, whatever its phase;
/// `operation` is the tick's operation, else its phase name.
pub(crate) fn check(tick: &crate::orchestrator::tick::Tick, now: DateTime<Utc>) -> Option<Stall>;
```

## Edits (you own these files)

1. `src/orchestrator/watchdog.rs`, `src/orchestrator/mod.rs` (one module line).
2. `src/daemon/server/lifecycle.rs`: the accept loop, at most every 5 s, reads the tick
   (`tick::read`), calls `watchdog::check`, and on `Some(stall)` records
   `crate::orchestrator::restart_budget::RestartReason::Stalled { operation: stall.operation,
   age_secs: stall.age_secs }` on the `RestartSignal` spawn-recovery added (once per stall: a
   pending or refused request is not re-recorded until the tick moves again); the restart path then runs as spawn-recovery
   implemented it (successor first, then exit; the stuck orchestrator thread is abandoned
   after the existing 5 s join). `run_server` stays at or under its pinned 126 lines: add the
   check inside the helper spawn-recovery extracted, or extract another.
3. A stale tick from a daemon that crashed (its `tick::clear` never ran) must not restart a
   fresh daemon: the daemon stamps the tick (`tick::record(work_dir, Phase::Sync)`) before the
   accept loop's first watchdog check, and the accept loop ignores a tick recorded before the
   daemon's own start time. Test it.
4. Tests: `watchdog::check` at 899 s and 900 s; an `idle` phase past the limit also restarts
   (the loop died between ticks); a missing tick file never restarts (`tick::read` returns
   `None` before the first tick).

## Abandoned step: what the successor sees (state it in the module doc)

- A git mutation the stuck thread ran keeps running in its own process group and finishes;
  the successor's merges fail on `index.lock` until it does, then succeed.
- A stage whose landing was cut between the target's update and `merged = true`:
  `initialize_run`'s reconcile and the merge-state ancestry check (`git/merge/status.rs`) mark
  it merged from ancestry.
- A host job the old daemon ran: settled by `recover_after_restart`; a lost `Land` job is
  resubmitted (W2's step 5).
- A dispute between filing and judging: its state is on disk (`disputes/`), so the successor's
  first `check_pending_disputes` continues it.

## Done

Your one check after the stage compiles: `cargo test --lib orchestrator::watchdog`.
