# knowledge-and-graph / w3-reconcile-lock: one reconcile lease per checkout

Tier: codex sol
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/knowledge-graph.md`, section CR-07.

You run no `git` command, touch no path under `.loom/`, and run no test. Your proof is `cargo check --all-targets` from `loom/`. Other workers edit other files at the same time; you see the base tree, and none of their work affects yours.

## Findings covered

- CR-07: `reconcile_lock_path(store)` returns `store.root().join("reconcile.lock")`, and every stage worktree shares the main project's context store, so all checkouts share one lease. A hook in worktree B that arrives while worktree A reconciles only sets the shared `pending` bit; A's extra pass re-reconciles A's own target, so B's request is never served, and one checkout's failure backoff throttles all.

## Files

Own (write):
- `loom/src/commands/hook/reconcile_graph.rs`
- `loom/src/commands/hook/reconcile_graph/lock.rs`
- `loom/src/commands/hook/reconcile_graph/tests_read_lock.rs`
- `loom/src/commands/hook/reconcile_graph/tests_wants_rebuild.rs`
- `loom/src/commands/hook/tests_reconcile_graph.rs`

Read only: `loom/src/commands/hook/reconcile_graph/cancel.rs` (`cancel_holder(lock_path: &Path, now)` takes the path; it never builds one), `loom/src/commands/hook/reconcile_graph/tests_lock.rs`, `tests_rebind.rs`, `tests_wants_rebuild_cancel.rs` (they build their own lock paths in temp directories and stay valid), `loom/src/context/local_overlay.rs` (`local_overlay_stage_name`), `loom/src/commands/hook/target.rs` (`HookTarget::project_root`).

Frozen (contract tests, never edit): `loom/src/commands/hook/reconcile_graph/lease_contract_tests.rs`.

## Changes

### 1. The lease path takes the checkout (`reconcile_graph/lock.rs`, symbol `reconcile_lock_path`)

Change `pub(super) fn reconcile_lock_path(store: &ContextStore) -> PathBuf` to `pub(super) fn reconcile_lock_path(store: &ContextStore, project_root: &Path) -> PathBuf` returning `store.root().join(format!("reconcile-{}.lock", crate::context::local_overlay::local_overlay_stage_name(project_root)))`. `local_overlay_stage_name` is `map-<sha256 of the canonical root>`, filename-safe, and canonicalises, so two spellings of one checkout agree. A stage worktree is its own `project_root` (`WorkDir::project_root()` is the checkout containing the state directory, not the main root), so this is also the per-stage key.

Replace the `LOCK_FILE` constant (`"reconcile.lock"`) by two constants for the prefix `"reconcile-"` and the suffix `".lock"`, or inline them in the one function, whichever keeps the file smaller; `lock.rs` is 369 lines and must not grow. Update the module doc (first lines, "The `reconcile.lock` lease") and the doc of `reconcile_lock_path` to say one lease file per checkout lives in the context cache directory. Do not change the lock line encoding, `decide`, `update_lock` or any other function: `update_lock` still takes the cache directory's exclusive flock for each short read-modify-write, which is shared across checkouts and held only for that moment.

### 2. The three call sites pass the checkout (`reconcile_graph.rs`)

- `try_reconcile`: `reconcile_lock_path(&store, &target.project_root)`.
- `try_cancel`: it currently discards the target (`if let Some((_, store)) = resolve_store()?`); bind it and call `cancel::cancel_holder(&reconcile_lock_path(&store, &target.project_root), unix_now())`.
- `try_spawn(project_root, now)`: `reconcile_lock_path(&store, project_root)`.
The spawned child resolves the same checkout (`spawn_detached` sets `current_dir(project_root)` and passes `LOOM_STAGE_ID` and `LOOM_WORK_DIR`; `HookTarget::from_environment` yields the same `project_root`), and the claim handed over through `LOOM_RECONCILE_CLAIM` is read from the same per-checkout file.
Update the module doc paragraph "Lease and backoff" (it names `reconcile.lock`) and the doc of `spawn_if_needed`.

### 3. Tests that call the old signature

Every call of `reconcile_lock_path(&store)` gains the checkout root the test already has in scope:
- `loom/src/commands/hook/tests_reconcile_graph.rs`: the calls in `reconcile_graph_leaves_a_finished_marker_after_a_successful_run` (`root`), `reconcile_graph_with_no_resolvable_work_dir_creates_nothing` (`temp.path()`), `spawn_if_needed_does_nothing_when_the_pack_is_healthy`, `spawn_if_needed_leaves_a_young_live_lock_untouched`, `spawn_is_refused_against_an_inferred_root_with_no_existing_cache`, `spawn_is_allowed_against_an_inferred_root_that_already_has_a_cache`, `spawn_is_allowed_when_loom_work_dir_was_explicitly_set` (each has `root`), and any other call `rg 'reconcile_lock_path' loom/src` finds in that file.
- `loom/src/commands/hook/reconcile_graph/tests_wants_rebuild.rs`: `never_built_degraded_pack_spawns_nothing` and `two_failed_runs_count_two_failures` (`temp.path()`).
- `loom/src/commands/hook/reconcile_graph/tests_read_lock.rs`: it uses the removed `LOCK_FILE` constant in `lock_with` and in `a_fifo_in_place_of_the_lock_reads_as_no_lock_without_blocking`; declare `const LOCK_FILE: &str = "reconcile.lock";` at the top of that test file instead (the file name is arbitrary there: it tests `read_lock` against a path).
No assertion changes in these tests.

### 4. Lock cleanup (answer, no code)

Nothing unlinks lock files: the `lock.rs` module doc says "Nothing here ever unlinks the lock", and `loom/src/commands/clean/**` has no reference to the reconcile lease (`rg reconcile loom/src/commands/clean` is empty). After this change the context cache directory gains one small file (under 100 bytes) per checkout that ever ran a hook; removing `.loom/cache` removes them. Pruning is not part of this change and `commands/clean/**` is not yours; do not edit it. State this in your report.

## Interfaces shared with other workers

None. The frozen contract calls `reconcile_lock_path(&ContextStore, &Path) -> PathBuf` and the private `try_claim(lock_path: &Path, now: u64, debounce_secs: u64, stale_lock_secs: u64, is_alive: impl Fn(u32) -> bool, pid: u32) -> Claim` with `Claim::Won` and `Claim::Queued`, both unchanged apart from the new parameter above.

## Tests (write them; the orchestrator runs them)

In `reconcile_graph/tests_read_lock.rs` (68 lines; it has `use super::*;` and `tempfile::TempDir`):
- `lease_paths_differ_per_checkout`: `ContextStore::with_root(<temp>/cache)`, two checkout temp directories A and B; `reconcile_lock_path(&store, a) != reconcile_lock_path(&store, b)`; calling it twice for A gives the same path; both paths have the store root as parent and a file name starting `reconcile-` and ending `.lock`.
- `a_live_holder_in_one_checkout_does_not_queue_a_request_from_another`: with the two paths above and `try_claim` reached as `use super::super::{try_claim, Claim};` (the same import `tests_lock.rs` has: both files are child modules of `lock`, so `super::super` is `reconcile_graph`), `try_claim(&lock_a, 1_000, 600, 1_800, |_| true, 4_242)` is `Claim::Won`, then `try_claim(&lock_b, 1_001, 600, 1_800, |_| true, 4_343)` is `Claim::Won` (today the second is `Claim::Queued` behind the first because both use one file), and the line in `lock_a` still has `pending == false`.
Tests that must keep passing unchanged: all of `cargo test --lib commands::hook::reconcile_graph::` and `commands::hook::tests_reconcile_graph`.

## Proof

From `loom/`: `cargo check --all-targets`, and nothing else.

## Traps

- Passing `target.project_root` in `try_cancel` requires keeping the `HookTarget` returned by `resolve_store`; the current code throws it away.
- A test helper named `lock_in(&temp)` in `tests_wants_rebuild.rs` and `tests_wants_rebuild_cancel.rs` builds `temp.path().join("reconcile.lock")` directly and calls `seed`, `cancel_holder`, `read_lock` on it; it never goes through `reconcile_lock_path`, so leave it alone.
- Do not canonicalise the root yourself; `local_overlay_stage_name` does.
- `reconcile_graph.rs` is 372 lines: no new helper functions, only the three edits and doc text.
