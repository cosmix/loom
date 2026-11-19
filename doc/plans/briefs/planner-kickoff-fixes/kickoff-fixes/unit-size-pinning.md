# Unit size-pinning

Implementer: codex, gpt-6.2-sol, effort xhigh. Wave: 2.

Read `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/common.md` in full first (the
settled decisions, the wire contract and, for codex units, "Rules for codex units"). Then read
section 2 of `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/server.md` except the `StartDraft::of` and `continue_draft` bullets, and the size tests of its section 5.

## Files you own (write only these)

- `loom/src/plan_server/chat/team.rs`
- `loom/src/plan_server/chat/kickoff.rs`
- `loom/src/plan_server/chat/kickoff_cmd.rs`
- `loom/src/plan_server/chat/kickoff_tests.rs`
- `loom/src/plan_server/chat/tests_dispatch/kickoff.rs`

## Entry points

`prepare_scope` and `kickoff_scopes` in `team.rs`; `KickoffTracker::observe` and `seed` in `kickoff.rs`; `record_owner` in `kickoff_cmd.rs`. Wave 1 changed `loom/src/plan_session/scope.rs` (`SessionScope::kickoff(root, size)`, `kickoff_resumed`, `ArmRefusal::WrongSize`); read it on disk.

## Done

`prepare_scope` writes the offer's size (and `kickoff_resumed` when `ChatOwner.resume` is set);
`kickoff_scopes` holds the size; `KickoffTracker::observe` and `seed` overwrite the shape's
size; the tests cover a feature kickoff whose block says plan and a refused `confirm_kickoff`;
`cargo check --all-targets` shows no error in your files. Orchestrator proof:
`cargo test --lib plan_server::chat::kickoff` and `cargo test --lib plan_server::chat::tests_dispatch::kickoff`.

## Traps

`ChatOwner.resume: Option<KickoffGrant>` is added in `dispatch.rs` by `draft-commands` in this same wave: read the field, do not add it. `StartDraft::of` and `continue_draft` are `draft-commands` work.
