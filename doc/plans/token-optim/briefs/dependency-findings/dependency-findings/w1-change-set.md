# W1: the accepted change set behind the StageDependency boost

Tier: sonnet (`loom-software-engineer`). Wave 2: you start after the three codex units of wave 1 returned and the orchestrator checked them. Read `doc/plans/token-optim/briefs/dependency-findings/dependency-findings/common.md` first; its section 3 pins the signatures you write and call, its section 5 lists what earlier plans changed. Paths are relative to `loom/`.

## What you own (write only these)

- `src/orchestrator/signals/retrieval.rs`
- `src/orchestrator/signals/retrieval_tests.rs` (new: the moved tests)
- `src/orchestrator/signals/retrieval_change_set_tests.rs` (new)
- `src/commands/hook/tests_worker_brief.rs` (one module declaration)
- `src/commands/hook/tests_worker_brief_dependency.rs` (new)
- `src/commands/hook/worker_brief/test_support.rs` (only in the fallback of Task 4)

Read-only: `src/git/merge/control_paths.rs` (C2 wrote `AcceptedBase` and `accepted_change_base`; `changed_paths`), `src/context/rank_source/paths.rs` (C3 wrote `join_working_dir`), `src/git/merge/mod.rs` (`merge_stage`), `src/git/merge/test_support.rs` (`isolated_git`), `src/fs/mod.rs` (`resolve_target_branch_from_config`), `src/verify/transitions/persistence.rs` (`load_stage`, `save_stage`), `src/commands/hook/worker_brief.rs` (`retrieve_pack`, `issue`, `Config`), `src/orchestrator/signals/tests_brief_e2e.rs` (`write_stage_overlay`, how an end-to-end brief test reads the signal).

Do not edit `src/commands/hook/worker_brief.rs`: `retrieve_pack` calls `StageQuery::dependency_paths(&config.work_dir, &config.stage)`, and keeping that signature is what gives every worker brief the change set with no edit there (the file is at its size limit).

## Task 1: move the retrieval tests out first

`retrieval.rs` is 397 lines; its inline `mod tests` (from `#[cfg(test)]` before `mod tests {` at about line 210 to the last `}`) is about 188 of them. Move the module body verbatim into `retrieval_tests.rs` (it starts with `use super::*;`, as the inline module did) and declare it at the end of `retrieval.rs`:

```rust
#[cfg(test)]
#[path = "retrieval_tests.rs"]
mod tests;
```

Confirm the move lost nothing by counting `#[test]` lines before and after (`rg -c '#\[test\]'` on the inline module, then on `retrieval_tests.rs`: 11 both times at `8034a71f`); the test run is your one check, at the end. The moved body loses one level of indentation; the orchestrator runs `cargo fmt --all` after you return. Test paths stay `orchestrator::signals::retrieval::tests::<name>`.

## Task 2: `dependency_changes` and the new `StageQuery::dependency_paths`

Today `StageQuery::dependency_paths` (retrieval.rs:99-123) returns each dependency's declared `files` and `artifacts`, through `find_stage_file` and `extract_stage_definition`. Graph node paths are repository-relative, so a glob or a `working_dir`-relative declared path never matches the exact comparison in `names_dependency_path`, and the boost has never fired in this repository.

Add `DependencyChange` and `dependency_changes` above `impl StageQuery`, with the signatures in common.md section 3. Rules, per id in `stage.dependencies`, in order:

1. `crate::verify::transitions::load_stage(id, work_dir)`; an error skips the dependency, as today.
2. Git path, only when all hold: `stage_type != StageType::Knowledge`; `status == StageStatus::Completed`; `completed_commit` is `Some`; the repository root resolves (`WorkDir::new(work_dir).ok().and_then(|r| r.main_project_root())`, the call `retrieve_stage_pack` makes); the target resolves (`crate::fs::resolve_target_branch_from_config(work_dir, &root)`). Then `accepted_change_base(&root, completed, &target)`:
   - whichever variant comes back, a `base` equal to `completed` (ASCII-case-insensitive) is rule 3: the commit already reached the target with no merge of its own (a dependency carried in by a downstream stage cut from its branch, whose own merge was then `AlreadyUpToDate`, or one fast-forwarded), so `git diff <completed> <completed>` would name no change and the boost would get nothing;
   - `Ok(AcceptedBase::Merged { base, .. })`: `paths` from `changed_paths(&root, &base, completed)`, `range = Some((base, completed.clone()))`;
   - `Ok(AcceptedBase::Unmerged { base })` with `!merge_assumed`: the same with that base;
   - `Ok(Unmerged)` with `merge_assumed`, any `Err`, or a `changed_paths` error: rule 3, with a `tracing::debug!` naming the error as the file already does.
3. Declared fallback, `range = None`: `files` then `artifacts`, skipping every entry containing `*`, `?` or `[`, each through `join_working_dir(dep.working_dir.as_deref(), entry)`, dropping `None`.

Every path then goes through `normalize_dependency_path` and duplicates are dropped, order kept.

Import with `use crate::git::merge::control_paths::{accepted_change_base, changed_paths, AcceptedBase};` and `use crate::context::rank_source::join_working_dir;` (import edges, which the plan's `reachable` check walks), never a fully qualified call.

`StageQuery::dependency_paths(work_dir, stage)` keeps its signature and becomes the flattened, order-preserving, deduplicated `paths` of `dependency_changes`. Rewrite its doc comment (it describes declared files today). Remove the imports nothing uses afterwards (`find_stage_file`, `extract_stage_definition`); clippy fails on them.

Cost: three read-only git calls per dependency (rev-list, merge-base, diff) each time a signal or brief is built. Keep each function under 50 lines (split the git branch into a helper if needed).

## Task 3: tests of the change set and of the stage brief

`retrieval_change_set_tests.rs`, declared at the end of `retrieval.rs`:

```rust
#[cfg(test)]
#[path = "retrieval_change_set_tests.rs"]
mod change_set_tests;
```

Share the repository setup, the merged-dependency fixture and the overlay writer through helper functions in this file, so its eight tests keep it under 400 lines. Each git test builds a `tempfile::TempDir` repository (`git init -b main`, local `user.email`/`user.name`, `.loom/` appended to `.git/info/exclude`), creates `<root>/.loom/work/stages`, and saves records with `crate::verify::transitions::save_stage`. Run git with the isolated config `git/merge/test_support.rs::isolated_git` uses (copy its three `env` lines; that helper is `pub(super)` to `git::merge`). Merge with `crate::git::merge::merge_stage("up", "main", root, lock.path(), MergeGate::Enforce)` and a separate TempDir as `lock`. Every `Stage` sets `id` explicitly (`Stage::new` derives a timestamped id). Each test under 50 lines:

- `a_merged_dependency_contributes_its_changed_files`: branch `loom/up` changes `src/up.rs`, then `merge_stage`; the record is Completed, `merged: true`, `completed_commit` the branch tip, `files: ["src/**"]`; `StageQuery::dependency_paths(&work_dir, &downstream)` is exactly `["src/up.rs"]`.
- `an_unmerged_dependency_used_as_base_contributes_its_changed_files`: the same without `merge_stage`; `["src/up.rs"]`.
- `an_assumed_merge_without_a_merge_commit_falls_back_to_declared_files`: no `merge_stage`, `merge_assumed: true`, `files: ["src/declared.rs"]`; `["src/declared.rs"]`.
- `a_dependency_without_a_commit_joins_declared_files_to_its_working_dir`: no git needed; `working_dir: Some("loom")`, `files: ["src/a.rs", "src/**/*.rs", "../web/b.ts"]`, `artifacts: ["src/a.rs"]`; `["loom/src/a.rs", "web/b.ts"]`.
- `a_knowledge_dependency_uses_declared_files`: `stage_type: Knowledge`, Completed, with a commit and a merge; declared fallback.
- `an_ancestor_without_a_merge_commit_falls_back_to_declared_files`: branch `loom/up` changes `src/up.rs`, then `git merge --ff-only loom/up` on main (no `merge_stage`); the record is Completed, `merged: true`, `merge_assumed: false`, `completed_commit` the branch tip, `files: ["src/declared.rs"]`; `["src/declared.rs"]`.
- `the_contract_signal_boosts_a_symbol_from_the_dependency_change`: the merged fixture and overlay of the next test; call `crate::orchestrator::signals::generate_contract_signal(&Session::new_contract("down"), &downstream, &worktree, &work_dir, None, &[])` (`contract.rs:66`; it retrieves through `retrieve_stage_pack` too, `contract.rs:78`) and read the returned path; its `### Source (signature index)` bullet for `src/up.rs` carries `UpstreamWidget` and `stage-dependency`.
- `the_stage_brief_boosts_a_symbol_from_the_dependency_change`: the merged fixture of the first test, plus a stage overlay for the downstream stage holding a node `src/up.rs#function:UpstreamWidget` written as `tests_brief_e2e.rs::write_stage_overlay` does (`GraphStore::save_overlay(plan_key, stage_id, &layer)`; copy the helper, it is private). The downstream stage description names `UpstreamWidget`. Call `crate::orchestrator::signals::generate_signal(...)` with `crate::orchestrator::signals::tests::{create_test_session, create_test_worktree}` and `&[]`, as `tests_brief_e2e.rs` does, and read the written signal file. Its `### Source (signature index)` section has a bullet naming `src/up.rs` that carries `UpstreamWidget` and the reason `stage-dependency` (`SelectionReason::StageDependency`). Under the old code the glob `src/**` matches nothing, so the reason is absent: this test fails before Task 2 and passes after it.

The moved test `dependency_file_and_artifact_paths_reach_the_query` must still pass unchanged (its dependency has no `working_dir`, no commit and is not Completed, so it takes the declared fallback). The case "a target commit merged into the stage branch is excluded" is pinned by C2's `tests_accepted.rs` and by the frozen contract `change_base_skips_target_merged_into_stage_branch` (`tests/dependency_inputs_contracts.rs`); do not duplicate it.

## Task 4: a worker brief carries the dependency's changed symbol

`src/commands/hook/tests_worker_brief_dependency.rs`, declared inside `src/commands/hook/tests_worker_brief.rs` beside its `#[path = "worker_brief/test_support.rs"] mod support;` line:

```rust
#[path = "tests_worker_brief_dependency.rs"]
mod dependency;
```

`tests_worker_brief.rs` is 271 lines at `8034a71f`, so the declaration fits; the test path is `commands::hook::worker_brief::tests::dependency::`, which the stage's acceptance selects. Fallback, only if the file has since grown past 397 lines: declare the module at the end of `worker_brief/test_support.rs` instead with `#[path = "../tests_worker_brief_dependency.rs"]`, and say so in your report (the orchestrator then corrects the acceptance filter by dispute).

The module is a descendant of `worker_brief`; start it with `use super::*;` and reach the private `issue`, `Config` and `retrieve_pack` through `super::super::` and the fixture through `support`. Name the test `a_worker_brief_boosts_the_dependency_change`. `let fixture = support::Fixture::new(false);` then build a git repository at `fixture._temp.path()` (plan 1 removed the `root` field; the work dir is `fixture.config.work_dir`) with an upstream stage merged as in Task 3 (changing `src/scoped_dep.rs`), save the upstream record (Completed, `completed_commit` the branch tip, `files: ["src/**"]`), and set `fixture.config.stage.dependencies = vec!["upstream".into()]`. Write a stage overlay (`GraphStore::save_overlay(PLAN, STAGE, &layer)`, as `test_support.rs::write_overlay` does; copy what you need, its helpers are private) holding a node `src/scoped_dep.rs#function:DependencyWidget`. Call `issue` the way the existing tests in `tests_worker_brief.rs` do, with `support::payload` whose prompt names `DependencyWidget`. The returned brief's bullet for `src/scoped_dep.rs` names `DependencyWidget` and carries `stage-dependency`. The worker query is `worker_query_text(stage, description, prompt)` after plan 1 (description, prompt, then stage text, cut at 2,048 bytes; `worker_brief.rs:120`), so the symbol in a short prompt with a short description survives the cap.

## Proof (from `loom/`; your one check, run once)

```bash
cargo test --lib orchestrator::signals::retrieval::
```

The subagent guard blocks `cargo build`, `clippy`, `fmt` and an unscoped `cargo test`; the orchestrator runs `cargo fmt --all` and the worker-brief test after you return. rustc warns that `DependencyChange.stage` and `.range` are never read: W2 reads them in wave 3; add no `#[allow]` or `#[expect]`. Every file at or under 400 lines and every function at or under 50 (`cargo test --test maintainability` runs in the orchestrator's gate). No git commands outside test TempDirs, no edits outside your list.

## Report

What changed per file, the test counts, and anything the tree did differently from this brief (with `file:line`). Record surprises and decisions with `loom memory note` / `loom memory decision` (`--evidence <path:line>`) as you go.
