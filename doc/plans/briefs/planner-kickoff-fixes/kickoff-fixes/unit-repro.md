# Unit repro

Implementer: Claude, loom-software-engineer (sonnet). It must run a fixture-backed dispatch test to confirm a cause, which a codex unit cannot. Wave: 0.

Read `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/common.md` in full first (the
settled decisions, the wire contract and, for codex units, "Rules for codex units"). Then read
section 0 of `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/server.md`.

## Files you own (write only these)

- `loom/src/plan_server/chat/tests_dispatch/set_aside.rs` (new)
- `loom/src/plan_server/chat/tests_dispatch.rs` (only the `mod set_aside;` line)

## Entry points

`draft_cmd::set_aside` in `loom/src/plan_server/chat/draft_cmd.rs`; the dispatch harness in `loom/src/plan_server/chat/tests_dispatch.rs` and its `drafts.rs` tests for how a kickoff is started and a turn left running.

## Done

The test exists, fails on the base tree for the reason it names, and the confirmed cause is
recorded with `loom memory note "set-aside cause: ..."`. If the cause is not the 600 s closing
write, report it to the orchestrator before wave 1 starts, so it can amend the `draft-commands`
brief; do not edit any other unit's file. Proof: `cargo test --lib plan_server::chat::tests_dispatch::set_aside` from `loom/`, failing on base.

## Traps

Keep the test after the fix: wave 2's `draft-commands` unit makes it pass. Mark nothing `#[ignore]`.
