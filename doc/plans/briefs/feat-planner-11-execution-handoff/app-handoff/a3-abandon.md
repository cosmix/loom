# A3: Abandon run

SEED: every file you own already exists as a one-line placeholder (the orchestrator created it so the crate compiles at every wave); replace it whole.

Stage `app-handoff` of `doc/plans/PLAN-feat-planner-11-execution-handoff.md`. Tier: opus
(`loom-senior-software-engineer`, effort high: a destructive sequence whose every refusal must
stop it). Never run git yourself, never spawn subagents. One narrow check at the end:
`cargo test --lib abandon` from `loom/`. Frozen contract files `tests/plan_handoff_contracts.rs`
and `tests/plan_handoff_contracts/**` are READ-ONLY.

## Files you own (relative to `loom/`)

- `src/plan_server/run/abandon.rs` (ends with `#[cfg(test)] mod tests;`)
- `src/plan_server/run/abandon/tests.rs`
- `src/commands/clean/worktrees.rs` (extract the unmerged listing)
- `src/commands/clean/mod.rs` (only a `pub(crate) use` line if needed)

`maintainability-baseline.txt` is NOT yours to edit: see "Ledger" below.

A2 owns `src/plan_server/run/mod.rs` and adds `mod abandon;` and your routes; it puts a
`RunJobs` extension (one job at a time) in `ctx.extensions()`; use it.

## Spec

`doc/product/section_11.md` lines 239-275 ("Abandoning an unfinished run"); section 01 lines
88-97 (retired state).

## Extraction in `commands/clean/worktrees.rs`

`confirm_branch_deletion` (`:71-~145`, ledger row `function src/commands/clean/worktrees.rs
confirm_branch_deletion 73`) computes the unmerged list inline (`:72-91`). Move that part into

```rust
pub(crate) struct UnmergedBranches { pub target_branch: String, pub branches: Vec<(String, usize)> }
pub(crate) fn unmerged_loom_branches(repo_root: &Path) -> UnmergedBranches
```

keeping its rules exactly: target from `load_config(..).base_branch()` through
`resolve_target_branch`, `list_loom_branches(..).unwrap_or_default()`, and
`commits_ahead_of(..).unwrap_or(1)` (a git error counts as unmerged). `confirm_branch_deletion`
calls it; its prompts, `LOOM_CLEAN_YES` handling and non-TTY refusal stay as they are. The
existing clean tests must pass unchanged.

## Ledger

`loom/maintainability-baseline.txt` is an exact ledger: `confirm_branch_deletion` shrinking
below 73 lines fails `cargo test --test maintainability` until its row is lowered or removed
(the row goes away when the function is 50 lines or fewer). The ORCHESTRATOR edits that row
after you report the new length; report it.

## `abandon.rs`

`pub(crate) fn view(ctx: &PlanContext) -> Option<AbandonView>` (A2's state route calls it when
the workspace is `Running` or `Stopped`): plan path from `get_plan_source_path`; merged and
unmerged stage ids from `collect_status_data(&work_dir)` (`data.merge.merged` vs every other
stage id in `data.stages`); `unmerged_branches` and `base_branch` from
`unmerged_loom_branches(root)`.

`POST /api/run/abandon` (Holder; refused in read-only mode; refused 409 `busy` while a job
runs; refused 409 `not_abandonable` unless the workspace is `Running` or `Stopped`, i.e. its
plan is `IN_PROGRESS-`). Body `AbandonRequest { confirmed_unmerged: Vec<UnmergedBranch> }`:
the exact (branch, ahead) list the dialog's second confirmation showed, empty when the dialog
showed none (px-state-15). The sequence function is `pub fn run_abandon(root: &Path, runtime: &RuntimePaths, loom_bin:
&Path, confirmed_unmerged: &[(String, usize)], on_line: &mut dyn FnMut(&str)) ->
anyhow::Result<AbandonOutcome>` with `AbandonOutcome::{Renamed { to }, Refused { reason },
BranchesChanged { now }, Failed { step, code }}`; the route maps `BranchesChanged` to 409
`unmerged_branches` carrying `now`.

1. Recompute `unmerged_loom_branches(root)` NOW (never trust the browser's earlier view). If
   the list, sorted, differs from `confirmed_unmerged` sorted: `BranchesChanged { now }`, and
   nothing runs (section 11: the user confirms the branches they were shown, and declining the
   second confirmation changes nothing).
1b. Before anything runs, write the abandon marker `<runtime dir>/abandon/<slug>.json`
   (the SERVER's runtime directory, `runtime.dir` of the `&RuntimePaths` the route passes from
   `ctx.runtime()`; never a fresh `runtime_paths(root)`, which can name another directory when
   `LOOM_PLAN_STATE` is set; `abandon/` created 0700, the file
   0600 through `crate::fs::safe_fs`, no-follow) holding `{ "plan": "doc/plans/IN_PROGRESS-PLAN-<slug>.md",
   "started_at": <RFC 3339> }`. It records that an abandon began HERE. Remove it after a
   successful rename, and after any outcome that leaves the run's state in place (a refusal or
   failure before `loom clean` changed anything: `BranchesChanged`, a failed `loom stop`, a
   failed `loom clean`); keep it when `loom clean` ran and the rename did not happen.
2. Run `<current_exe> stop` (A2's `run_streamed`, job `Abandon`, step `loom stop`); a non-zero
   exit stops the sequence.
2b. After `loom stop` succeeds, recompute `unmerged_loom_branches(root)` AGAIN (a stage may
   have committed while the daemon stopped, px-security-08) and compare it with
   `confirmed_unmerged` the same way; a difference ends with `BranchesChanged { now }` and
   nothing further runs.
3. Run `<current_exe> clean --all` with stdin null; set `LOOM_CLEAN_YES=1` (step 2b's list
   equals what the holder confirmed, so every branch it deletes was shown and confirmed, or there
   is none); `env_remove` it in every other child. A non-zero exit stops (that includes `refuse_unreviewed_move`,
   `src/commands/target/mod.rs:45`, which `begin_clean` calls first).
4. `loom clean` exits 0 when it declines (`src/commands/clean/mod.rs` `execute`: "Aborted —
   nothing was deleted." then `Ok(())`). So confirm the outcome on disk: `.loom/work` does not
   exist (`symlink_metadata` is NotFound) AND none of the run's worktrees remain (every
   directory under `<root>/.worktrees/` that `git worktree list --porcelain` lists, or that
   matches a stage id of the run, is gone) AND no `loom/*` branch remains
   (`list_loom_branches`). If any remains: stop, keep the plan's `IN_PROGRESS-` name, and end
   with a `done` frame whose step is `verify` and line
   `loom clean left the run's state in place; the plan keeps its name.`.
5. Rename through the no-follow primitives, never `std::fs` (section 12:499-502):
   `crate::fs::safe_fs::safe_open_dirfd(root)`; refuse (Refused, no rename) when
   `planning::paths::symlink_component(root, Path::new("doc/plans/IN_PROGRESS-PLAN-<slug>.md"))`
   or the same for `doc/plans/archive/RETIRED-PLAN-<slug>.md` is `Some`, when the source is not a
   regular file (`symlink_metadata`), or when the target exists in any form; create
   `doc/plans/archive` with `safe_fs::safe_create_dir_all_in_workdir` when absent; then
   `safe_fs::safe_rename_in_workdir(dirfd.as_raw_fd(), "doc/plans/IN_PROGRESS-PLAN-<slug>.md",
   "doc/plans/archive/RETIRED-PLAN-<slug>.md")`. Steps 1-5 of this item are `fn rename_to_retired(root, slug)`. Commit nothing (the next run dialog lists the
   rename under uncommitted changes). The plan's brief bundle stays where it is.

Each step's outcome streams as `RunOutput` frames (A2's `child.rs`); the last frame is `done`
with the exit code of the step that ended the sequence (0 after a successful rename).

## Finish abandoning (px-state-16)

`pub fn stranded_plan(root: &Path, runtime: &RuntimePaths) -> Option<PathBuf>`: an abandon marker
`<runtime dir>/abandon/<slug>.json` exists AND names that plan, `.loom/work` absent, the plan
`doc/plans/IN_PROGRESS-PLAN-<slug>.md` exists (plan 1's `plan_state` = Running), no `loom/*`
branch (`list_loom_branches`) and no `.worktrees/*` directory: that plan. Without the marker it
is `None`: a plan running from another clone, or a fresh clone, looks the same on disk and must
never be offered (px-state-R04). `finish_abandon` removes the marker after its rename. A2 reports it as
`RunStateView.finish_abandon`. `pub fn finish_abandon(root: &Path, runtime: &RuntimePaths) -> anyhow::Result<AbandonOutcome>`
re-checks `stranded_plan` and runs `rename_to_retired` alone; a branch or worktree left is
`Refused` naming it. Route `POST /api/run/finish-abandon` (Holder, not in read-only mode, 409
`busy` while a job runs). Plan 11 alone detects this state (plan 1's checklist reads only `doc/`, section 01 "When
the state is recomputed").

All children of this sequence run in their own process group (A2's `child.rs`), so a first
Ctrl-C on `loom plan` does not kill `loom stop` or `loom clean` mid-step.

## Tests (`abandon/tests.rs`)

On a fixture git repo in a `TempDir` with a fake `loom` executable (a shell script you write in
the test whose behaviour each test sets: exit codes, whether it deletes `.loom/work`), driving
the sequence function directly (`run_abandon(root, runtime, loom_bin, confirmed_unmerged, on_line)`, so
tests pass the fake binary): a confirmed list that differs from the recompute runs nothing; the
matching list passes `LOOM_CLEAN_YES=1` (the fake records its env); a list that grows between
the view and the request answers `BranchesChanged`; a fake `loom stop` that commits on a
`loom/*` branch ends with `BranchesChanged` before `loom clean` runs; `loom clean` exiting 0 but leaving `.loom/work` → no
rename; `loom stop` failing → nothing after it; a successful sequence renames into a created
`archive/`; an existing target refuses the rename; a symlinked plan file, a symlinked
`doc/plans/archive` and a symlinked `doc/plans` each refuse it with the link target untouched;
`stranded_plan`/`finish_abandon` with and without a leftover branch; the same stranded tree
WITHOUT the marker gives `stranded_plan == None` and `finish_abandon` refuses; the marker is
removed after a rename and after `BranchesChanged`; with `LOOM_PLAN_STATE` set to another
directory, a marker `run_abandon` wrote under the given `RuntimePaths` is found by
`stranded_plan` given the same `RuntimePaths`.

Also, each its own test: the route refuses `not_abandonable` for a `Done` and an `Absent`
workspace and `busy` while another job runs, and its `RouteSpec` role is `Role::Holder`; `view`
on a fixture workspace lists merged and unmerged stage ids and the unmerged branches;
`unmerged_loom_branches` counts a branch whose ahead-count git cannot compute (a target branch
that does not exist) as unmerged; a fake `loom clean` exiting 1 (as `refuse_unreviewed_move`
does) stops the sequence with no rename.

Every file under 400 lines, functions under 50.
