# Unit scope-size

Implementer: codex, gpt-6.2-sol, effort xhigh. Wave: 1.

Read `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/common.md` in full first (the
settled decisions, the wire contract and, for codex units, "Rules for codex units"). Then read
section 1 of `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/delete.md`.

## Files you own (write only these)

- `loom/src/plan_session/scope.rs`
- `loom/src/plan_session/scope/arm.rs` (new)
- `loom/src/plan_session/scope_tests.rs`

## Entry points

`arm_kickoff`, `refuse_existing`, `refuse_live` and the private `target` in `loom/src/plan_session/scope.rs`; `is_feature_name` in `loom/src/planning/kinds.rs`.

## Done

`KickoffScope.size`, `SessionScope::kickoff(root, size)`, `SessionScope::kickoff_resumed`,
`ArmRefusal::WrongSize` and `pub fn kickoff_target` exist as pinned, the arming code lives in
`scope/arm.rs`, the new tests are written, and `cargo check --all-targets` shows no error in
your files. Orchestrator proof: `cargo test --lib plan_session::scope`.

## Traps

Callers of `SessionScope::kickoff` outside your files (`chat/team.rs`) break until wave 2; leave them. `kickoff_resumed` skips the existing-target refusals for its own target only, never for another path.
