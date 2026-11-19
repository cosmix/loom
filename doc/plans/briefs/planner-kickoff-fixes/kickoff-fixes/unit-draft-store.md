# Unit draft-store

Implementer: codex, gpt-6.2-sol, effort xhigh. Wave: 2.

Read `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/common.md` in full first (the
settled decisions, the wire contract and, for codex units, "Rules for codex units"). Then read
section 3 of `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/delete.md` up to and including "Landing".

## Files you own (write only these)

- `loom/src/plan_server/chat/drafts.rs`
- `loom/src/plan_server/chat/drafts/store.rs`
- `loom/src/plan_server/chat/drafts/handlers.rs`
- `loom/src/plan_server/chat/drafts/encode.rs`
- `loom/src/plan_server/chat/tests_drafts.rs`

## Entry points

`Draft`, `Seed` and `Draft::view` in `drafts.rs`; `Files::landed` and `spellings` in `drafts/store.rs`; `detail` in `drafts/handlers.rs`. Wave 1 changed `loom/src/plan_server/wire/kickoff.rs` (`KickoffDraftView.size` is `KickoffSize`; the detail has `written` and `committed`) and `loom/src/plan_session/scope.rs` (`pub fn kickoff_target`); read those files on disk, the navigation kit does not see them.

## Done

`Draft.size`, `Draft.finished`, `Seed.size` and the new landing rule exist; `drafts.rs`
declares `mod delete;` and `#[cfg(test)] mod delete_tests;` and re-exports `written` and `remove_written` (written by `draft-delete`
in this wave, signatures pinned in delete.md); `detail` fills `written` and `committed`; the
module comment states the new rule; tests are written; `cargo check --all-targets` shows no error
in your files. Orchestrator proof: `cargo test --lib plan_server::chat::drafts`.

## Traps

A draft whose bare target exists but `finished` is false has NOT landed. An old draft file without `size` fails to parse and is skipped with the existing warning; add no migration.
