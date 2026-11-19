# Unit bridge

Implementer: codex, gpt-6.2-sol, effort xhigh. Wave: 1.

Read `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/common.md` in full first (the
settled decisions, the wire contract and, for codex units, "Rules for codex units"). Then read
`doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/bridge.md` in full.

## Files you own (write only these)

- `loom/src/plan_session/bridge/session.rs`
- `loom/src/plan_session/bridge/ending.rs`
- `loom/src/plan_session/bridge/end_plan.rs` (new, only if `ending.rs` would pass 360 lines)
- `loom/src/plan_session/bridge/commands.rs`
- `loom/src/plan_session/bridge/mod.rs`
- `loom/src/plan_session/bridge/recovery.rs`
- `loom/src/plan_session/bridge/harness.rs`
- `loom/src/plan_session/bridge/tests_stopping.rs`
- `loom/src/plan_session/bridge/closing_contract_tests.rs`
- `loom/src/plan_server/summaries/rebuilds/session.rs` (the `Ended` pattern only)

## Entry points

`EndPlan` in `session.rs`; `Session::plan_end` and `end_due` in `ending.rs`; `Bridge::end` in `commands.rs`; `BridgeEvent` in `mod.rs`; the teardown in `recovery.rs`.

## Done

`Bridge::end_within` and `BridgeEvent::Ended.cause` exist exactly as pinned, the three tests in
bridge.md are written, and `cargo check --all-targets` from `loom/` shows no error in your
files. The orchestrator proves the unit with `cargo test --lib plan_session::bridge::`.

## Traps

`loom/src/plan_server/chat/mod.rs` matches `Ended` exhaustively and is owned by `draft-commands`: its compile error is expected until wave 2. A later end may lower the bound, never raise it. Recovery sends `cause: None`.
