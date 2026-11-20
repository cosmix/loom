# A: pinned host git, the commondir guard, pinned branch helpers, in-progress merges

Stage `host-git-integrity`, wave 1, tier opus. Read `../common.md` first (D2 and D7).

## Why

Loom's daemon, its monitor and an operator's `loom` CLI run git against a stage worktree `T`
outside every sandbox. Most of those calls let git discover its directory through `T/.git`, a
file the agent can rewrite. A rewritten pointer names a directory whose `config` defines a
`filter.<name>.clean` driver, and the host `git status`/`git diff` that re-hashes a stat-dirty
file runs it (`verify/contracts/test_support.rs::plant_foreign_git_dir` builds exactly that
fixture). Since `2908339a` the runner prepends `-c core.fsmonitor=false`
(`git/runner.rs::NO_HOOKS_ARGS`), so fsmonitor is closed; filters, `core.worktree` and forged
refs are not.

A second redirect was confirmed this session on git 2.53: a `commondir` file planted in the
MAIN repository's git directory `R/.git/commondir` makes every discovery-based git call in `R`
take its config, objects and refs from the named directory. With a foreign config defining a
clean filter and a same-length rewrite of a tracked file, `git -c core.hooksPath=/dev/null -c
core.fsmonitor=false status` in `R` ran the filter. `GIT_DIR=R/.git` alone does not stop it;
`GIT_COMMON_DIR=R/.git` does. Checkout-rooted sessions (knowledge, merge resolution) can write
`R/.git`, so they can plant it.

You build the pinned interface every other worker converts call sites to, and close the
`commondir` redirect for loom's own git.

## Files you own (relative to `loom/`)

`src/git/worktree/pinned.rs`, `src/git/worktree/pinned_tests.rs`, `src/git/runner.rs`,
`src/git/runner/tests.rs` (new), `src/git/runner/pinned.rs`, `src/git/runner/pinned/tests.rs`,
`src/git/branch/status.rs`, `src/git/branch/status_tests.rs`, `src/git/branch/ancestry.rs`,
`src/git/branch/mod.rs`, `src/git/merge/in_progress.rs`, `src/git/merge/in_progress_tests.rs`
(new).

Not yours: `src/git/mod.rs`, `src/git/merge/mod.rs` (worker B), `src/git/cleanup/**` (codex
units X8, X9, X14), `src/git/branch/cleanup.rs` (X15), `src/git/worktree/checks.rs` (X16). The
codex units in wave 2 call exactly the signatures below; do not rename them.

## 0. Make room first (file sizes)

`src/git/runner.rs` is 351 lines and `src/git/merge/in_progress.rs` 396, both with inline tests;
the additions below push both over 400, which fails `cargo test --test maintainability`.
Before adding anything, move each file's inline `mod tests { ... }` body, unchanged, to its own
file: `runner.rs` to `src/git/runner/tests.rs`, declared `#[cfg(test)] #[path =
"runner/tests.rs"] mod tests;`; `in_progress.rs` to `src/git/merge/in_progress_tests.rs`,
declared `#[cfg(test)] #[path = "in_progress_tests.rs"] mod tests;` (a `#[path]` is relative to
the declaring file's directory). The test paths `git::runner::tests::*` and
`git::merge::in_progress::tests::*` stay the same; a moved assertion line is not a test-integrity
event. Every test this brief adds to those two files goes in the new files.

## 1. `WorktreeGit` additions (`src/git/worktree/pinned.rs`)

Anchors (advisory, read at `3fc28031`, unchanged at `2908339a`): `WorktreeGit` :36,
`pinned` :66, `pinned_in_project_of` :83, `run` :97, `registered_admin_dir` :122.

```rust
/// Global options for a read that never writes the index back.
pub const READ_ONLY_ARGS: [&str; 1] = ["--no-optional-locks"];

impl WorktreeGit {
    /// Git for `checkout` as a process outside every stage sandbox runs it:
    /// pinned (`Self::pinned`) when `checkout`, canonicalized, is the
    /// outermost stage location `<repo>/.worktrees/<name>` among itself and
    /// its ancestors (`<repo>/.git` a directory, or the gitfile of a
    /// `--separate-git-dir` checkout); an error when an ancestor is the
    /// outermost stage location (the checkout lies inside a stage worktree);
    /// as discovered for any other directory, the main checkout included. A
    /// stage location that cannot be pinned is an error, never discovered git.
    pub fn for_checkout(checkout: &Path) -> Result<Self>;

    /// `Self::for_checkout` for the checkout that holds `dir`. `dir` is made
    /// absolute without resolving symlinks, and its lexical ancestors (itself
    /// included) are matched against the stage locations; the OUTERMOST match
    /// is pinned, its work tree that worktree, but only when
    /// `dir.canonicalize()` lies under the canonical matched worktree, else
    /// `Err`. With no match, git is discovered at `dir` and `work_tree()` is
    /// the top level `rev-parse --show-toplevel` reports at `dir`, not `dir`.
    /// A `.git` an agent planted below a stage worktree is never followed.
    pub fn for_dir(dir: &Path) -> Result<Self>;

    /// `git <args>` behind `READ_ONLY_ARGS`, through `Self::run`.
    pub fn run_read_only(&self, args: &[&str]) -> Result<Output>;

    /// `Self::run_read_only`, failing on a non-zero exit with the runner's
    /// error format; the trimmed stdout otherwise.
    pub fn read_checked(&self, args: &[&str]) -> Result<String>;

    /// The administrative directory a pinned handle runs with; `None` for a
    /// discovered one.
    pub fn git_dir(&self) -> Option<&Path>;
}
```

- One private helper decides stage shape: `fn stage_worktree_repo(path: &Path) -> Option<&Path>`
  returns `path.parent().parent()` when `path.parent()`'s file name is `.worktrees` and
  `symlink_metadata(<repo>/.git)` succeeds, whatever its type: a directory, or the gitfile of a
  `--separate-git-dir` main checkout (`git init --separate-git-dir`), whose worktree registry
  lives in the store the gitfile names. Do NOT require a directory: in that layout a stage
  worktree would be classified as no stage and get discovered git through its agent-writable
  `T/.git`. `Self::pinned` settles the layout (it asks git in `<repo>` for the common directory
  and requires a registration there) and fails closed.
- The OUTERMOST stage-shaped path decides, never the innermost: an inner match
  (`T/.worktrees/x`) has a `<repo>` inside the stage worktree `T`, whose `T/.git` the agent
  writes, so pinning to it would ask agent-controlled git for the common directory.
  `for_checkout` checks the canonical checkout and each of its ancestors: the checkout itself
  outermost means `Self::pinned(repo, checkout)`; a proper ancestor outermost means `bail!`
  ("<checkout> lies inside the stage worktree <ancestor>"); none means discovered. `for_dir`
  checks each lexical ancestor of the absolutized `dir` (itself included) and pins to the
  outermost match.
- `for_dir` never lets a symlink decide the worktree: the lexical match picks the candidate,
  then `dir.canonicalize()?` must `starts_with` the canonical work tree `Self::pinned` returns,
  else `bail!` naming both paths. Without it, an agent-made symlink `T/loom -> /tmp/x` would
  move git discovery into the agent's directory (`for_dir(T/loom)` lexically sits inside `T`,
  but its real location does not).
- A discovered handle from `for_dir` takes its `work_tree()` from `git rev-parse
  --show-toplevel` run at `dir` through the runner (so the `commondir` guard covers it), then
  `Self::discovered(<that path>)`; a failing `rev-parse` is `Err`. X4 and X10 replace their
  own `--show-toplevel` calls with `work_tree()`, and porcelain paths are relative to the top
  level, so `dir` itself would be wrong for a subdirectory.
- `read_checked` reuses the runner's success check: make `git/runner.rs::stdout_of_success`
  `pub(crate)` and call it; do not copy its message format.
- Update the module doc: name `for_checkout`/`for_dir` as the entry points for host code that
  has only a path, and `pinned`/`pinned_in_project_of` for code that knows the repository.

Known imprecision, document it on `for_dir` in one sentence: a repository that itself lives
inside an outer stage worktree (`/x/.worktrees/o/repo`) and is reached through `for_dir` on a
subdirectory is pinned to the outer worktree, because the outermost stage location decides;
`for_checkout(repo)` on it is an error. Callers that hold the checkout root use
`for_checkout`.

## 2. The `commondir` guard (`src/git/runner.rs`, `src/git/runner/pinned.rs`)

At `2908339a` the runner has `run_git_with_env` (discovery unless the caller's env pins) and
`runner/pinned.rs::run_git_pinned_within` (drops `GIT_DIR`, `GIT_WORK_TREE`,
`GIT_INDEX_FILE`, `GIT_COMMON_DIR` from the environment, then discovers). Note:
`run_git_pinned` is NOT D2 pinning; it still discovers through `.git`. Never use it for a stage
worktree.

Add `pub(crate) fn run_git_with_env_within(args: &[&str], env: &[(&str, &OsStr)], repo_root:
&Path, timeout: Duration) -> Result<Output>` in `runner.rs`: it is today's `run_git_with_env`
body with the deadline as a parameter, and `run_git_with_env` becomes `run_git_with_env_within`
with `git_timeout(args)`. Worker B's checkout-integrity checks (`fsck`, `verify-pack`,
`commit-graph verify`) call it with 300 s; keep this exact signature.

Add one private function in `runner.rs`, called by `run_git_with_env_within` before it spawns
(so both `run_git_with_env` and B's calls pass it) and by `run_git_pinned_within` before it
spawns:

```rust
/// Refuses discovery-based git whose nearest `.git` at or above `repo_root`
/// is a directory holding `commondir`: git would take another repository's
/// configuration, objects and refs from the path it names. A main
/// repository's git directory never holds one (only `worktrees/<name>/`
/// does), and a session that can write the main git directory can plant it.
/// Skipped when `env` sets `GIT_DIR` or `GIT_COMMON_DIR`: the caller pinned.
fn refuse_redirected_git_dir(repo_root: &Path, env: &[(&str, &OsStr)]) -> Result<()>
```

Walk `repo_root.canonicalize()` ancestors (a path that does not resolve: return `Ok`, git will
fail on its own); at the first ancestor with a `.git` entry (`symlink_metadata`): a file means
`Ok`; a directory holding a `commondir` entry means `bail!` with a message containing
`commondir` and the path; otherwise `Ok`. Stats only, no git.

## 2b. Replace objects and submodules

- Every git command the runner starts carries `GIT_NO_REPLACE_OBJECTS=1`: set it in
  `runner.rs::git_command`, which both `run_git_with_env` and `runner/pinned.rs::pinned_command`
  build on, so it holds for discovered, env-pinned and `WorktreeGit` runs alike. A checkout
  session can write `R/.git/refs/replace/*` mid-session, and the daemon's own merges and
  ancestry checks in `R` would otherwise read the replacement object instead of the real one.
  Document it beside `NO_HOOKS_ARGS`. Before relying on it, run `rg -n 'replace' src --type
  rust` and confirm no loom code path depends on replace refs being honoured; report any.
- Submodules, for EVERY runner call and every direct site that splices `NO_HOOKS_ARGS`
  (`rg -n NO_HOOKS_ARGS src --type rust` lists them; discovered calls in the main checkout `R`
  included): add `-c diff.ignoreSubmodules=all -c submodule.recurse=false -c
  status.submoduleSummary=false` to `git/runner.rs::NO_HOOKS_ARGS` itself, which grows from
  four to ten elements (`[&str; 10]`), and extend its doc comment, which names only the hooks
  and fsmonitor keys, with these three. `WorktreeGit::run` adds nothing of its own, and a
  discovered handle (git in `R`) gets them like a pinned one. A tracked gitlink otherwise makes
  host `git status` or `git diff` recurse into `<path>/.git`, a directory the agent wrote, and
  run its configuration. That reaches `R` too: a checkout session can commit a gitlink and
  write `R/sub/.git` with a clean filter, and `git/merge/probe.rs::require_clean_repository`
  runs `status --porcelain=v1 --untracked-files=no` in `R` before every merge (measured on git
  2.53: it ran that filter without the three flags and did not with them). The only existing
  submodule use is `--ignore-submodules=all` in `fs/knowledge/catalog/evidence.rs`, which the
  flags leave unaffected.

## 3. Pinned branch helpers (`src/git/branch/status.rs`, `ancestry.rs`, `mod.rs`)

Add, and re-export from `branch/mod.rs`:

```rust
pub fn has_uncommitted_changes_in(git: &WorktreeGit) -> Result<bool>;
pub fn list_working_tree_changes_in(git: &WorktreeGit) -> Result<Vec<String>>;
pub fn commits_ahead_of_in(git: &WorktreeGit, branch: &str, base: &str) -> Result<usize>;
pub fn is_ancestor_of_in(git: &WorktreeGit, commit_sha: &str, branch: &str) -> Result<bool>;
```

Move each body into the `_in` function (status reads through `run_read_only`;
`working_tree_changes` takes `git.work_tree()` for `is_tool_artifact`; `branch_exists_ref`
takes the handle). The existing `&Path` functions become one-line wrappers over
`WorktreeGit::discovered(repo_root)`, so every existing caller and test keeps its behaviour.
Pattern to mirror: `verify/review/fingerprint.rs::compute_local`, which takes a `WorktreeGit`.
Do not copy its fingerprint logic.

## 4. In-progress merges (`src/git/merge/in_progress.rs`)

- `git_dir_for_repo_path(repo_path)`: call `WorktreeGit::for_checkout(repo_path)?` FIRST, for
  every path and whatever the type of its `.git` entry, since an agent can replace a stage
  worktree's `T/.git` file with a directory holding a planted `MERGE_HEAD`. When `git_dir()` is
  `Some` (a stage-shaped path), return it: the registered admin directory, whatever `T/.git`
  holds. When `None` (the main checkout, or a checkout loom did not create), read `.git` as
  today: a directory is returned, a file is parsed. A stage-shaped path that cannot be pinned
  is an `Err`, which `merge_head_exists` and `detect_at` already turn into "no merge".
- `unmerged_paths(repo_path)`: `WorktreeGit::for_checkout(repo_path)?.read_checked(&["diff",
  "--name-only", "--diff-filter=U"])`.
- Signatures of every `pub` function stay as they are (`commands/stage/complete.rs` and
  `commands/stage/merge/preflight.rs` call them and are not yours).

## Tests to add

In `pinned_tests.rs` (reuse `contract_worktree` and `plant_foreign_git_dir` from
`verify/contracts/test_support.rs`; positive control last, as the existing tests do):

- `a_stage_checkout_is_pinned_and_the_main_checkout_is_discovered`: `for_checkout(worktree)`
  has `git_dir() == Some(registered)`; `for_checkout(repo)` has `None`.
- `a_directory_inside_a_stage_worktree_is_pinned_past_a_nested_git_dir`: create
  `<worktree>/sub/.git` as a directory holding an `init`ed repository whose config defines the
  marker clean filter and `<worktree>/sub/.gitattributes` `* filter=evil` plus a stat-dirty
  tracked file of that nested repo; `for_dir(<worktree>/sub)` has the registered `git_dir()` and
  `run_read_only(&["status", "--porcelain"])` leaves the marker absent.
- `a_read_only_read_follows_no_repointed_git_file`: `plant_foreign_git_dir`, then
  `for_checkout(worktree)?.run_read_only(status)`; marker absent; then discovered status
  creates it.
- `a_stage_shaped_directory_the_repository_does_not_register_is_refused`:
  `<repo>/.worktrees/stranger` as a plain directory; `for_checkout` is `Err` containing
  `is not a registered worktree`.
- `a_symlinked_directory_inside_a_stage_worktree_is_refused`: `<worktree>/loom` is a symlink to
  a directory outside the worktree that holds an `init`ed repository with the marker clean
  filter, `.gitattributes` applying it and a stat-dirty tracked file; `for_dir(<worktree>/loom)`
  is `Err` and the marker is absent. Positive control last: plain `git status` in
  `<worktree>/loom` creates it.
- `a_stage_worktree_of_a_separate_git_dir_checkout_is_pinned`: `git init --separate-git-dir
  <t>/store.git <t>/repo`, a commit, `git worktree add <t>/repo/.worktrees/s1 -b loom/s1`;
  `plant_foreign_git_dir` on s1; `for_checkout(s1)` has `git_dir()` under the canonical
  `<t>/store.git/worktrees/` and `run_read_only(status)` leaves the marker absent;
  `for_dir(<s1>/sub)` likewise; positive control last (plain `git status` in s1 creates it).
- `a_checkout_inside_a_stage_worktree_is_refused`: in a stage worktree `T` of a normal
  repository, `T/.worktrees/x` is a directory and `T/.git` stays the gitfile git wrote;
  `for_checkout(T/.worktrees/x)` is `Err` containing `lies inside the stage worktree`, and
  `for_dir(T/.worktrees/x/sub)` has `T`'s registered `git_dir()`.
- `for_dir_in_a_plain_subdirectory_reports_the_top_level`: in a plain repository (no stage
  worktree), `for_dir(<repo>/sub)` has `git_dir() == None` and `work_tree()` equal to the
  canonical repository root, not `<repo>/sub`; in a stage worktree, `for_dir(<worktree>/sub)`
  has the registered `git_dir()` and `work_tree()` equal to the canonical worktree.

In `runner/tests.rs` (reuse `isolated_git`/`isolated_git_ok`):

- `a_main_git_dir_holding_commondir_is_refused`: repository `R` with committed `README.md`
  (`one\n`); a bare clone `foreign.git` beside it with `core.bare=false` and
  `filter.evil.clean = "touch <marker>; cat"`; `R/.gitattributes` `* filter=evil`; rewrite
  `R/README.md` to `two\n` (same length, so git must re-hash it); write
  `R/.git/commondir` naming `foreign.git`. `run_git(&["status", "--porcelain"], &R)` is `Err`
  containing `commondir`; the marker is absent. Positive control last: plain `git status` in
  `R` creates the marker.

- `a_planted_replace_ref_is_ignored_by_loom_git`: repository with commits C1 and C2 on
  `main`; `git replace <C1> <C2>` with plain git; `run_git_checked(&["cat-file", "-p", C1],
  R)` prints C1's own content (its own tree line), while plain `git cat-file -p C1` (the
  positive control, last) prints C2's.

In `pinned_tests.rs` also:

- `a_pinned_status_does_not_recurse_into_a_gitlink`: in the stage worktree, record a gitlink
  `sub` (`git update-index --add --cacheinfo 160000,<oid>,sub` and a commit), make `<worktree>/
  sub` an `init`ed repository with one committed file, whose config defines the marker clean
  filter and whose `.gitattributes` applies it, and leave that file stat-dirty with a
  same-length rewrite; `for_checkout(worktree)?.run_read_only(&["status", "--porcelain"])`
  leaves the marker absent. Positive control last: plain `git status` in the worktree creates
  it. If plain git does not recurse in your fixture, find the git setting that makes it (and
  that `NO_HOOKS_ARGS` turns off) before writing the assertion; never ship a control that
  cannot fire.

In `runner/tests.rs` also: `a_gitlink_in_the_main_checkout_is_not_recursed_into` (the same
gitlink fixture as the pinned test, built in a plain repository `R`; `run_git(&["status",
"--porcelain=v1", "--untracked-files=no"], &R)` leaves the marker absent, as
`require_clean_repository` runs it; positive control last: plain `git status` in `R` creates
it).

In `status_tests.rs`: `pinned_status_lists_changes_through_the_registered_git_dir`
(`list_working_tree_changes_in(&WorktreeGit::for_checkout(worktree)?)` lists a file changed in
the worktree).

In `in_progress_tests.rs`: `a_worktree_merge_head_is_read_from_its_registered_git_dir` (write a
`MERGE_HEAD` into the registered admin directory, repoint `T/.git` to a directory without one;
`detect_in_progress_merge_at_worktree(T)` finds the merge; a `MERGE_HEAD` written only into the
repointed directory is not reported; the same holds when `T/.git` is replaced by a DIRECTORY
holding a `MERGE_HEAD`).

Git in tests: `GIT_CONFIG_GLOBAL`/`GIT_CONFIG_SYSTEM` pointed at missing files,
`GIT_CONFIG_NOSYSTEM=1`, identity and `commit.gpgsign=false` via `-c`.

## Traps

- `WorktreeGit::pinned`'s own `common_dir(repo_root)` runs `git rev-parse --git-common-dir` in
  `R` through the runner, so the guard covers it: do not add a second check there.
- Never derive `R` from git run inside `T` (`rev-parse --show-toplevel` or
  `--git-common-dir` in `T` follows `T/.git`).
- `--no-optional-locks` is a global option: it goes before the subcommand, after the
  runner's `-c` pairs, which `run` already places.
- Keep `pinned.rs` under 400 lines; move the new tests to `pinned_tests.rs`, not inline.

## Check

Once, after your last edit: `cargo test --lib git::` (worker B writes other `git/` files in the
same wave; a compile error in a file you do not own is reported with its file:line, not
fixed).
