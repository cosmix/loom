# Unit wire

Implementer: codex, gpt-6.2-sol, effort xhigh. Wave: 1.

Read `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/common.md` in full first (the
settled decisions, the wire contract and, for codex units, "Rules for codex units"). Then read
the wire contract in `common.md` and the first two bullets of section 3 of `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/server.md`, and the `KickoffDraftDetail` pin in section 3 of `delete.md`.

## Files you own (write only these)

- `loom/src/plan_server/wire/chat.rs`
- `loom/src/plan_server/wire/kickoff.rs`
- `loom/src/plan_server/wire/tests_chat.rs`
- `loom/src/plan_server/wire/tests_schema.rs`
- `web/src/plan/api/fixtures/chat-commands.json`
- `web/src/plan/api/fixtures/kickoff-drafts.json`

## Entry points

`ChatCommand` in `wire/chat.rs`; `KickoffDraftView` and `KickoffDraftDetail` in `wire/kickoff.rs`.

## Done

`SetAsideKickoff.now`, `DiscardKickoff.delete_files`, `KickoffDraftView.size: KickoffSize` and
the two detail fields exist; both fixtures match the contract; the wire tests assert them; and
`cargo check --all-targets` shows no error in your files. Orchestrator proof:
`cargo test --lib plan_server::wire::`.

## Traps

The web units read these fixtures: their shape is exactly the contract in common.md. Consumers in `chat/` break until wave 2.
