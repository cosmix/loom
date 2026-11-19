# Unit draft-commands

Implementer: codex, gpt-6.2-sol, effort xhigh. Wave: 2.

Read `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/common.md` in full first (the
settled decisions, the wire contract and, for codex units, "Rules for codex units"). Then read
sections 2 (the `StartDraft::of` and `continue_draft` bullets), 3 (from `draft_cmd::set_aside` on), 4 and the remaining tests of section 5 of `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/server.md`.

## Files you own (write only these)

- `loom/src/plan_server/chat/draft_cmd.rs`
- `loom/src/plan_server/chat/draft_continue.rs` (new, when `draft_cmd.rs` would pass 360 lines)
- `loom/src/plan_server/chat/dispatch.rs`
- `loom/src/plan_server/chat/mod.rs`
- `loom/src/plan_server/chat/tests_dispatch/drafts.rs`
- `loom/src/plan_server/chat/tests_dispatch/draft_commands.rs`

## Entry points

`set_aside`, `discard`, `resume`, `StartDraft::of` and `continue_draft` in `draft_cmd.rs`; the `SetAsideKickoff` and `DiscardKickoff` arms and `ChatOwner` in `dispatch.rs`; the `BridgeEvent::Ended` match in `mod.rs`. Wave 1 wrote `Bridge::end_within` and `Ended.cause` (`loom/src/plan_session/bridge/`), the wire fields (`loom/src/plan_server/wire/`), and `DraftContinuation.target` (`loom/src/planning/guidance/continuation.rs`); `draft-store` and `draft-delete` write `Draft.size`, `Draft.finished`, `Seed.size`, `drafts::written` and `drafts::remove_written` in this wave. Read wave-1 files on disk; code to the pins for the rest.

## Done

Set aside, Stop now, the end cause, Continue with a resumed grant and Delete behave as section 3
and 4 say; the dispatch arms pass `now` and `delete_files`; the tests are written; the whole
crate compiles: `cargo check --all-targets` from `loom/` is clean, since you are the last unit
on the chat module. Orchestrator proof: `cargo test --lib plan_server::chat::` (including
`tests_dispatch::set_aside` from the `repro` unit, which must now pass).

## Traps

If the `repro` unit's recorded cause differs from the 600 s closing write, the orchestrator amends this brief before you start: follow the amendment. An interrupt with no running turn returns an error that is expected and ignored. Never derive delete paths from the request.
