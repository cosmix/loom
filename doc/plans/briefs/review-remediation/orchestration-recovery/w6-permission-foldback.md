# orchestration-recovery / w6-permission-foldback: only the main checkout's own settings feed approved.json

Tier: opus
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/security.md`, section CR-01.

## Findings covered

- CR-01: the permission fold-back reads `settings.local.json` files a session can write (its working directory's and five fixed subdirectories', under a worktree or under the main checkout for checkout-rooted sessions) and records their allow rules, such as `Bash(*)`, into the loom-owned `approved.json` that every later session capsule allows; the reader follows symlinks, has no size cap and blocks on a FIFO inside the daemon; one malformed file aborts the whole fold-back.

## Decision

Only the main checkout's own `<repo_root>/.claude/settings.local.json` is a fold-back source. Claude Code writes a user's "don't ask again" approvals there (curated knowledge, `architecture/security-and-isolation.md`, "Where a Session's Write Grants Come From"); no session can write it. Every worktree file and every subdirectory file stops being read, the worktree root's included. It is read with the bounded no-follow reader. A read or parse failure logs a warning and contributes nothing; nothing can abort the fold-back. The blanket-rule policy of `ControlSurfaces::names` (rules such as `Bash(*)` coming from the main checkout's own file) stays as it is.

## Files

Own (write):
- `loom/src/fs/permissions/sync.rs`
- `loom/src/fs/permissions/mod.rs`
- `loom/src/fs/permissions/sync/tests.rs`
- `loom/src/fs/permissions/tests/sync_tests.rs`
- `loom/src/fs/permissions/tests/sync_read_deny_tests.rs`
- `loom/src/fs/permissions/tests/foldback_source_tests.rs` (new; declare it in `fs/permissions/tests/mod.rs`, which you also own for that one line)
- `loom/src/commands/stage/complete.rs`
- `loom/src/commands/stage/complete_force.rs`
- `loom/src/orchestrator/core/inbox_drain/sweep.rs`
- `loom/src/commands/stage/complete_pipeline.rs` (one call, line 155; outside the territory assigned to this stage, see the stage draft)
- `loom/src/orchestrator/core/merge_handler/resolver_exit.rs` (one method, lines 89-103; outside the territory assigned to this stage, see the stage draft)
- `loom/src/orchestrator/core/merge_handler/resolver_exit_tests.rs` (two tests; outside the territory assigned to this stage, see the stage draft)

Read only: `loom/src/fs/safe_read.rs`, `loom/src/fs/permissions/approved.rs`, `loom/src/fs/permissions/state_root.rs`, `loom/src/orchestrator/core/crash_handler.rs` (its call site belongs to `w1-dead-worker-recovery`).

Frozen (contract tests, never edit): `loom/src/fs/permissions/foldback_contract_tests.rs`.

## Changes

### 1. The fold-back (`fs/permissions/sync.rs`)

- Delete `sync_worktree_permissions`, `sync_worktree_permissions_with_working_dir`, `settings_paths_to_check`, `read_settings` and `SyncResult` (its `deny_added` and `worktrees_updated` are always 0).
- Add `pub fn fold_back_approvals(main_repo_path: &Path) -> usize`: read `crate::fs::safe_read::read_to_string_bounded(main_repo_path, Path::new(".claude/settings.local.json"), MAX_SETTINGS_BYTES)` with `const MAX_SETTINGS_BYTES: usize = 1 << 20;`. A not-found error (`crate::fs::safe_read::is_not_found`) means no rules; any other read error, a UTF-8 error or a JSON parse error logs `tracing::warn!(path = %.., error = %format!("{error:#}"), "cannot read the main checkout's local settings for approved permissions")` and means no rules. Pass the allow rules through `portable_permissions(extract_permissions(&settings).0)` (sorted and deduplicated), then `super::approved::record_fold_back(main_repo_path, &rules)`, and return its count. Fold the remaining part of `record_approvals` into it and delete `record_approvals`.
- Rewrite the module doc and the function doc for the new source: one file, the main checkout's, never a worktree or a subdirectory, because those are writable by a session. Keep the "never writes either settings file" sentence. Keep `extract_permissions`, `portable_permissions`, `normalize_single_slash_path`, `is_worktree_specific_permission` and `transform_worktree_path` unchanged (a rule recorded in the main file can still carry `../` or `.worktrees/` paths). If `extract_permissions`' deny half has no caller left, keep the function as is: its tests in `sync/tests.rs` use it.
- `fs/permissions/mod.rs:44`: `pub use sync::fold_back_approvals;`.

### 2. Callers

- `commands/stage/complete.rs:202-237`: the wrapper becomes `pub(super) fn sync_worktree_permissions(working_dir: &Option<PathBuf>)`. It still finds the main checkout with `find_repo_root_from_cwd(dir)`, calls `fold_back_approvals(root)` and, when the count is above zero, prints `Recorded {n} approved permission(s) from the main checkout's local settings`. Drop the `acceptance_dir` parameter and the deny/propagation message branches. Update the import at line 8.
- `commands/stage/complete_force.rs:173`: `sync_worktree_permissions(&execution_paths.worktree_root);`.
- `commands/stage/complete_pipeline.rs:155`: `sync_worktree_permissions(worktree_root);` (`acceptance_dir` stays bound; it is used at line 167).
- `orchestrator/core/inbox_drain/sweep.rs`: replace the call at line 109 with `crate::fs::permissions::fold_back_approvals(host.repo_root());` and delete `fold_back_permissions` (lines 128-147); its checkout-exists check guarded a worktree read that no longer happens. Remove imports that become unused.
- `orchestrator/core/merge_handler/resolver_exit.rs:89-103`: `fold_back_resolver_permissions` becomes `fn fold_back_resolver_permissions(&self)` (the stage id has no use left) and calls `crate::fs::permissions::fold_back_approvals(&self.config.repo_root);` with no worktree check and no error branch. Update the call at line 55 and its doc comment: the resolver's approvals land in the main checkout's file, so the order against the worktree removal no longer matters.
- `orchestrator/core/crash_handler.rs` is changed by `w1-dead-worker-recovery` to call `fold_back_approvals(&self.config.repo_root)`; do not edit it.

### 3. Existing tests (assertions change: report each one)

Every fixture that wrote the worktree's file moves to `<main>/.claude/settings.local.json` and every call becomes `fold_back_approvals(main_dir.path())` returning a count:
- `tests/sync_tests.rs`: `test_sync_basic_permissions` (2 added; drop the deny assertion and the `assert_main_settings_never_created` call, since the fixture is now that file), `test_sync_drops_inert_write_rules` (1), `test_sync_transforms_worktree_paths` (3), `test_sync_deduplicates` (the main file lists `Read(src/**)` twice and `Edit(tests/**)` once: 2 added, list length 2), `test_sync_missing_worktree_settings` renamed `test_sync_missing_main_settings` (0), `test_sync_never_creates_main_settings` (no main file: 0 added and the file still absent), `test_sync_leaves_main_settings_byte_identical` (only the main file; bytes unchanged and `Read(existing/**)` recorded), `test_sync_idempotent` (2, then 0).
- `tests/sync_read_deny_tests.rs`: the fixture moves to the main file; `sync_with_mixed_read_denies` returns `(usize, Value)`; `test_sync_never_promotes_read_denies` asserts 1 added.
- `orchestrator/core/merge_handler/resolver_exit_tests.rs`: `approvals_in_the_resolver_worktree_are_recorded` becomes `approvals_in_the_main_checkout_are_recorded` (the rule is written to `<repo>/.claude/settings.local.json`; call `fold_back_resolver_permissions()`); `a_missing_worktree_folds_nothing_back_and_does_not_fail` becomes `the_resolver_worktree_settings_are_not_a_source` (the rule is written to the worktree's file only; after the call `approved.json` is absent or lacks it). Update the module doc.
- `sync/tests.rs`: `single_slash_paths_are_normalized_to_cwd_relative_before_recording` and `fold_back_records_approvals_through_the_control_surface_filter` put all their rules in the main file (expected lists unchanged); `fold_back_leaves_both_local_settings_files_untouched` keeps both read-only files, calls `fold_back_approvals(&repo)` and asserts both byte-identical.

## Interfaces shared with other workers

Provides `pub fn fold_back_approvals(main_repo_path: &Path) -> usize` (re-exported as `crate::fs::permissions::fold_back_approvals`), called by `w1-dead-worker-recovery` from `crash_handler.rs` and by the frozen contract file.

## Tests

In `tests/foldback_source_tests.rs` (reuse the `create_state_root` / `approved_allow` shapes of `sync_tests.rs:20-37`):
- `session_writable_files_contribute_nothing`: write `{"permissions":{"allow":["Bash(*)","Edit(//home/u/.bashrc)"]}}` at `<main>/loom/.claude/settings.local.json`, `<main>/.worktrees/s1/.claude/settings.local.json` and `<main>/.worktrees/s1/loom/.claude/settings.local.json`, and `WebFetch(domain:docs.rs)` in the main file; the approved list is exactly `["WebFetch(domain:docs.rs)"]`. Today all three files are read.
- `a_fifo_main_settings_file_returns_promptly`: `nix::unistd::mkfifo` (pattern `tokens/tail_tests.rs:63`) at the main file; run `fold_back_approvals` on a thread and `recv_timeout(Duration::from_secs(10))`; it returns 0. Today it blocks.
- `a_symlinked_main_settings_file_is_not_followed`: the main file is a symlink to a JSON file elsewhere holding `Bash(cargo:*)`; 0 recorded.
- `an_oversized_or_malformed_main_file_records_nothing`: a file over 1 MiB, then `{not json`; 0 each, no panic.

Contract to turn green: `foldback-reads-only-main-checkout` (file `loom/src/fs/permissions/foldback_contract_tests.rs`).

## Proof

From `loom/`: `cargo test --lib fs::permissions::`, `cargo test --lib commands::stage::`, `cargo test --lib orchestrator::core::inbox_drain::`, `cargo test --lib orchestrator::core::merge_handler::`. `cargo check --all-targets` must be clean once `w1-dead-worker-recovery` has updated `crash_handler.rs`; until then its old call is the only expected error.

## Traps

- Keeping the subdirectory scan "only under the main checkout" is still a hole: a checkout-rooted session (knowledge, merge) can write `<main>/loom/.claude/settings.local.json`.
- `Path::exists` and `fs::read_to_string` both follow symlinks and block on a FIFO; use only `read_to_string_bounded`.
- Turning a read error into `Err` for the caller brings back the abort: the function returns a count and logs.
- Do not touch `ControlSurfaces::names` or `approved.rs`; the blanket-rule policy is out of scope.
- `inbox_drain/tests_sweep.rs::a_session_whose_worktree_vanished_is_retired_without_a_fold_back` must keep passing unchanged: with no main settings file `record_approved` writes nothing (`approved.rs:61`), so `approved.json` stays absent.
