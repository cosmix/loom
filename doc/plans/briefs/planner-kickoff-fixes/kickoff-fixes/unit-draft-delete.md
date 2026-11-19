# Unit draft-delete

Implementer: codex, gpt-6.2-sol, effort xhigh. Wave: 2.

Read `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/common.md` in full first (the
settled decisions, the wire contract and, for codex units, "Rules for codex units"). Then read
the "Delete" part of section 3 of `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/delete.md`.

## Files you own (write only these)

- `loom/src/plan_server/chat/drafts/delete.rs` (new)
- `loom/src/plan_server/chat/drafts/delete_tests.rs` (new)

## Entry points

`kickoff_target` in `loom/src/plan_session/scope.rs` and `safe_remove_tree_in_workdir`, `safe_remove_empty_dir_in_workdir` and `safe_open_dirfd` in `loom/src/fs/safe_fs.rs`, all written in wave 1 (read them on disk); `unheld` in `loom/src/plan_server/trace/settle_head.rs` for the `git cat-file` pattern, which runs through `crate::git::runner::run_git` in the code, never from your shell.

## Done

`written` and `remove_written` exist with the pinned signatures and every rule in delete.md has
its own test, including the symlinked-bundle and edited-target cases; `cargo check --all-targets`
shows no error in your files. Orchestrator proof: `cargo test --lib plan_server::chat::drafts::delete`.

## Traps

Security-sensitive. Derive every path from the stored target, checked byte for byte against `kickoff_target(size, name).document`; never widen that check. All filesystem access goes through `safe_fs`. `drafts.rs` (the `mod delete;` and `#[cfg(test)] mod delete_tests;` lines and the re-exports) is owned by `draft-store`: do not edit it; write `delete_tests.rs` as a child module of `drafts` that uses `super::delete::*`.
