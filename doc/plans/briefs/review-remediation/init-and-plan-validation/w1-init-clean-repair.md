# init-and-plan-validation / w1-init-clean-repair: init preflight order, repository root resolution, cleanup helper reuse

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/orchestrator.md`, sections R4, CR-09, CR-17, CR-53; `doc/plans/briefs/review-remediation/validation/cli-tui-webserver.md`, section CR-57 (sub-claims b and c only).

## Findings covered
- R4: `loom init <plan> --clean` deletes the previous run before two deterministic refusals (a dependency cycle, a detached HEAD). The cycle half is fixed by worker w2 in plan schema validation; this worker adds the branch half and the init-level tests.
- CR-17: `clean`, `init` and `repair` take `repo_root` from the process working directory, so a subdirectory or a stage worktree acts on the wrong tree. `run` is already guarded and stays untouched.
- CR-53: `init --clean` removes `.worktrees/*` entries with raw git calls that follow symlinks and skip id validation.
- CR-57(b): `loom repair --fix` exits 0 when fixes failed.
- CR-57(c): `fix_invalid_work` fails with ENOTDIR on a socket, FIFO or device.
- CR-09: `mark_plan_done_if_all_merged` resolves a relative `source_path` against the working directory.

## Files
Own (write), all under `loom/`:
- `src/fs/plan_lifecycle.rs`, `src/fs/plan_lifecycle_tests.rs` (one call site only), `src/fs/plan_lifecycle/relative_root_tests.rs` (new)
- `src/commands/common/mod.rs`
- `src/commands/init/execute.rs`, `src/commands/init/plan_setup.rs`, `src/commands/init/cleanup.rs`, `src/commands/init/mod.rs`, `src/commands/init/tests.rs` (one call site only), `src/commands/init/execute/tests.rs`, `src/commands/init/cleanup_worktree_tests.rs` (new)
- `src/commands/clean/mod.rs`, `src/commands/clean/worktrees.rs`
- `src/commands/repair.rs`, `src/commands/repair/workspace.rs`, `src/commands/repair/workspace/tests.rs`, `src/commands/repair/tests_actions.rs`

Read only: `src/git/revision.rs` (`toplevel`), `src/git/runner.rs` (`run_git_checked`), `src/git/cleanup/batch.rs` (`cleanup_multiple_stages`), `src/git/cleanup/config.rs` (`CleanupConfig::forced`), `src/git/target_guard/test_support.rs` (`repo`, `held_repo`), `tests/test_support/git_repo.rs` (`TestRepo`, `git`), `src/commands/init/base_branch.rs`.

Frozen (contract tests, never edit): `src/commands/init/execute_contract_tests.rs`, `src/commands/clean/root_contract_tests.rs`, `src/fs/plan_lifecycle_contract_tests.rs`.

Not yours: `src/commands/run/**` (leave `run` as it is), `src/git/**`, `src/plan/**`.

## Changes

Do them in this order. Item 1 first: CR-09 must land before anything that lets a command start from a subdirectory.

### 1. CR-09 (`src/fs/plan_lifecycle.rs`)
Seam: `mark_plan_done_if_all_merged` (about line 202) and `rename_completed_plan` (about line 257).
Today: the existence test `current_path.exists()` runs against the bare relative `source_path` before `root` is resolved, and `rename_completed_plan` runs `symlink_metadata` and `rename` on the bare relative paths. Both depend on the working directory.
Change:
- In `mark_plan_done_if_all_merged`: keep the early `get_plan_source_path`, prefix test and `if !all_stages_merged(work_dir)? { return Ok(None); }` in their order. Then resolve `root` (`work_dir.main_project_root().context("Cannot determine plan project root")?`, moved up from its current position), then `if !root.join(&current_path).exists() { return Ok(None); }`. Call `rename_completed_plan(work_dir, &root, &current_path, &new_path)`.
- New signature: `fn rename_completed_plan(work_dir: &WorkDir, root: &Path, current_path: &Path, new_path: &Path) -> Result<()>`. Inside, run `fs::symlink_metadata`, `fs::rename` and the rollback `fs::rename` on `root.join(new_path)` / `root.join(current_path)`. Keep passing the relative `current_path` and `new_path` unchanged to `update_plan_source_path` (config stays relative), to `commit_post_completion_changes` (git paths) and to the `println!`/error messages. `Path::join` with an absolute path returns it unchanged, so an absolute `source_path` (plan outside the repository) behaves as before.
- Return value of `mark_plan_done_if_all_merged` stays the relative `new_path`.
Only caller of `rename_completed_plan`: line about 229 and the test at `src/fs/plan_lifecycle_tests.rs` (`done_rename_rolls_back_if_config_update_fails`, last test of the file). Update that call to `rename_completed_plan(&work, temp.path(), &plan, &done)`; assertions stay. The file is 399 lines: add no line to it.

### 2. CR-17 helper (`src/commands/common/mod.rs`)
Add three functions (the file is 123 lines; keep helpers and their tests in this file):
- `pub(crate) fn repo_toplevel() -> Result<PathBuf>`: `repo_toplevel_in(&std::env::current_dir()?)`.
- `fn repo_toplevel_in(dir: &Path) -> PathBuf`: `crate::git::revision::toplevel(dir)`; on `Err` return `dir.to_path_buf()`. Comment the fallback: outside any work tree there is no repository to resolve, `loom init` creates one there (`ensure_repo_ready_for_worktrees`), and the other commands keep their previous cwd-relative behavior.
- `pub(crate) fn main_repo_root(command: &str) -> Result<PathBuf>`: `main_repo_root_in(command, &std::env::current_dir()?)`; and `fn main_repo_root_in(command: &str, dir: &Path) -> Result<PathBuf>`: `let root = repo_toplevel_in(dir)`; when `crate::git::revision::toplevel(dir)` succeeded (call it once and reuse its result; do not call git twice), run `run_git_checked(&["rev-parse", "--path-format=absolute", "--git-dir", "--git-common-dir"], &root)?` (same flags as `src/plan_server/root.rs` line 26); the output has two lines; when they differ the checkout is a linked worktree, so `bail!("loom {command} must run from the main checkout; {} is a linked worktree", root.display())`. Return `root`.
Decision: do NOT call `require_main_identity` here. `loom clean` is the remedy for state that has no identity anchor; refusing on a missing anchor would block recovery.

### 3. CR-17 callers
- `src/commands/clean/mod.rs` `execute`: replace `std::env::current_dir()?` with `crate::commands::common::main_repo_root("clean")?`. Everything downstream already takes `repo_root`.
- `src/commands/repair.rs` `execute`: replace `std::env::current_dir()?` with `crate::commands::common::repo_toplevel()?`. Repair does not refuse a linked worktree: `workspace::check` already treats a `.loom/work` symlink in a worktree as correct.
- `src/commands/init/execute.rs` `execute`: replace `std::env::current_dir()?` with `crate::commands::common::main_repo_root("init")?`; and in `create_init_work_dir` replace `WorkDir::new(".")?` with `WorkDir::new(repo_root)?` (the function already receives `repo_root`). The plan path argument stays relative to the working directory: `preflight_plan` canonicalizes it before anything else.

### 4. R4 branch half (`src/commands/init/execute.rs`, `plan_setup.rs`, `tests.rs`)
Seam: `execute` (line 70), `prepare_workspace` (line 107), `initialize_with_plan` (`plan_setup.rs` line 117, the `checked_out_branch(repo_root)?` at line 174).
Today the branch is read inside `initialize_with_plan`, after `cleanup_previous_run` deleted the state directory and `.worktrees`.
Change:
- `prepare_workspace(repo_root: &Path, clean: bool, no_repair: bool, needs_branch: bool) -> Result<Option<String>>`. Right after the `repo_bootstrap` value is computed (the `ensure_pinned_repo_ready` / `ensure_repo_ready_for_worktrees` pair, which may create the first commit: this keeps first-time bootstrap working) and BEFORE `startup_repairs`, compute `let base_branch = needs_branch.then(|| checked_out_branch(repo_root)).transpose()?;` (`use super::base_branch::checked_out_branch;` moves from `plan_setup.rs` to `execute.rs`). Return `Ok(base_branch)` at the end.
- In `execute`: `let base_branch = prepare_workspace(&repo_root, clean, no_repair, preflighted.is_some())?;` and replace `if let Some(preflighted) = preflighted {` by `if let (Some(preflighted), Some(base_branch)) = (preflighted, base_branch) {`; pass `&base_branch` on.
- `initialize_with_plan` new signature, exactly: `pub fn initialize_with_plan(work_dir: &WorkDir, plan: &PreflightedPlan, repo_root: &Path, base_branch: &str, terminal_backend: Option<SessionBackendKind>) -> Result<usize>`. Delete the `let base_branch = checked_out_branch(repo_root)?;` line and the import; `plan_table(parsed_plan, relative_source_path, base_branch)` takes the `&str`. Net line count of `plan_setup.rs` (392) and of `initialize_with_plan` (baseline 182) must not rise.
- Callers: `execute.rs` and `src/commands/init/tests.rs` line 58 (`init_from_stages`): pass `"main"` before `backend`; update the doc comment above it only as far as the detached-HEAD sentence no longer applies. No assertion changes.
The cycle half needs no code here: `preflight_plan_if_given` already runs `parse_plan` before `prepare_workspace`, and w2 makes schema validation refuse a cycle with the text `Circular dependency detected: a -> b -> a`.

### 5. CR-53 (`src/commands/init/cleanup.rs`, `src/commands/clean/worktrees.rs`)
- `cleanup_worktrees_directory(repo_root)` (line 120): replace the `read_dir` loop body. Collect stage ids with `fs::read_dir(&worktrees_dir).with_context(..)?`, each entry `.with_context(..)?`, keep names whose `entry.file_type().with_context(..)?.is_dir()` (`DirEntry::file_type` does not follow symlinks; a link is skipped). Call `crate::git::cleanup_multiple_stages(&ids, repo_root, &CleanupConfig::forced())` (re-exported at `crate::git`; `CleanupConfig` lives in `crate::git::cleanup`). It validates each id, removes the worktree, deletes `loom/<id>` and the stage cache. Print only warnings: for each `(stage_id, result)`, each `result.warnings` entry as `  ⚠ {stage_id}: {warning}` in the style of `clean/worktrees.rs` lines 235-243. Keep the trailing `worktree prune` and `remove_dir_all(.worktrees)`. Remove the now unused imports (`branch_name_for_stage`, and `run_git` only if still unused: `prune_stale_worktrees` and the trailing prune still use it).
- Order in `execute.rs::cleanup_previous_run`: call `cleanup_worktrees_directory(repo_root)?` BEFORE `cleanup_work_directory(repo_root)?` (swap the two lines). `cleanup_multiple_stages` drains each worktree's memory and telemetry spools into the state directory's journal before removing it; with the state directory already deleted that drain writes into a directory that no longer belongs to a run, and `archive_state_dir` would miss the drained entries. `loom clean --all` already runs worktrees first.
- `src/commands/clean/worktrees.rs` line about 210: replace `.filter(|entry| entry.path().is_dir())` with `.filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))` so both commands skip links before any git call.

### 6. CR-57(b) (`src/commands/repair.rs`)
`execute` is at its baseline of 99 lines (`loom/maintainability-baseline.txt`, `function src/commands/repair.rs execute 99`) and must not grow.
- Add `fn print_fix_summary(result: &RepairResult)` holding the `Summary` block now inline in the `if fix {` branch (the header, the three count lines and the `issues_failed` line); call it.
- Add `fn outcome(result: &RepairResult) -> Result<()> { if result.issues_failed > 0 { bail!("{} repair(s) failed", result.issues_failed) } Ok(()) }` (`use anyhow::{bail, Result};`).
- At the end of the fix branch `return outcome(&result);`. `main` turns the `Err` into exit 1. The dry-run path keeps exiting 0.

### 7. CR-57(c) (`src/commands/repair/workspace.rs`)
`fix_invalid_work`: replace `work_path.is_file()` with `match fs::symlink_metadata(&work_path)?` (context message naming the path): `remove_dir_all` only when `file_type().is_dir()`, otherwise `remove_file`. Keep the trailing `scrub_main_repo_settings_identity` call.

## Interfaces shared with other workers
None called from other workers. Observable dependency on w2: after w2's change `parse_plan` fails a cyclic plan with a message containing `Circular dependency detected: ` followed by the stage ids joined by ` -> `. Your test 1 needs it; w2 and w3 finish before you start.

## Tests
1. `src/commands/init/execute/tests.rs` (`CwdGuard`, `held_repo` exist there; add `#[serial]`): `init_clean_refuses_a_cyclic_plan_and_keeps_the_old_run`. Build state with `crate::git::target_guard::test_support::repo()`: write `repo.work.join("config.toml")` (any text, so `holds_orchestration_state` is true), a `keep.txt` in `repo.work`, and `repo.root/.worktrees/stage-a/keep.txt`. Write a two-stage plan `a -> b -> a` (copy the fixture shape of `create_test_plan` in `src/commands/init/tests.rs`, version 1, two `StageDefinition`s with `dependencies`). `execute(Some(plan), true, None, true)` must fail with a message containing `Circular dependency detected`, and both keep files must exist. Fails today (both deleted). Depends on w2.
2. Same file: `init_clean_from_a_subdirectory_refuses_like_the_root_does`: copy `init_refuses_unanchored_existing_sessions_without_deleting_them` (line 68), create `repo.root/src`, enter it with `CwdGuard`, call `execute(None, true, None, true)`; assert the error contains `Stop all sessions` and the session file is intact. Fails today: the cwd is taken as the root, the state is found by walking up and `--clean` deletes it.
3. Same file: `prepare_workspace_bootstraps_a_repository_with_no_commits`: `TestRepo::builder().branch("main").identity("t", "t@t.com").no_seed().create()` (see `src/commands/init/base_branch.rs` tests for the builder), call `prepare_workspace(root, false, true, true)` (the tests module reaches it through `use super::...`; add the import) and assert `Ok(Some("main"))`. Second case in the same test: `needs_branch = false` returns `Ok(None)`. Never call `execute` on a success path in a test: `configure_workspace` writes the real home directory.
4. `src/commands/init/cleanup_worktree_tests.rs` (new; register `#[cfg(test)] mod cleanup_worktree_tests;` in `init/mod.rs`): `a_symlink_in_worktrees_is_skipped_and_a_real_worktree_is_removed`. Repository with a commit from `TestRepo`; `git worktree add .worktrees/s1 -b loom/s1`; `.worktrees/link` symlink to a directory outside the repository holding `keep.txt`; branch `loom/link` created with `git branch loom/link`. After `cleanup_worktrees_directory(root)`: `keep.txt` still exists, `loom/link` still exists (`git rev-parse --verify refs/heads/loom/link`), `loom/s1` is gone, `.worktrees` is gone. The `loom/link` assertion fails today.
5. `src/commands/common/mod.rs` `mod tests` (extend; uses `TestRepo`): `repo_toplevel_in` of `<repo>/a/b` returns `<repo>` (compare canonicalized); of a plain temp dir returns that dir; `main_repo_root_in("clean", ..)` of a linked worktree (`git worktree add`) errors with `linked worktree`; of the main checkout and of a repository with no commits returns the root. None of these touch the working directory, so no `#[serial]`.
6. `src/commands/repair/tests_actions.rs`: `a_failed_fix_makes_the_outcome_an_error`: an issue with `action: RepairAction::Workspace(workspace::WorkspaceFix::WorkSymlink)` against an empty temp dir (`fix_work_symlink` fails: nothing to remove); `apply_fixes` returns `issues_failed == 1`; `outcome(&result)` is `Err` containing `1 repair(s) failed`; a result with `issues_failed == 0` gives `Ok`. Look at the other tests of the file for how a `DetectedIssue` is built.
7. `src/commands/repair/workspace/tests.rs`: `fix_invalid_work_removes_a_socket`: create `.loom/` in a temp dir, bind a `std::os::unix::net::UnixListener` at `.loom/work` (guard with `crate::process::sandbox_probe::skip_without_unix_socket("<test name>", &path)`, as `src/commands/init/execute/tests.rs` line 123 does), drop the listener, call `fix_invalid_work(root)`, assert `Ok` and the path gone. Fails today with ENOTDIR.
8. `src/fs/plan_lifecycle/relative_root_tests.rs` (new; add `#[cfg(test)] mod relative_root_tests;` next to `mod import_tests;` at the end of `plan_lifecycle.rs`; own fixture helpers, as `import_tests.rs` does): `done_rename_uses_the_project_root_not_the_working_directory`: `source_path = "doc/plans/IN_PROGRESS-PLAN-x.md"` (relative) in `config.toml` (copy the `write_config` format from `plan_lifecycle_tests.rs` line 18), the plan file under the temp root, one merged stage file. `#[serial]`, enter a different empty temp directory first (restore on drop). `mark_plan_done_if_all_merged` returns `Some`, the `DONE-PLAN-x.md` file exists under the temp root and the in-progress file is gone. Fails today: returns `Ok(None)`.

Tests 1 and 2 reach `configure_workspace` on the code as it is today (before the fix `execute` goes past the cleanup), and that function writes under `$HOME` (`install_codex_hooks`, `assets::install::default_paths`). Give both tests a guard that sets `HOME` to a temporary directory and restores it on drop (`src/stage_cache/contract_tests.rs` line 34 sets `HOME` the same way), together with `#[serial]`.

Contract tests this worker turns green (by id): `init-clean-keeps-old-run-on-cyclic-plan` (with w2), `init-clean-keeps-old-run-on-detached-head`, `clean-from-subdirectory-acts-on-root`, `clean-refuses-from-stage-worktree`, `plan-done-rename-ignores-process-cwd`.

CR-17 for `clean` has no extra unit test: the two clean contracts and test 5 cover it. The `worktrees.rs` filter change (item 5) is hygiene: `cleanup_worktree` already refused a symlink, so no test can fail on the old code.

## Proof
From `loom/`: `cargo check --all-targets`; `cargo test --lib commands::init::`, `cargo test --lib commands::clean::`, `cargo test --lib commands::repair::`, `cargo test --lib commands::common::`, `cargo test --lib fs::plan_lifecycle`; `cargo fmt` on the files you changed; `wc -l` on `execute.rs`, `plan_setup.rs`, `repair.rs` (none above its current line count plus 10, `repair.rs` `execute` not longer than 99 lines).

## Traps
- `clean::execute` and `init::execute` tests change the working directory: `#[serial]`, and restore it on drop.
- The `.worktrees/` check in `is_in_worktree` is a path-string test; do not use it for the refusal. Use the git-dir / common-dir comparison.
- `repo_toplevel_in` falls back to the directory on any git error. Do not turn that into a silent `unwrap_or_default`; the fallback is the documented contract and `init` depends on it to create the repository.
- Moving `checked_out_branch` earlier must come AFTER the bootstrap commit is created: a repository with zero commits otherwise fails the `at least one commit` refusal before init can create it.
- Do not duplicate the contract tests; the frozen contract files already assert the cyclic-plan and detached-HEAD refusals.
- Test 1 passes only because w2's schema change is already on disk when you start (you run after w2 and w3 finish). If `rg -n dependency_cycles src/plan/schema/validation.rs` finds nothing, stop and report: do not add a cycle check in the init code.
- A compile error in a file you do not own is reported, never fixed.
