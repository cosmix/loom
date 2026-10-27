# chat-server C5: restore decision

Codex unit (gpt-6.1-sol, effort high). Working dir: loom/.

## Files

- Owns: `src/plan_server/review/restore.rs`; `src/plan_server/review/restore_tests.rs`
- May read: `doc/plans/PLAN-feat-planner-09-chat.md (or its IN_PROGRESS- spelling) stage chat-server PUBLIC SURFACE bullet restore.rs (authoritative)`; `plan 2 functions plan_server::wire::paths::to_doc_rel`

## Entry points

PinnedFile, SaveResult, RestoreResult, pub fn restore_file(path, pin, turn_running, force, save) -> RestoreResult, pub fn save_path_of(changed_path: &str) -> Option<&str>

## Steps

1. Implement restore_file exactly as the PUBLIC SURFACE states (pure; the save function is a parameter; refusals never call save).
2. save_path_of wraps to_doc_rel: doc/plans/PLAN-x.md gives Some(plans/PLAN-x.md); a path outside doc/ or one failing check_relpath_syntax gives None.
3. In restore_tests.rs (declared by the orchestrator in mod.rs as a cfg(test) module; start it with use super::restore::*;) write tests deleted_file_refused_without_save, invalid_maps_to_invalid, refused_maps_to_refused, save_path_of_strips_doc_prefix, save_path_of_refuses_outside_doc; the frozen contracts in tests/plan_review_contracts.rs cover the rest.

## Done

The module compiles and the frozen restore contracts pass when the orchestrator runs them.

Proof (the orchestrator runs it after you): `cargo test --lib plan_server::review::restore_tests:: && cargo test --test plan_review_contracts`

## Traps

The production caller restore_files lives in routes.rs (another worker), not here; do not import plan 3 or plan 7 code in this file.

Rules: Rust 2021, no new dependency, no unwrap outside tests, keep the file under 360 lines and
every function under 45 (cargo fmt grows them; the limits are 400 and 50). Double literal braces
inside format!. Put CLI placeholders inside backticks in rustdoc. Your own check is compile-only:
cargo test --lib --no-run from loom/. Do not run git. Do not touch any dot-loom or dot-work path.
