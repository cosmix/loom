# W1: busy module, tick format, provisioning on the shared helper

Read `doc/plans/briefs/scheduler-busy-status/busy-tick/common.md` in full first. It fixes every
signature named here.

## Files you own

- `loom/src/orchestrator/busy.rs` (new)
- `loom/src/orchestrator/busy_tests.rs` (new, `#[cfg(test)] #[path = "busy_tests.rs"] mod tests;`)
- `loom/src/orchestrator/mod.rs` (add `pub mod busy;` only)
- `loom/src/orchestrator/tick.rs`
- `loom/src/orchestrator/core/provision_gate.rs`
- `loom/src/orchestrator/core/provision_gate_tests.rs`
- `loom/src/orchestrator/core/provision_gate_pin_tests.rs` (a submodule of
  `provision_gate_tests.rs`; it calls `provision_worktree`)

Do not edit the frozen contract file `loom/tests/scheduler_busy_contracts.rs`; make it pass.

## Tasks

1. **`tick.rs`.**
   - Add `busy: Option<Busy>` to `Tick`, `record_busy`, and parsing of line 3 per common.md. A
     bad third line (wrong prefix, unknown op, bad timestamp, empty, over-long or bad stage id)
     gives `busy: None` and leaves the tick readable. Update every `Tick { .. }` literal in this
     file's tests (no other file builds this type).
   - Make `record` and `record_busy` write `orchestrator.tick.tmp` and rename it over
     `orchestrator.tick` (common.md, "The tick file format"). Keep both best-effort.
   - Rewrite the doc comment on `STALL_THRESHOLD_SECS`: it currently says "a slow git
     operation or a bounded 5s teardown never trips it" (the stage's `after_stage` checks that
     the phrase "never trips it" is gone). Say instead that the threshold applies to unwrapped
     work, and that wrapped long operations re-stamp the tick through `busy::while_busy`.
     Update the module doc to describe the third line. Keep `Tick::is_stalled`.
2. **`busy.rs`.** Implement the constants, `BusyOp`, `Busy`, `while_busy`, `while_busy_every`,
   `LoopState` and `loop_state` from common.md. Base the helper on `run_with_fresh_tick` /
   `keep_tick_fresh` / `StopTicker` from `provision_gate.rs`, and keep their panic safety: a
   `std::thread::scope`, a park loop, and a stop guard that sets the flag and unparks. Then
   **move** them: delete the three from `provision_gate.rs`. Changes from today's design:
   - Compute `started_at` once. Call `tick::record_busy` on the CALLING thread before spawning
     the helper and before calling `f`. The helper re-stamps only after each interval has
     elapsed; it writes no stamp of its own at start. Today `keep_tick_fresh` writes its first
     stamp on the helper thread, which races the frozen contract that reads the tick from
     inside the closure.
   - Park for `interval.min(Duration::from_secs(1))`, not a fixed 1 s.
   - Spawn the helper with `std::thread::Builder::new().spawn_scoped(scope, ..)`; on `Err`,
     run `f` without a helper (the synchronous stamp still stands). Never panic on a failed
     spawn.
   - Write the two-line clearing record only after the helper thread has been joined, on both
     return and unwind. Do it from a guard created OUTSIDE the scope: its `Drop` runs after
     the scope has joined the helper (`std::thread::scope` waits for every scoped thread before
     it resumes the unwind). Otherwise a late helper write could re-add the busy line after the
     clear.
   - `while_busy` calls `while_busy_every(.., REFRESH_INTERVAL, f)`. `while_busy_every` is
     `pub` and `#[doc(hidden)]`: the frozen contract `busy_line_restamps_while_running` calls
     it with 200 ms.
   - Optionally `debug_assert!` in `record_busy` that the stage id passes the reader's rules,
     so a busy line this code writes can never read back as "not busy".
3. **`provision_gate.rs`.** `provision_worktree` takes `stage_id: &str` and wraps EACH entry in
   its own call, in order, stopping at the first failure:
   `entries.iter().try_for_each(|entry| busy::while_busy(work_dir, Phase::Spawning, BusyOp::Provisioning, stage_id, || run_provision(std::slice::from_ref(entry), worktree_path)))`.
   `PROVISION_TIMEOUT` bounds one command, so one wrap per entry keeps every wrap inside the
   600 s provisioning ceiling; a single wrap around all entries would let several healthy slow
   entries read as a stall (plan Decision 5). Do not edit `orchestrator/provision.rs`.
   Remove `TICK_INTERVAL` and the now-unused imports. Fix all three callers:
   `pre_spawn_gates_passed` in `provision_gate.rs` (pass `&stage.id`),
   `provisioning_stamps_the_daemon_tick` in `provision_gate_tests.rs`, and the call in
   `provision_gate_pin_tests.rs`.
4. **`provision_gate_tests.rs`.** The one tick test is `provisioning_stamps_the_daemon_tick`. It
   asserts no tick before the call and phase `Spawning` after it returns; it never looks
   during the run, and it passes unchanged once the clearing write is `record(Spawning)`. Keep
   it as it is apart from the new `stage_id` argument. Add a NEW test that proves the busy line
   during the run: give `provision_worktree(work, "stage-a", ..)` one entry whose command copies
   the tick into the worktree, e.g. `entry(&format!("cp {} seen.tick", work.path().join("orchestrator.tick").display()))`,
   then parse `worktree/seen.tick` with `tick::read` (copy it into a TempDir named as a work dir,
   or read it as text) and assert the busy line names `provisioning` and `stage-a`. Check how
   `entry(..)` and `run_provision` run a command (shell or argv) before you write it. If you
   change any existing assertion line, record it in `loom memory` for the integrity dispute.
   Add a second NEW test for the per-entry wrap: two entries, the first copying the tick to
   `seen-1.tick` and then running `sleep 1`, the second copying it to `seen-2.tick`; assert both
   copies carry a busy line naming `provisioning` and `stage-a`, and that the second's
   `started_at` is strictly later than the first's (one wrap per entry, not one for the run).
5. **`busy_tests.rs`.** Cover in-crate what the contracts do not:
   - every `loop_state` rule boundary, at 59/60 s and at ceiling minus one and the ceiling itself;
   - `BusyOp::parse(as_str(op)) == Some(op)` for all three;
   - `while_busy` returning `f`'s value;
   - the clearing record after a normal return leaves the phase `while_busy` was given;
   - `record` and `record_busy` leave no `orchestrator.tick.tmp` behind.

## Done when

`cargo test --manifest-path loom/Cargo.toml --lib orchestrator::busy` and
`--lib orchestrator::tick` and `--lib orchestrator::core::provision_gate` pass, and the contracts
that exercise `while_busy` and `tick::read` pass:

```bash
cargo test --manifest-path loom/Cargo.toml --test scheduler_busy_contracts
```

The alert contracts and `web_snapshot_carries_the_busy_line` stay red until W2; the contracts
that must pass after you are `busy_line_clears_after_panic`,
`busy_line_names_op_and_stage_while_running`, `busy_line_restamps_while_running` and
`malformed_busy_line_reads_as_not_busy`. Use the `loom-rust` and `loom-concurrency` skills.
Report the files changed and anything you could not settle.
