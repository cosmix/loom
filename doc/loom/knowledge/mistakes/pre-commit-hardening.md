# Pre Commit Hardening

> Partial-staging guard edges

Design decisions and edge cases behind the pre-commit partial-staging guard, and how the black-box regression settled disputed claims about git's defaults.

## The Guard: What It Checks and Why (2026-09-12)

`loom/.githooks/pre-commit` unsets `GIT_LITERAL_PATHSPECS`, `GIT_GLOB_PATHSPECS`,
`GIT_NOGLOB_PATHSPECS`, `GIT_ICASE_PATHSPECS` first, enumerates staged paths NUL-safe with
`git diff --cached --no-renames --name-only -z --diff-filter=ACMT`, then for each path runs
`git diff --quiet -- ":(literal)$file"` and exits 1 naming every path where index and working
tree differ — before `cargo fmt`, markdownlint, or any `git add`. Design choices, each checked
against the tree rather than assumed:

- **Shebang is `#!/usr/bin/env bash`, not `#!/bin/sh`.** NUL-safe iteration needs `read -r -d ''`,
  which dash (Ubuntu's `/bin/sh`) lacks; bash 3.2 (macOS's shipped version) supports it, so the
  no-`mapfile` portability constraint still holds.
- **`--no-renames --diff-filter=ACMT`, not a bare `--diff-filter=ACM`.** `git diff` enables
  `diff.renames` for porcelain output by default since git 2.9 — verified true even with
  `GIT_CONFIG_GLOBAL=/dev/null` and `GIT_CONFIG_NOSYSTEM=1` — so a `git mv`'d path edited again in
  the working tree shows as `R` and would escape an ACM-only filter. **A reviewer claimed git
  diff does not default to detecting renames; a mutant hook built without `--no-renames` was run
  against the fixture and failed the renamed-path case, refuting the claim.** Settle a
  git-default dispute with a mutant run, not by reasoning about it.
- **Two `git diff --quiet` calls stay separate rather than sharing a loop helper**, and there is no
  submodule-dirtiness handling — the repo's `.gitmodules` exists but is 0 bytes and the index
  holds no `160000` gitlink entries, so there are no submodules to special-case. That conclusion
  was reached correctly, but only after being reached once *without* checking (`.gitmodules`
  existing was read as "there might be submodules" and dismissed on assertion, not evidence) —
  before dismissing a submodule edge case, run `test -f .gitmodules && git ls-files -s | rg '^160000'`.
- **The guard cannot false-positive under `git commit -a` or `git commit <path>`.** Git builds the
  hook's `GIT_INDEX_FILE` as a temporary index from `HEAD` plus the named/all-modified paths for
  that commit, so `git diff --quiet` sees index == working tree for those paths. The flip side is
  a real limitation, not a bug: `git commit <path>` (`--only`) does **not** trip the guard for a
  *different*, genuinely partially staged file — that file's partial entry is simply absent from
  the temporary index the hook inspects.

## Why the Hook Was Never Syntax-Checked (fixed 2026-09-12)

`scripts/check-hook-syntax.sh` only parsed `*.sh` under `loom-hooks/` and `scripts/`; a git hook has no
extension, so `loom/.githooks/pre-commit` was invisible to it and to the CI hook-syntax step. The
partial-staging fixture's fully-staged path was the only thing that ever ran the hook to its last
line. Fixed by adding a second `find` pass over `loom/.githooks` for executable files
(`scripts/check-hook-syntax.sh:23,49`).

## Test Design: a Space in a Filename Does Not Prove NUL-Safety

A fixture filename containing only an internal space does not distinguish `-z`/NUL enumeration
from newline enumeration: git does not C-quote a bare space, and `read -r` keeps it. A mutant
that enumerates by newline still passed the space case. Use a name git *does* C-quote without
`-z` (a double quote, a backslash, a tab, or non-ASCII under default `core.quotePath`) to make a
newline-enumeration mutant actually fail.

## Commit formatting can change unrelated unstaged Markdown

**What happened:** The core repair commit hook reformatted an unrelated tracked plan brief while linting all tracked Markdown, leaving that change unstaged.

**Why:** The hook formats the whole tracked Markdown set and only restages paths already selected for the commit.

**Prevention:** Compare the post-commit working tree with the reviewed path inventory; preserve unrelated original content instead of absorbing formatter drift into a documentation commit.

**Fix:** Revert only the newly introduced formatting diff in doc/plans/briefs/live-token-insights/token-core/c2-model-block.md, leaving all prior user work untouched.

## Commit requests can include existing tracked changes

**What happened:** A request to commit the work and docs was interpreted as limited to the current fix, leaving existing README, installer, and knowledge edits uncommitted; the user clarified that all modified files were intended.

**Why:** Task ownership was used to narrow commit scope beyond the requested scope.

**Prevention:** Apply an explicit request covering all modified files to existing tracked changes too, reviewing and grouping them before committing.

**Fix:** Review and commit the remaining modified tracked files in logical Conventional Commits while keeping unrelated untracked artifacts separate.

## The Pre-Commit Markdown Autofix Ran Over Every Tracked File and Corrupted Content (2026-10-20)

**What happened:** each `loom stage commit` left six archived plans modified and the tree dirty. The autofix also changed content: `_: &Runtime` became `*: &Runtime` in a code line of `REVIEW-PLAN-container-backend.md`, and `loom-*containers` became `loom-_containers`. The changes were reverted with `git restore`.

**Why:** `loom/.githooks/pre-commit` ran `bunx markdownlint-cli2 --fix` over every tracked `.md`, including `doc/plans/archive`, which the gate's markdownlint excludes, and re-staged only the staged files.

**Prevention:** run an autofixer only on the staged paths and mirror the gate's exclusions, because `markdownlint-cli2` does not apply `.markdownlintignore` to paths named on its command line.

**Fix:** the hook lints only the staged markdown (`loom/.githooks/pre-commit:59-72`).

## The Staged-Markdown Narrowing Left the Regression Script Asserting the Old Behavior (2026-10-22)

**What happened:** 19fd93b6 narrowed the pre-commit Markdown autofix to staged files, but `scripts/test-pre-commit-partial-staging.sh` still required `bunx` to run in its `fully-staged` case, which stages only a `.rs` file. CI failed on both Linux and macOS hook jobs.

**Why:** The hook change was verified by hand, not by the regression script that pins the hook's flow.

**Prevention:** Any edit to `loom/.githooks/pre-commit` runs `./scripts/test-pre-commit-partial-staging.sh` before commit.

**Fix:** `fully-staged` now stages a README.md change and asserts `bunx` linted it by name; `unstaged-only` asserts `bunx` did not run for the unstaged README.
