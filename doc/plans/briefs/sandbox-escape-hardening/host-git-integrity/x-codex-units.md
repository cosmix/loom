# X: single-file pinning conversions (codex lane)

Stage `host-git-integrity`, wave 2 (after worker A returns), lane codex via
`loom-codex-forwarder`, `--model gpt-5.6-terra --effort xhigh`. One unit is one file. Waves:
X1-X6, then X7-X12, then X13-X16, each once the previous wave returns, with at most six
foreground forwards at once. The main agent forwards each unit as its own forwarder with the
prompt "Read
doc/plans/briefs/sandbox-escape-hardening/host-git-integrity/x-codex-units.md: the section
Shared rules, then unit X<n> only. Change only the file that unit names." The main agent runs
every unit's command only after ALL units AND workers B and C have returned.

## Shared rules

- Edit only your unit's file. Run no git, touch no `.loom` path, write no test that builds a
  repository unless your unit asks for one (X1, X5, X13). Do not run cargo: the main agent runs
  the unit's command after you return.
- Worker A added these to `loom::git` (read their doc comments in
  `loom/src/git/worktree/pinned.rs` and `loom/src/git/branch/{status,ancestry}.rs` first):
  - `WorktreeGit::pinned(repo_root: &Path, worktree: &Path) -> Result<WorktreeGit>`: use it
    when the function already holds the repository root.
  - `WorktreeGit::for_checkout(checkout: &Path) -> Result<WorktreeGit>`: a checkout ROOT
    (a stage worktree or the main checkout) and nothing else in hand.
  - `WorktreeGit::for_dir(dir: &Path) -> Result<WorktreeGit>`: a directory somewhere inside a
    checkout (an acceptance directory).
  - `WorktreeGit::run_read_only(&self, args: &[&str]) -> Result<Output>`,
    `read_checked(&self, args) -> Result<String>` (fails on non-zero exit, trimmed stdout),
    `work_tree(&self) -> &Path`.
  - `crate::git::branch::{has_uncommitted_changes_in, list_working_tree_changes_in,
    commits_ahead_of_in, is_ancestor_of_in}`, each taking `&WorktreeGit` first.
- Why: git that discovers its directory through a stage worktree's `.git` file (an
  agent-writable pointer) reads the agent's configuration, whose clean filter `git status`
  runs on the host. Pinned git never reads that file.
- Keep every existing error path's outcome (the "answers no", "no evidence", "refusal" the
  function already gives on a git failure). Keep line length under 100 columns.
- Ledgered items must keep their exact line counts (`loom/maintainability-baseline.txt`):
  the unit says when. Count lines before and after your edit.

## X1: `loom/src/orchestrator/monitor/parked.rs`

`finished_in_worktree(worktree, branch, base)`: `let Ok(git) =
WorktreeGit::for_checkout(worktree) else { return false; };`, then `commits_ahead_of_in(&git,
branch, base)` and `has_uncommitted_changes_in(&git)` in place of the path forms. Update the
module doc's link `[`has_uncommitted_changes`]` to the `_in` name. Existing tests must pass
unchanged (`a_failed_git_probe_answers_no` passes `/nonexistent/worktree`: `for_checkout` fails
on it, which answers no).

Add one behavioural test in the file's `tests` module, because every existing test there is
negative and a conversion that always errors would pass them. Build the repository and stage
worktree with `crate::verify::contracts::test_support::contract_worktree(<repo>, "s1")`
(`#[cfg(test)] pub(crate)`, reachable from lib tests), then commit in the worktree with a small
local `git` helper (`-c user.name=t -c user.email=t@t -c commit.gpgsign=false -c
core.hooksPath=/dev/null`; add and commit the contract file `contract_worktree` leaves
untracked), so the branch holds one commit beyond `main` and nothing is uncommitted. First `finished_in_worktree` answers `true`: that proves the converted call runs.
Then `crate::verify::contracts::test_support::plant_foreign_git_dir(&worktree)` (call it, do
not re-implement it) and call again: the marker file it returns stays absent. Positive control
last: plain `git status` in the worktree creates the marker.
Command: `cargo test --lib orchestrator::monitor::`

## X2: `loom/src/handoff/session_content.rs`

`build_session_content` and `modified_files` run git in `handoff.checkout` (the session's
worktree, or the repository root for a knowledge session). Resolve `let git =
WorktreeGit::for_checkout(handoff.checkout).ok();` once in `build_session_content`; the
branch is `git.read_checked(&["rev-parse", "--abbrev-ref", "HEAD"]).ok()`, replacing
`current_branch(handoff.checkout).ok()`; `modified_files` takes `Option<&WorktreeGit>` and runs
`run_read_only(&["status", "--short"])`, empty when `None` or on failure, as today. Drop the
now-unused imports.
Command: `cargo test --lib handoff::`

## X3: `loom/src/verify/before_after.rs`

`find_prior_stage_work(stage_branch, base_branch, repo_root, worktree_path)`: the working-tree
probe becomes `WorktreeGit::pinned(repo_root, worktree_path)` then
`list_working_tree_changes_in(&git)`. A pin failure takes the existing `Err(e)` arm (the same
`tracing::warn!` and `None`). Update the doc comment: the probe reads the worktree through its
registered git directory. `commits_ahead_of` there already runs in `repo_root`; leave it.
Command: `cargo test --lib verify::before_after`

## X4: `loom/src/verify/criteria/cache_fingerprint.rs`

`capture_with_budget`: replace `resolve_repo_root(acceptance_dir)` with `let git =
WorktreeGit::for_dir(acceptance_dir).ok()?; let repo_root =
git.work_tree().to_path_buf();`. `head`: `git.read_checked(&["rev-parse", "HEAD"]).ok()?`.
`git_bytes(args, &repo_root)` becomes `git_bytes(&git, args)` over `git.run_read_only(args)`,
same size cap and success check. Delete `resolve_repo_root` and the unused runner imports.
The rest of the function (the `starts_with` check, hashing) is unchanged.
Command: `cargo test --lib verify::criteria::`

## X5: `loom/src/orchestrator/merge_lifecycle/containment.rs`

`uncontained_worktree_head(stage_id, repo_root, target_branch)`: `is_ancestor_of("HEAD",
target_branch, &worktree)` becomes `WorktreeGit::pinned(repo_root, &worktree).and_then(|git|
is_ancestor_of_in(&git, "HEAD", target_branch))`, keeping the three match arms; a pin failure
lands in the existing "cannot verify the worktree HEAD" refusal.

Add an inline `#[cfg(test)] mod tests` to the file with one behavioural test, because a
conversion that always errors would otherwise pass: build a repository with its stage worktree
via `crate::verify::contracts::test_support::contract_worktree(<repo>, "s1")`
(`#[cfg(test)] pub(crate)`, reachable from lib tests), then
`crate::verify::contracts::test_support::plant_foreign_git_dir(&worktree)` (call it, do not
re-implement it). `uncontained_worktree_head("s1", <repo>, "main")` is `None` (the worktree HEAD
is main's commit, read through the registered git directory, not the planted one), and the
marker file `plant_foreign_git_dir` returned stays absent. Positive control last: plain `git
status` in the worktree creates the marker.
Command: `cargo test --lib orchestrator::merge_lifecycle::`

## X6: `loom/src/verify/wiring_detection.rs`

`collect_added_source_files` is ledgered at exactly 56 lines: keep 56. Replace the five-line
`run_git(...)` call and its `.with_context(...)?` with:

    // Pinned to the worktree's registered git directory: a process outside
    // the stage sandbox never follows the worktree's own `.git` file.
    let output = WorktreeGit::for_dir(worktree_path)?
        .run_read_only(&["diff", "--name-status", &format!("{}..HEAD", base_branch)])
        .with_context(|| format!("Failed to run git diff in {}", worktree_path.display()))?;

Swap the `use crate::git::runner::run_git;` line for `use crate::git::worktree::WorktreeGit;`.
Command: `cargo test --lib verify::wiring_detection:: && cargo test --test maintainability`
(`collect_added_source_files` is ledgered at 56)

## X7: `loom/src/verify/duplicate_detection.rs`

The file is ledgered at exactly 436 lines: keep 436. In `collect_changed_source_files`, replace
the four-line `run_git(...)?;` call with:

    // Pinned to the worktree's registered git directory: a process outside
    // the stage sandbox never follows the worktree's own `.git` file.
    let output = WorktreeGit::for_dir(worktree_path)?
        .run_read_only(&["diff", "--name-only", &format!("{}..HEAD", base_branch)])?;

and swap the `run_git` import for `WorktreeGit` (confirm with a search that `run_git` has no
other use in the file first).
Command: `cargo test --lib verify::duplicate_detection:: && cargo test --test maintainability`

## X8: `loom/src/git/cleanup/worktree.rs`

`blocking_paths(worktree_path)` and `tracked_scaffold_paths(worktree_path)` run
`run_git_checked` in the worktree. Use `WorktreeGit::for_checkout(worktree_path).and_then(|git|
git.read_checked(<same args>))`, keeping `.unwrap_or_default()` / the empty-set fallback
(unit tests call `remove_worktree_scaffold` on plain directories, which `for_checkout` treats as
discovered git that fails, as today). `cleanup_worktree`'s `git worktree remove` runs in
`repo_root`: leave it.
Command: `cargo test --lib git::cleanup::`

## X9: `loom/src/git/cleanup/removal.rs`

- `require_clean_worktree(resources)`: the `status --porcelain=v1 --untracked-files=all
  --ignored=matching` call runs through `WorktreeGit::for_checkout(&resources.worktree_path)?`
  and `read_checked`, keeping `.context("Failed to verify worktree cleanliness")`.
- `require_merged_history`: the `"HEAD"` check in the worktree becomes a pinned ancestry check.
  Give `require_ancestor` a `&WorktreeGit` parameter and call it with
  `WorktreeGit::discovered(repo_root)` for the three `repo_root` checks and
  `WorktreeGit::pinned(repo_root, &resources.worktree_path)?` for the worktree HEAD, over
  `is_ancestor_of_in`. Error messages unchanged.
Command: `cargo test --lib git::cleanup::`

## X10: `loom/src/context/worktree_graph.rs`

`build_for_worktree(working_dir)` discovers everything with git run in `working_dir` (a
directory inside a stage worktree under an operator's host `loom stage complete` or `loom
verify`): `rev-parse --show-toplevel`, `rev-parse --path-format=absolute --git-common-dir`,
`rev-parse HEAD`, and `changed_paths` (`diff-index`, `ls-files --others`). Convert them:

- `let git = WorktreeGit::for_dir(working_dir)?;` at the top; `worktree` is
  `git.work_tree().to_path_buf()`; the common directory is
  `git.read_checked(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?` (a pinned
  handle answers with its `GIT_COMMON_DIR`, which is `R/.git`); `HEAD` through
  `git.read_checked`.
- `changed_paths(worktree, base_revision)` takes `&WorktreeGit` in place of the path and runs
  both commands through `read_checked`; every caller is in this file.
- `from_scratch(worktree, working_dir, extractors)` runs `ls-files --full-name ...` in
  `working_dir`, which limits the listing to that subtree. A pinned handle runs in the work tree
  instead, so pass the subtree as a pathspec: append `--` and `working_dir`'s path relative to
  the work tree (nothing when they are equal). Build the handle there with `for_dir(working_dir)`.
  `work_tree()` is canonical, so take the relative path from `working_dir.canonicalize()`.
- Keep the doc comment on `build_for_worktree` true: it discovers the worktree and base through
  git pinned to the worktree's registered git directory.
- The file is not in the maintainability ledger; keep every function under 50 lines.
Command: `cargo test --lib context::worktree_graph`

## X11: `loom/src/orchestrator/core/provision_gate.rs`

The file is created by PLAN-stage-exits-and-environment and exists only once that plan has
merged; read it first. Its `git status --porcelain=v1 -z --untracked-files=all` before and after
provisioning runs `run_git_checked` in the stage worktree, by discovery. Convert both calls to
`WorktreeGit::for_checkout(worktree)?.read_checked(&[<same args>])`: the read-only run (no index
write-back), with the same success check and trimmed stdout, so the before/after comparison is
unchanged. A `for_checkout` error takes the path the function already gives a failing git call.
Drop the unused runner import.
Command: `cargo test --lib orchestrator::core::provision_gate:: && cargo build`

## X12: `loom/src/verify/criteria/cache_ignore.rs`

`check_ignore(candidates, acceptance_dir)` runs `git -C <acceptance_dir> check-ignore -q --stdin
-z` on the host during `loom stage complete` and `loom verify`, by discovery. Convert it to
`WorktreeGit::for_dir(acceptance_dir).ok()?` (an error is `None`, as a missing repository is
today). A `WorktreeGit` offers no stdin and, pinned, runs in the work tree rather than in
`acceptance_dir`, so pass the candidates as arguments:
`git.run_read_only(&["check-ignore", "-q", "--", <paths>])`, each path as
`acceptance_dir.canonicalize().ok()?.join(token)` (canonical, so it lies under the canonical
work tree git compares it with; the same resolution `-C <acceptance_dir>` gave a relative
token; a path that is not UTF-8 is `None`, which means do not cache).
Exit 0 is `Some(true)`, exit 1 `Some(false)`, anything else `None`, as today. The runner's
read deadline replaces `CHECK_IGNORE_TIMEOUT`; delete the spawn, stdin and wait code and the
imports and constant they leave unused. `path_like_tokens` and its tests are unchanged.
Command: `cargo test --lib verify::criteria:: && cargo build`

## X13: `loom/src/orchestrator/adjudication/prompt/sources.rs`

`run_git_show(work_dir, commit)` spawns `Command::new("git")` in the daemon, with the commit
after `--`: `["show", "--no-color", "--stat", "-p", "--", commit]`. Git then reads the SHA as a
pathspec and shows HEAD, so the briefing quotes the wrong commit. Two changes:

- Route the call through `crate::git::runner::run_git_checked(&["show", "--no-color", "--stat",
  "-p", commit, "--"], &project_root)`: the SHA before `--`, the project root as today. Keep
  the SHA-shape check, whose comment about `--` is rewritten to say the shape check is what
  keeps an option out of the commit slot. The result is `Ok(String)` of git's stdout; a failure
  stays an `Err`. Drop the `NO_HOOKS_ARGS` import; `Command` stays because `run_listing` runs
  `find`.
- Add a test in the file's `tests` module that the shown commit is the evidence commit, not
  HEAD: in a `TempDir` repository (`<repo>/.loom/work` as the work directory) make two commits
  with different file names and messages (`-c user.name=t -c user.email=t@t -c
  commit.gpgsign=false`), call `run_git_show` with the FIRST commit's SHA, and assert the output
  names the first commit's file and message and not the second's.
Command: `cargo test --lib orchestrator::adjudication::prompt:: && cargo build`

## X14: `loom/src/git/cleanup/batch.rs`

`prune_worktrees(repo_root)` and the branch probe in `needs_cleanup(stage_id, repo_root)` each
spawn `Command::new("git")` with `NO_HOOKS_ARGS` in `repo_root`. Route both through the runner:
`run_git_checked(&["worktree", "prune"], repo_root)?` (an `Err` as today; the message takes the
runner's format) and `run_git(&["rev-parse", "--verify", &format!("refs/heads/{branch_name}")],
repo_root)` kept inside the existing `matches!(.., Ok(o) if o.status.success())`. Both run in
`repo_root`, never in a stage worktree, so `for_checkout` does not apply. Drop the imports they
leave unused.
Command: `cargo test --lib git::cleanup:: && cargo build`

## X15: `loom/src/git/branch/cleanup.rs`

`cleanup_merged_branches(target_branch, repo_root)` spawns `Command::new("git")` for `branch
--merged <target>` in `repo_root`. Route it through `run_git(&["branch", "--merged",
target_branch], repo_root)` keeping `.with_context(|| "Failed to get merged branches")?` and
the existing handling of the exit status (it reads stdout whatever the status). Drop the
imports it leaves unused.
Command: `cargo test --lib git::branch:: && cargo build`

## X16: `loom/src/git/worktree/checks.rs`

`check_git_available` (`git --version`) and `check_worktree_support` (`git worktree list`) spawn
`Command::new("git")` with no directory, so git runs in the process's cwd. Route both through
`run_git(&[..], Path::new("."))`, which keeps that cwd and adds the runner's `commondir` guard;
keep each function's `Ok`/`bail!` outcomes and messages (a spawn failure keeps "Git is not
installed or not in PATH"). Neither can be a stage worktree, so `for_checkout` does not apply.
Drop the imports they leave unused.
Command: `cargo test --lib git::worktree:: && cargo build`
