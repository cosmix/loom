# Pre Commit Hardening

> Partial-staging guard decisions, edge cases, mutant-settled git defaults

> Design decisions and edge cases behind the pre-commit partial-staging guard, and how the black-box regression settled disputed claims about git's defaults.

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
