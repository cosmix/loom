# C: pinned git in the shell hooks, the D1 commit instruction, the host-git call-site guard

Stage `host-git-integrity`, wave 1, tier sonnet. Read `../common.md` first (D1, D2).

## Why

Claude Code runs loom's hooks on the host, unsandboxed, with the session's cwd. Three hooks run
bare `git` there, so git discovers its directory through the worktree's `.git` file, which the
agent can rewrite to name a directory whose config defines a clean filter or
`core.fsmonitor` (both run by `git status`):

- `loom-hooks/commit-guard.sh`: `detect_loom_worktree` (`git rev-parse --abbrev-ref HEAD`),
  `check_git_clean` and `get_uncommitted_changes` (`git status --porcelain`).
- `loom-hooks/stage-terminal-guard.sh`: `detect_loom_worktree` (`git rev-parse`).
- `loom-hooks/codex-forward-guard.sh`: `resolve_workspace_root` (`git -C "$cwd" rev-parse
  --show-toplevel`, which a foreign config's `core.worktree` can redirect).

`poll-guard.sh`, `no-preexisting-failures.sh` and `_progress-classification.sh` only classify
the agent's own git command lines; they run no git. Leave them alone.

## Files you own

`../loom-hooks/_common.sh`, `../loom-hooks/commit-guard.sh`,
`../loom-hooks/stage-terminal-guard.sh`, `../loom-hooks/codex-forward-guard.sh`,
`../loom-hooks/tests/pinned-git.sh` (new), `../loom-hooks/tests/commit-guard-pinned-status.sh`
(new), `../loom-hooks/tests/commit-guard-sigpipe-many-dirty-files.sh`,
`../loom-hooks/tests/run-all.sh`, and (section 4) `tests/host_git_call_sites.rs` (new),
`tests/host_git_call_sites.txt` (new), both relative to `loom/`.

## 1. Helpers in `_common.sh` (append at the end; bash 3.2, POSIX awk, no realpath)

```bash
# loom_git_repo_root - Echo the main checkout R that holds the loom state
# directory LOOM_WORK_DIR names (<R>/.loom/work, or <R>/.work on a legacy
# workspace), canonical (`cd && pwd -P`). Returns 1 when LOOM_WORK_DIR is
# unset or has neither shape, when loom_main_git_dir fails for R, or when
# that git directory holds `commondir`, which would point git at another
# repository.
loom_git_repo_root()

# loom_main_git_dir <R> - Echo G, the main checkout's git directory,
# canonical: R/.git when it is a directory (not a symlink); when R/.git is a
# regular file (a `git init --separate-git-dir` checkout), the directory its
# one `gitdir: <path>` line names (relative paths resolve against R).
# Returns 1 for anything else. The plan's one layout rule (D7), shared with
# worker B's `checkout_integrity::main_git_dir`. Runs no git.
loom_main_git_dir()

# loom_checkout_root <dir> - Echo the checkout that holds <dir>: the stage
# worktree R/.worktrees/<id> when <dir> (canonical) lies inside one, else R
# when it lies inside R. Returns 1 when <dir> is outside R. Structural: runs
# no git and never reads a .git file.
loom_checkout_root()

# loom_git_admin_dir <R> <T> - Echo the one directory under G/worktrees/
# (G from loom_main_git_dir; a directory, not a symlink) whose `gitdir` file
# names <T>/.git and whose `commondir` resolves to G. Returns 1 for none or
# more than one.
loom_git_admin_dir()

# loom_pinned_git <checkout> <git args...> - Run git for <checkout> (R or a
# stage worktree of R, as loom_checkout_root gives it) with GIT_DIR,
# GIT_COMMON_DIR and GIT_WORK_TREE taken from loom's records, never from a
# .git file: G (loom_main_git_dir) for R, the registered admin directory for
# a worktree; GIT_COMMON_DIR is G either way.
# Adds -c core.fsmonitor=false -c core.hooksPath=/dev/null
# -c diff.ignoreSubmodules=all -c submodule.recurse=false
# -c status.submoduleSummary=false --no-optional-locks, exports
# GIT_NO_REPLACE_OBJECTS=1 for the call, and drops GIT_INDEX_FILE. Returns
# git's status, or 1 without running git when the checkout cannot be pinned.
loom_pinned_git()
```

The three submodule flags matter because a pinned `status` without them printed `M sub` and ran
a clean filter defined in an agent-written `T/sub/.git` (measured). They mirror the runner's
`NO_HOOKS_ARGS` (worker A), which carries them for every loom git call.

Mirror `loom/src/git/worktree/pinned.rs::registered_admin_dir` and `names_work_tree`: read the
first line of `gitdir`/`commondir`, resolve a relative one against the admin directory, compare
canonical directories (`cd "$(dirname "$g")" && pwd -P` for the `gitdir` target, whose basename
must be `.git`). The worktree id is the first path component after `$R/.worktrees/`. When
`LOOM_WORKTREE_PATH` is set and names a different worktree than the one `loom_checkout_root`
derived, `loom_pinned_git` returns 1.

`R` comes from `LOOM_WORK_DIR` only. Never from cwd: the agent can `cd` anywhere, and a fake
`<dir>/.worktrees/x` tree with its own `.git` is one `mkdir` away. The registry check is what
turns a cwd-derived worktree into a trusted one.

## 2. The hooks

- `commit-guard.sh`: `source "$(dirname "${BASH_SOURCE[0]}")/_common.sh"` near the top.
  `detect_loom_worktree` method 2: `root=$(loom_checkout_root "$(pwd -P)")` and
  `loom_pinned_git "$root" rev-parse --abbrev-ref HEAD`; any failure is "not a loom worktree"
  (its current failure outcome). `check_git_clean` and `get_uncommitted_changes` run
  `loom_pinned_git "$root" status --porcelain` for the checkout the hook resolved. A status that
  cannot run pinned is NOT clean: the checklist's first item says the changes could not be
  listed because the worktree's git directory could not be pinned from loom's records, and
  asks the agent to check `git status` itself. Keep the `status=$(...)` capture before `head`
  (the SIGPIPE fix).
- In the same hook, replace the two lines that tell the agent to run `git add <specific-files>
  && git commit` and not to use `git add -A` with common.md's D1 sentence, joined into one
  line, word for word:

      Commit with `loom commit -m "<type(scope): description>" -- <files>` in one Bash call, then run `loom request status <id> --wait 90` in the next call; `git add` and `git commit` fail in a stage worktree because its git directory is read-only.

  Hold it in a single-quoted `readonly` variable so no backtick or quote needs escaping.
  commit-guard only reaches this text for worktree sessions (merge sessions exit early, a
  knowledge session in `R` on `main` is never detected).
- `stage-terminal-guard.sh`: source `_common.sh` and pin method 2 the same way. Rewrite the
  comment that says the copied functions are kept "not sourced, so this hook has no load-order
  dependency" to state the dependency on `_common.sh` now.
- `codex-forward-guard.sh::resolve_workspace_root`: replace the `git -C "$cwd" rev-parse
  --show-toplevel` block with `candidate=$(loom_checkout_root "$cwd" 2>/dev/null || true)`,
  keeping the existing fallback to `$cwd` when it is empty. No git runs there any more. The
  existing codex-forward tests use a `LOOM_WORK_DIR` without the `.loom/work` shape and a cwd
  outside any repository, so they exercise the fallback and must stay green unchanged.

## 3. Tests

- `tests/pinned-git.sh` (new). Fixture in a `mktemp -d`: `R` = `git init -b main`, commit
  `README.md` (`one\n`), `mkdir -p R/.loom/work`, `git -C R worktree add -q -b loom/s1
  .worktrees/s1`. A bare clone `foreign.git` beside `R` with `core.bare=false`,
  `symbolic-ref HEAD refs/heads/loom/s1`, `filter.evil.clean = touch <tmp>/filter-ran; cat` and
  `core.fsmonitor = touch <tmp>/fsmonitor-ran; false`. Point `T/.git` at it (`gitdir:
  <foreign>`), `git read-tree HEAD` in `T`, write `T/.gitattributes` `* filter=evil` and
  `T/README.md` `two\n`. Setup git runs with `GIT_CONFIG_GLOBAL=/dev/null
  GIT_CONFIG_NOSYSTEM=1` and `-c user.name=t -c user.email=t@t`. Source `_common.sh`, then
  assert, each with an explicit `LOOM_WORK_DIR` and `LOOM_WORKTREE_PATH` unset:
  1. `LOOM_WORK_DIR=$R/.loom/work loom_pinned_git "$T" status --porcelain` succeeds, lists
     `README.md`, and neither marker exists.
  1b. A committed gitlink `sub` (set it up before the clone, with `git update-index --add
     --cacheinfo 160000,<oid>,sub` and a commit on `loom/s1`, so both git directories record
     it) plus an agent-written `T/sub/.git` (an `init`ed repository) whose config defines a
     clean filter touching its own marker `sub-filter-ran`, `T/sub/.gitattributes` `*
     filter=evil`, and a stat-dirty `sub/f` (a same-length rewrite of a committed file).
     `loom_pinned_git "$T" status --porcelain` succeeds, does not list `sub`, and leaves
     `sub-filter-ran` absent. Its positive control (plain `git -C "$T/sub" status`, which
     creates the marker) runs at the very end, after case 7, so it cannot leak into the
     marker-absent assertions of the other cases.
  2. `loom_checkout_root "$T/sub"` (after `mkdir "$T/sub"`) echoes `$T`; of `$R` echoes `$R`.
  3. The legacy layout: `mkdir R/.work`, `LOOM_WORK_DIR=$R/.work` pins too.
  4. `LOOM_WORK_DIR` unset: returns non-zero, no marker.
  5. `LOOM_WORK_DIR` naming a second repository's `.loom/work` (that does not register `T`):
     non-zero, no marker.
  6. `R/.git/commondir` written: `loom_git_repo_root` fails; remove it afterwards.
  1c. A second fixture `R2` made with `git init -b main --separate-git-dir <tmp>/store2.git
     R2` (so `R2/.git` is a gitfile), the same commit, `.loom/work`, stage worktree `T2`,
     repointed `T2/.git` and filter as case 1, with its own marker `filter2-ran`.
     `loom_main_git_dir "$R2"` echoes the canonical store; `LOOM_WORK_DIR=$R2/.loom/work
     loom_pinned_git "$T2" status --porcelain` succeeds, lists `README.md` and leaves
     `filter2-ran` absent. Its positive control (plain `git -C "$T2" status`) joins the final
     controls after case 7.
  7. The positive control: plain `git -C "$T" status --porcelain` creates `filter-ran`,
     proving the fixture live; 1b's control follows it as the very last step.
- `tests/commit-guard-pinned-status.sh` (new): the same fixture plus
  `R/.loom/work/stages/01-s1.md` with `status: executing`; run the hook from `T` with
  `LOOM_WORK_DIR=$R/.loom/work` and `env -u LOOM_MERGE_SESSION`. Output contains
  `COMPLETION CHECKLIST for stage 's1'`, `README.md` under "Modified files:", the literal
  `loom commit -m "<type(scope): description>" -- <files>`, and not `git add <specific-files>`;
  no marker exists. A second run without `LOOM_WORK_DIR` still exits 0, still prints the
  checklist with the could-not-be-listed item, and creates no marker.
- `tests/commit-guard-sigpipe-many-dirty-files.sh`: its fixture initialises a standalone
  repository inside the worktree directory, which no longer pins. Change only setup lines: make
  `PROJECT_ROOT` a repository with one commit, add the worktree with `git worktree add` (the
  `mkdir -p "$WORKTREE"` before it leaves an empty directory, which `worktree add` accepts), and
  `export LOOM_WORK_DIR="$PROJECT_ROOT/.loom/work"` before the loop. Do not edit its loop
  condition or any `if [[ ... ]]` check.
- `tests/run-all.sh`: add `-u LOOM_WORK_DIR -u LOOM_WORKTREE_PATH` to `run_test`'s `env -u`
  list (a stage session exports both), and one `run_test` line for each new test.

## 4. The host-git call-site guard (`loom/tests/host_git_call_sites.rs`)

D2 holds only while no new host code discovers git through a worktree's `.git`. Add an
integration test that makes that mechanical, in the shape of `tests/maintainability.rs` and its
checked-in `maintainability-baseline.txt`.

- It scans every `.rs` file under `loom/src` and reports each file whose NON-TEST text calls
  one of: `run_git(`, `run_git_checked(`, `run_git_with_env(`, `run_git_bool(`,
  `run_git_pinned(`, `run_git_pinned_checked(`, `run_git_pinned_within(`,
  `WorktreeGit::discovered(`, `Command::new("git")` (word-bounded, so `run_git(` does not match
  inside `run_git_checked(`; the last four are discovery too), or one of the `&Path` helpers
  that reach a worktree through discovered git: `has_uncommitted_changes(`,
  `list_working_tree_changes(`, `commits_ahead_of(`, `is_ancestor_of(`, `current_branch(`,
  `get_conflicting_files(`, `merge_head_exists(` (the files X1, X3 and X5 convert reach `T`
  through these, so without them the guard would never have flagged those files; the files that
  define them match their own definitions and are classified by what they run). Non-test text:
  skip files named `tests.rs`, `*_tests.rs`, `tests_*.rs`, `test_support.rs`, files under a
  `tests/` directory or a directory named `tests_*`, and every item gated `#[cfg(test)]`: from
  the attribute line (with any `#[path = ...]` or other attribute lines after it) through the
  item's closing brace at depth zero, or its terminating `;` for `mod x;` and `use`. That covers
  `#[cfg(test)] mod tests { ... }`, `#[cfg(test)] #[path = "x.rs"] mod tests;` and `#[cfg(test)]
  pub(crate) fn`, not only modules. Count braces after stripping `//` line comments (a brace
  inside a string literal is a known imprecision). `src/git/runner.rs` and `src/git/runner/**`
  define the functions and are exempt by name in the test.
- The allowlist `loom/tests/host_git_call_sites.txt` has one line per file: `<path relative to
  loom/> | <class> | <one-line reason>`, `#` comments allowed. Classes, stated in the file's
  header comment:
  - `host-on-R`: runs git outside the sandbox through the runner, with its cwd in the main
    checkout (or a directory the operator named), where the runner's `commondir` guard
    applies. Only files whose git goes through the runner get this class.
  - `host-direct`: runs `Command::new("git")` outside the sandbox without the runner, so the
    `commondir` guard and `GIT_NO_REPLACE_OBJECTS` do not apply; its reason names why it is not
    converted.
  - `host-pinned`: runs git outside the sandbox against a stage worktree, only through
    `WorktreeGit::pinned`, `pinned_in_project_of`, `for_checkout` or `for_dir`; its direct
    runner calls are on `R`.
  - `in-session`: runs only inside a stage session's sandbox (the agent's own CLI), where git
    following the agent's `.git` is the agent's own business.
- The test fails, naming each, for: a matching file with no entry; an entry whose file no
  longer matches (stale); an unknown class; an empty reason; a duplicate entry.
- Write the scanner as a function over a source root and the allowlist text that returns the
  problems, so a self-test can drive it: `#[test] fn
  the_scanner_names_an_unlisted_file_and_skips_test_code`. On a `TempDir` tree it must report
  an unlisted file holding `run_git(`; report nothing for the same call inside `#[cfg(test)]
  mod tests { ... }`, inside a `#[cfg(test)] pub(crate) fn`, or under a `tests_*/` directory;
  and report a stale entry. It keeps the guard from passing on an emptied allowlist or a
  scanner that matches nothing.
- Write the list for the END state of this stage, not for HEAD: read
  `x-codex-units.md` and `a-pinned-host-git.md` (in this directory) for what each converted
  file calls afterwards (for example `verify/criteria/cache_fingerprint.rs` calls no runner
  function once X4 lands; `git/cleanup/removal.rs` calls `WorktreeGit::discovered(repo_root)`,
  `host-on-R`). Classify each remaining file by reading its call sites (`rg -n` the patterns
  above over `src`; count the files after PLAN-stage-exits-and-environment merged, which adds
  `run_git_checked` callers such as `orchestrator/core/provision_gate.rs`,
  `verify/impact_tests.rs` and `v2_lints/repo_hooks.rs`). A file you cannot classify gets your
  best class and the reason "UNVERIFIED: <what is unclear>"; list those in your report.
- Direct `git` spawns: the files expected to end the stage as `host-direct` are
  `commands/knowledge/annotate.rs`, `commands/pressure/paths.rs` and
  `commands/handoff/create.rs`; verify each by reading its call site, and give each a reason
  that names why it stays unconverted. `commands/stage/merge/preflight.rs` runs `rev-parse
  --show-toplevel` through the runner in the cwd of an operator's `loom stage merge`: classify
  it `host-on-R` and say in its reason that it is an operator path.
- The floor: the final list contains the line `src/git/cleanup/removal.rs | host-on-R |
  <reason>` (the plan's acceptance greps for `src/git/cleanup/removal.rs | host-on-R`), with
  exactly that spacing, `<path> | <class> | <reason>`, on every line.
- Files other stages of this plan add in parallel, which this stage cannot list (its own test
  would call them stale) and the integration-verify stage adds: `src/commands/commit/paths.rs |
  in-session`, `src/sandbox/session_fs.rs | host-on-R`, `src/process/sandbox_probe/srt.rs |
  in-session` (test support, run in-session and by the srt harness) and
  `src/orchestrator/core/inbox_drain/commit.rs | host-pinned`. Do not list them; name them in
  your report as the lines integration-verify adds.
- The main agent runs it after wave 2 (`cargo test --test host_git_call_sites`); its output
  names every missing or stale entry, and the main agent reconciles the list then. Your one
  check below does not cover it.

## Traps

- `_common.sh` is sourced by many hooks under `set -euo pipefail`: every new function must be
  safe under `set -u` (default every optional variable with `${X:-}`) and must not `exit`.
- `git worktree add` refuses a non-empty directory: create files in `T` only after it.
- Run hook tests with `GIT_CEILING_DIRECTORIES` unset or pointed at the temp dir, as the
  existing commit-guard tests do.

## Check

Once: `bash ../loom-hooks/tests/run-all.sh` from `loom/` (95 passed, 0 failed at `2908339a`).
