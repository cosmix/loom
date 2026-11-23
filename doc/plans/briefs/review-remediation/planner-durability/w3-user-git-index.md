# planner-durability / w3-user-git-index: a refused handoff commit leaves the index as it was

Tier: codex sol
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/planner-backend.md`, sections P5 and CR-76 (sub-claims 76b and 76c only).

You run no `git` command, touch no path under `.loom/`, and run no tests. Your proof is `cargo check --all-targets` from `loom/`. The orchestrator runs the tests you write. Look bodies up yourself with `loom map --outline <file>`, `loom map --find-all <symbol>` and `rg`.

## Findings covered
- P5: `run_sequence` runs `git add` on every ticked path before `git commit -- <paths>`; a refused commit leaves a partially staged tracked file fully staged and a new file staged.
- CR-76(b): `committed_bytes` turns any failing `git cat-file` into `Ok(None)` ("no such file"), so a git error reads as a hook change.
- CR-76(c): `IndexBefore::reset` runs `git reset -q --` with no pathspec when its target list is empty, which resets the whole index.

## Files
Own (write), paths under `loom/`:
- `src/git/user_git.rs`, `src/git/user_git_tests.rs`
- `src/plan_server/run/commit.rs`, `src/plan_server/run/commit/tests.rs`
- `src/plan_server/trace/settle_commit.rs`, `src/plan_server/trace/settle_head.rs`, `src/plan_server/trace/settle_commit_tests.rs`

Read only: `src/git/runner.rs`, `src/plan_server/trace/settle_job.rs` (`commit_holds_settled`, the caller of `committed_bytes`), `src/plan_server/trace/settle_head_tests.rs`, `tests/test_support/git_repo.rs` (`TestRepo`, `git_checked`).

Frozen (contract test, never edit): `src/plan_server/run/commit/index_contract_tests.rs`.

## Changes

### 1. Promote the two settle helpers into `git/user_git.rs`
- `untracked`: today private in `plan_server/trace/settle_commit.rs` (symbol `untracked`, near line 242). Move it to `git/user_git.rs` as `pub(crate) fn untracked(root: &Path, paths: &[&str]) -> Result<Vec<String>>`: `run_literal(root, &["ls-files", "-z", "--others", "--exclude-standard", "--"], paths)`, then the existing `names("git ls-files", ..)` helper of `user_git.rs`, its `String` error mapped with `anyhow::Error::msg`. Paths come back relative to `root`, as before. Delete the settle copy and import the new one in `settle_commit.rs` (`try_record` is its caller). `listed` stays in `settle_commit.rs` (`differs_from_head` uses it).
- `unstage`: today `pub(super)` in `plan_server/trace/settle_head.rs` (symbol `unstage`, near line 118). Add to `git/user_git.rs` `pub(crate) fn unstage(root: &Path, added: &[String]) -> Result<(), String>`: nothing to do for an empty list; otherwise `checked("git rm --cached", run_literal(root, &["rm", "--cached", "-q", "-f", "--ignore-unmatch", "--"], added))`. Keep the `-f` comment. `settle_head::unstage(root, added, reason) -> String` keeps its signature and callers and becomes: `match user_git::unstage(root, added) { Ok(()) => reason, Err(why) => format!("{reason}; {why}") }`. The failure wording becomes git's `checked` form ("git rm --cached failed: ..."); no test asserts the old one.

### 2. P5: `plan_server/run/commit.rs` `run_sequence`
- Delete `addable_paths`. Move its doc point (a staged deletion or a rename source is in neither the index nor the work tree; it stays on the commit pathspec only) into a comment on the new step.
- After `IndexBefore::read`: `let added = untracked(root, &path_strs)?;` (the `&str` view of `paths`). Run `git add -- <literal(added)>` through `user_git` exactly as today, only when `added` is non-empty. Tracked paths are no longer added: `git commit -- <paths>` takes their work-tree content itself (`--only` semantics) and, when a hook refuses, git rolls the real index back.
- When `git add` exits non-zero, or after `git commit` when HEAD did not move: call `user_git::unstage(root, &added)`; a failure is streamed as one `on_line("git rm --cached", OutputStream::Stderr, &why)` line. Then return the code as today.
- HEAD moved: `index.reset(root, paths, "HEAD")` as today; it already resets `added` (they are among `paths`).

### 3. CR-76(c): `IndexBefore::reset` in `git/user_git.rs`
When `targets` is empty, skip the `git reset` call only. Still compute `kept` with `differing(..)` and report failures as today: a hook may have committed paths the user had staged even when `ours` and `others` are empty.

### 4. CR-76(b): `committed_bytes` in `plan_server/trace/settle_commit.rs`
Keep the signature `pub(super) fn committed_bytes(root: &Path, sha: &str, path: &Path) -> Result<Option<Vec<u8>>>`. First `run_git(&["ls-tree", "-z", sha, "--", &format!("./{path}")], root)`: a non-zero exit is `Err` (with `refusal("git ls-tree", &output)`); empty output is `Ok(None)`; an entry whose type field is not `blob` (a tree or a submodule) is `Ok(None)`. Then `cat-file blob <sha>:./<path>`: a non-zero exit is `Err(refusal("git cat-file", ..))`, success is `Ok(Some(stdout))`. The doc comment already promises `Err` when git cannot answer; `commit_holds_settled` maps `Err` to "holds" and needs no change.

## Interfaces shared with other workers
None. No other worker edits these files.

## Tests (write them; the orchestrator runs them)
- `plan_server/run/commit/tests.rs`, fixture `Repo` (`Repo::new`, `put`, `hook`, `git_text`, `commit`), `#[serial_test::serial]`:
  - `a_refused_commit_leaves_partial_staging_and_new_files_as_they_were`: stage `doc/a.md` as `staged\n`, then write `worktree\n` to it; add untracked `doc/new.md`; stage a change to `src.txt`; a `pre-commit` hook that exits 1; record `git ls-files --stage` and `git diff --cached` before; `repo.commit(&["doc/a.md", "doc/new.md"], "edit")` returns `Ok(1)`; both outputs are equal after. Fails today (`doc/a.md` fully staged, `doc/new.md` staged).
  - `a_new_file_is_committed_and_unstaged_paths_stay_unstaged`: no hook; commit `doc/new.md` plus a modified unstaged `doc/a.md`; both are in HEAD and `git diff --cached --name-only` is empty.
- `git/user_git_tests.rs`, helper `repo()`: `reset_with_no_paths_keeps_the_staged_index` (stage a file, `IndexBefore::read`, `reset(root, &[] as &[&str], "HEAD")`, the file is still staged; fails today) and `untracked_lists_only_unknown_paths`.
- `plan_server/trace/settle_commit_tests.rs`, helper `repo()`: `committed_bytes_of_an_unknown_commit_is_an_error` (`committed_bytes(root, &"0".repeat(40), Path::new(PLAN))` is `Err`; fails today with `Ok(None)`) and `committed_bytes_of_an_absent_path_is_none`. Use `super::settle_commit::committed_bytes`.
- Existing settle tests that must stay green unchanged: `settle_commit_tests.rs` `a_refusing_hook_reports_its_words_and_unstages_the_new_brief`, `settle_head_tests.rs` `a_failure_before_any_commit_unstages_the_new_brief` and `a_refusal_after_head_moved_elsewhere_unstages_the_new_brief`.
- Contract to turn green: `refused-commit-leaves-staging`.

## Proof
`cargo check --all-targets` from `loom/`. The orchestrator then runs `cargo test --lib git::user_git::`, `cargo test --lib plan_server::run::commit::` and `cargo test --lib plan_server::trace::`.

## Traps
- An empty `added` list must never reach `git rm --cached --` or `git add --` without a path: both helpers return early.
- Do not keep `git add` for tracked paths "to be safe": that is the defect.
- `ls-tree` takes the path relative to the current directory; keep the `./` prefix so a `root` below the work-tree top resolves as `cat-file` does.
- Comparing raw `.git/index` bytes is not the target: `git add` then `git rm --cached` can drop the cached tree extension. The tests compare `ls-files --stage` and `diff --cached`.
