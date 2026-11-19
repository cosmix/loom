# Worker B: per-end bound and end cause in the session bridge

Read `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/common.md` first.

Territory brief for the codex unit `bridge` (`unit-bridge.md`), which owns every file in the
Files section below.

## What to build

The bridge needs two things the kickoff commands use:

1. **A per-end bound.** `EndPlan` (`loom/src/plan_session/bridge/session.rs:180`) gains
   `bound: Option<Duration>`. `Session::plan_end` (`loom/src/plan_session/bridge/ending.rs`,
   "The first end request sets the plan; a later one may only harden it") keeps its signature and
   creates the plan with `bound: None`. Add, in `loom/src/plan_session/bridge/commands.rs` next to
   `Bridge::end`:

   ```rust
   /// `end` whose final write must finish within `bound` instead of the configured
   /// `final_write_bound`. A later call may lower the bound, never raise it.
   pub fn end_within(
       &self,
       id: &str,
       mode: EndMode,
       cause: EndCause,
       bound: Duration,
   ) -> Result<Vec<String>, BridgeError>
   ```

   It runs the same `request_end` path as `end`, then sets `plan.bound` to
   `Some(min(existing, bound))`, then flushes. `end_due` (`ending.rs:283`) uses
   `plan.bound.unwrap_or(bound)` for its `late` test. `Bridge::end` is unchanged in behaviour.
   A second `end`/`end_within` with `EndMode::Teardown` on a session whose plan is `FinalWrite`
   must tear it down at the next `end_step` (the existing hardening; keep it and test it through
   `end_within`).

2. **The cause on `Ended`.** `EndPlan` gains `cause: EndCause`, set by `plan_end`.
   `BridgeEvent::Ended` (`loom/src/plan_session/bridge/mod.rs:169`) gains
   `cause: Option<EndCause>`: `Some(plan.cause)` when the session had an end plan, `None` when it
   ended without one (the process died, a `SessionEnd` nobody asked for). The recovery teardown
   at server start (`loom/src/plan_session/bridge/recovery.rs:162`) sends `None`. `EndCause`
   derives `Debug, Clone, Copy, PartialEq, Eq` already; keep it internal (no serde needed).

Update every exhaustive `BridgeEvent::Ended { id, kind, outcome }` pattern you own so it
compiles (`loom/src/plan_server/summaries/rebuilds/session.rs:38` takes `..`). Do NOT edit
`loom/src/plan_server/chat/mod.rs`: worker S owns it and adds the field there. The patterns in
`loom/src/plan_server/pressure/driver/step.rs:246` and `bridge/harness.rs:239` already use `..`.

## Tests (in the existing bridge test files you own)

- `end_within` with a 1 s bound on a confirmed-style authoring session that never answers its
  closing message is torn down after the bound, while `end` alone would wait the harness's 30 s
  `final_write_bound` (`bridge/harness.rs:65`). Use the harness clock the existing stopping tests
  use (`loom/src/plan_session/bridge/tests_stopping.rs`).
- A `FinalWrite` end followed by an `end_within(..., Teardown, ...)` is torn down at the next
  `end_step` and emits one `Ended`.
- `Ended.cause` is `Some(EndCause::Agent)` for an end the done fence requested
  (`bridge/done_fence.rs:17`), `Some(EndCause::Holder)` for a holder end, `None` for a session
  whose process vanished.

Run `cargo test --lib plan_session::bridge::` from `loom/`.

## Files

`loom/src/plan_session/bridge/session.rs`, `ending.rs`, `commands.rs`, `mod.rs`,
`recovery.rs`, the bridge test files (`tests_*.rs`, `closing_contract_tests.rs`, `harness.rs`),
and `loom/src/plan_server/summaries/rebuilds/session.rs` (pattern only). `ending.rs` is 367
lines: if your change pushes it past 400, move `end_due` and `plan_end` into a new
`bridge/end_plan.rs` module.
