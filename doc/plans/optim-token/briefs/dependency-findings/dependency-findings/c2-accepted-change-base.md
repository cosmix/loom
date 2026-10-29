# C2 (codex unit accepted-change-base): where a merged stage change starts

Lane: codex, gpt-5.6-terra, effort xhigh. Paths are relative to `loom/`.

Write only these two files:

- `src/git/merge/control_paths.rs`
- `src/git/merge/control_paths/tests_accepted.rs` (new)

Read for reference only: `src/git/merge/control_paths/tests.rs` (its imports and how it calls `merge_stage`), `src/git/merge/test_support.rs` (`git_ok`, `git_out`, `init_repo`, `lock_dir`, `commit_file`, `stage_branch`, `rev`), `src/git/merge/mod.rs` (`merge_stage`), `src/git/merge/tree.rs` (`commit_merge`). Start from `changed_paths` and `control_path_violation_as` in `control_paths.rs`.

Run no git outside a test TempDir, write nothing under `.loom/`, and run no build, test or formatter: the orchestrator compiles and runs the tests right after you.

## Why

loom merges every non-knowledge stage into its target through a two-parent merge commit M: `commit_merge` (`tree.rs`) takes `parents: [&str; 2]`, the target tip first and the stage tip second, and the stage tip at that merge is the stage record `completed_commit`. In this repository `e133f68c` has parents `5f489d2c` (target) and `d4cd3148` (the guard-core stage commit). The change a stage brought is `git diff <base> <completed>` with `base = merge-base(M^1, completed)`: the last target commit the stage branch contains. Target commits merged into the stage branch while resolving a conflict then drop out of the diff, and target commits that reached the target after the fork but before M do not appear as reverse changes.

## Steps

1. Add to `control_paths.rs`, beside `changed_paths` (exact signatures; a later worker calls them):

   ```rust
   /// Where the change a stage merge brought to `target` starts.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub(crate) enum AcceptedBase {
       /// The first-parent history of `target` holds `merge`, whose second
       /// parent is the stage commit; `base` is `merge-base <merge>^1 <completed>`.
       Merged { merge: String, base: String },
       /// No such merge: `base` is `merge-base <completed> <target>`. That is
       /// the fork point when the stage has not merged yet (a dependency used
       /// as a base branch), and `completed` itself when the commit already
       /// reached `target` another way; the caller treats the second as unknown.
       Unmerged { base: String },
   }

   pub(crate) fn accepted_change_base(repo_root: &Path, completed: &str, target: &str) -> Result<AcceptedBase>
   ```

   Body, in order:
   - `bail!` unless `completed` is 40 or 64 ASCII hex digits (a private `fn is_commit_id(value: &str) -> bool`). This also keeps a value starting with `-` away from the git option parser.
   - `let target_ref = branch_ref(target);` (already imported), so a tag named like the branch cannot stand in, as `control_path_violation` does.
   - `crate::git::run_git_checked(&["rev-list", "--first-parent", "--merges", "--parents", "--ancestry-path", &format!("{completed}..{target_ref}")], repo_root)?`. Each output line is `<commit> <parent1> <parent2> ...`, newest first; later stage merges on the target appear before the dependency's own. Take the first line whose SECOND parent (the third field, `split_whitespace().nth(2)`) equals `completed`, compared ASCII-lowercased: `merge_stage` writes `[old target tip, branch tip]` (`git/merge/tree.rs::commit_merge`), and a match on the first parent would pick a later merge on top of a fast-forwarded commit, whose base is `completed` itself. On this repository `git rev-list --first-parent --merges --parents --ancestry-path d4cd3148..e133f68c` prints `e133f68c... 5f489d2c... d4cd3148...`.
   - Found: `base = run_git_checked(&["merge-base", &format!("{merge}^1"), completed], repo_root)?`; return `Merged { merge, base }`.
   - Not found: `base = run_git_checked(&["merge-base", completed, &target_ref], repo_root)?`; return `Unmerged { base }`.

   `run_git_checked` returns trimmed stdout. Each new function stays under 50 lines.

2. Declare the tests at the end of `control_paths.rs`, after the existing `mod tests;`:

   ```rust
   #[cfg(test)]
   #[path = "control_paths/tests_accepted.rs"]
   mod tests_accepted;
   ```

3. Write `src/git/merge/control_paths/tests_accepted.rs`. Import the helpers from `super::super::test_support` and `merge_stage` from `super::super` exactly as `control_paths/tests.rs` does, plus `use super::*;` for `MergeGate`, `accepted_change_base`, `AcceptedBase` and `changed_paths`. Use `#[serial]` only if `control_paths/tests.rs` does. Each test under 50 lines:

   - `a_merged_stage_starts_at_its_fork_point`: `init_repo()`, `stage_branch(root, "s", &[("s1.txt", "a"), ("s2.txt", "b")])`, `completed = rev(root, "loom/s")`, fork = `rev(root, "main")`, then `merge_stage("s", "main", root, lock.path(), MergeGate::Enforce)` with `lock = lock_dir()`. `accepted_change_base(root, &completed, "main")` is `Merged`, its `merge` equals `rev(root, "main")`, its `base` equals the fork, and `changed_paths(root, &base, &completed)` equals `["s1.txt", "s2.txt"]` (sorted before comparing).
   - `a_target_merged_into_the_stage_branch_is_not_its_change`: `init_repo()`; branch `loom/s` from main with `s1.txt`; back on main commit `other.txt` (commit X = `rev(root, "main")`); check out `loom/s`, run `git merge --no-edit main`, check out main; commit `third.txt` on main; `completed = rev(root, "loom/s")`; `merge_stage(...)`. The result is `Merged` with `base == X`, and `changed_paths(root, &base, &completed)` is exactly `["s1.txt"]` (neither `other.txt` nor `third.txt`).
   - `an_unmerged_stage_uses_the_merge_base_with_the_target`: `stage_branch` as in the first test, then a commit on main, and no `merge_stage`. The result is `Unmerged { base }` with `base == git_out(root, &["merge-base", &completed, "main"])`.
   - `a_value_that_is_not_a_commit_id_is_refused`: `accepted_change_base(root, "--all", "main")` and `accepted_change_base(root, "abc", "main")` are both `Err`.
   - `a_later_stage_merge_is_skipped`: as the first test (`s` with `s1.txt`, fork = `rev(root, "main")`, `merge_stage("s", ...)`), then `stage_branch(root, "t", &[("t1.txt", "c")])` and `merge_stage("t", ...)`, so main's newest merge is `t`'s. The result is `Merged` whose `merge` is `s`'s merge (`git_out(root, &["rev-parse", "main^1"])`: main is `t`'s merge, and its first parent is `s`'s), with `base == fork`, and `changed_paths(root, &base, &completed)` is exactly `["s1.txt"]`.
   - `a_fast_forwarded_commit_has_no_merge`: `stage_branch(root, "s", &[("s1.txt", "a")])`, `completed = rev(root, "loom/s")`, then `git_out(root, &["merge", "--ff-only", "loom/s"])` with main checked out (no `merge_stage`). The result is `Unmerged { base }` with `base == completed`.

## Done means

`control_paths.rs` holds `AcceptedBase`, `accepted_change_base`, `is_commit_id` and the test declaration; the new test file holds the six tests. The orchestrator proves it with `cargo test --lib git::merge::control_paths::tests_accepted::`.

Constraints: `control_paths.rs` (185 lines at the start) stays under 400 lines and every function under 50 after formatting, with a 10 percent margin. Literal braces inside `format!` are doubled. A rustdoc comment that names a placeholder puts it in backticks. Report what you changed in one paragraph.
