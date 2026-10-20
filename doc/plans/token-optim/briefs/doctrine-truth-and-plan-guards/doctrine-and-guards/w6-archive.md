# W6: the archive abort and the failed-run distill test

Read `doc/plans/token-optim/briefs/doctrine-truth-and-plan-guards/doctrine-and-guards/common.md` first.

## Files you own

- `src/fs/memory/archive.rs`
- `src/fs/memory/tests/archive.rs` (declared from `src/fs/memory/mod.rs` as `#[path = "tests/archive.rs"] mod archive_tests;`, which W4 owns; you only edit the test file)
- `src/fs/plan_lifecycle.rs`
- `src/fs/plan_lifecycle_tests.rs`
- `src/commands/init/cleanup.rs`
- `src/commands/clean/mod.rs`
- `src/commands/memory/handlers/tests_resolve_pending.rs`

The stage `model` allowlist is C2's (a codex unit); do not touch `src/plan/schema/`.

## 1. Archive failure aborts cleanup

`archive_run_state` (`src/fs/memory/archive.rs`, lines 16-28) returns `Option<PathBuf>`: `None` means both "nothing to archive" and "archiving failed" (it prints a warning). Two callers then delete the state directory and one renames the plan to `DONE-`, so a failed copy loses every journal: `commands/init/cleanup.rs` `cleanup_work_directory` (about line 193), `commands/clean/mod.rs` `clean_state_directory` (about line 230), and `fs/plan_lifecycle.rs` `archive_run_state_before_done` (about lines 253-266, called from `mark_plan_done_if_all_merged` at about line 223).

1. `archive.rs`: `pub fn archive_run_state(work_dir: &Path, main_root: &Path, plan_id: Option<&str>) -> Result<Option<PathBuf>>`. `Ok(None)` when there is nothing to archive; `Ok(Some(path))` on success. When `try_archive_run_state` fails and `<work_dir>/memory` holds at least one `*.md` file, return the error; it must name the archive path (`create_dir_all` already has that context; wrap the copy loop in `.with_context(|| format!("archiving run state to {}", archive_path.display()))` too). When it fails and no journal exists (telemetry only), print the existing warning and return `Ok(None)`. Rewrite the doc comment to match.
2. `cleanup.rs` and `clean/mod.rs`: `archive_run_state(&work_dir, &main_root, plan_id.as_deref())?;` exactly, before `fs::remove_dir_all`. Both functions already return `Result`, and their callers propagate (`commands/init/execute.rs` `cleanup_work_directory(&repo_root)?`; `clean::execute` `clean_state_directory(repo_root)?`).
3. `plan_lifecycle.rs`: `archive_run_state_before_done(work_dir: &WorkDir, main_root: &Path) -> Result<()>`, called as `archive_run_state_before_done(work_dir, &root)?;` where `root` is the project root `mark_plan_done_if_all_merged` already resolved (with an error) before generating the review. Drop the old project-root fallback and its warning; it reads the plan id as today, calls `archive_run_state(work_dir.root(), main_root, plan_id.as_deref())?` and returns `Ok(())`. An error there leaves the plan `IN_PROGRESS` and retryable, as a review failure already does (the daemon logs the error, `src/daemon/server/orchestrator.rs`; `loom run` in the foreground propagates it, `commands/run/foreground.rs`). Update its doc comment and the doc of `mark_plan_done_if_all_merged` (archive errors propagate like review errors).
4. Tests. Block archiving by creating `<root>/.loom/memory` and writing a regular file at `<root>/.loom/memory/archive`.
   - `fs/memory/tests/archive.rs`: adapt the two existing tests to the `Result` (`.unwrap().unwrap()`, `.unwrap().is_none()`); add `archive_failure_without_journals_only_warns` (a `telemetry/` directory, no `memory/*.md`, blocked archive: `Ok(None)`). The case with journals is the frozen contract `archive_failure_with_journals_is_an_error` in `tests/doctrine_guard_contracts.rs`.
   - `cleanup.rs` `archive_tests`: add `cleanup_work_directory_keeps_state_when_archiving_fails`: a journal, a blocked archive, `cleanup_work_directory` returns `Err`, and `.loom/work/memory/stage.md` still exists.
   - `clean/mod.rs` tests: add `clean_state_directory_keeps_state_when_archiving_fails`, the same shape (the file is 361 lines; stay under 400).
   - `plan_lifecycle_tests.rs` (358 lines; stay under 400): add `archive_failure_prevents_done_and_retry_finishes`, modelled on `review_failure_prevents_done_and_retry_finishes`: a journal in `memory/`, a blocked archive, `mark_plan_done_if_all_merged` is `Err`, the `IN_PROGRESS-` plan still exists; remove the blocking file; the retry returns the `DONE-` path.

## 2. The failed-run distill procedure, run once as a test

W3 documents distilling a failed run in the README: start the new plan with `loom init`, then copy `<main>/.loom/memory/archive/<plan-id>-<timestamp>/memory/*.md` into its `.loom/work/memory/` before `loom run`; the new distill stage lists the copied entries in `loom memory pending --group`, and `loom memory pending --strict` needs a receipt for each. Prove that with the real functions, in `tests_resolve_pending.rs` (182 lines; it already imports `pending_report` and `strict_should_fail` from `super::pending`):

`archived_journals_copied_into_a_new_state_dir_are_pending`: in a `TempDir`, append a `MemoryEntryType::Note` to stage `old-stage` of `<temp>/old/.loom/work` with `crate::fs::memory::append_entry`; archive it with `archive_run_state(&old_work, temp.path(), Some("plan-a"))` (`.unwrap().unwrap()` gives the archive path); create `<temp>/new/.loom/work/memory` and copy every file of `<archive>/memory/` into it; `pending_report(&new_work, None)` lists the note's id in `pending`, and `strict_should_fail(true, <its pending count>)` is true. The test changes no environment variable and needs no `#[serial]`.

## 3. Confirm the retry path the README promises, read only

W3 writes in the README that, after a failed plan completion, running `loom run` again finds every stage merged and retries the completion step. The stage-model survey is not yours: `doc/plans/` is untracked, so this worktree cannot see the archived plans, and the plan records the planning-time survey. Read `src/orchestrator/core/run.rs` (`run`, `initialize_run`, `run_finished`), `src/daemon/server/orchestrator.rs` (`finish_orchestration`) and `src/commands/run/foreground.rs`, and confirm that a `loom run` of a plan whose stages are all merged reaches `mark_plan_done_if_all_merged` again. Record the answer with `loom memory note` and the file and line evidence, and put it in your report. If it does not, say so in the report: the main agent then routes a README correction to a fresh W3 worker. Do not change the daemon: what it reports after a completion failure is out of scope.

## Check (run once, at the end)

```bash
cargo test --lib -- fs::memory:: fs::plan_lifecycle:: commands::init:: commands::clean:: commands::memory::handlers::tests_resolve_pending
```

Report every caller you changed and the retry-path answer.
