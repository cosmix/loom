# W1: Partial-Staging Guard

## Objective

Change `loom/.githooks/pre-commit` so a partially staged path cannot be replaced in the index by
the path's full working-tree contents.

## Files

- Own: `loom/.githooks/pre-commit`
- Read-only context: `doc/loom/knowledge/mistakes/testing-and-lint.md`, section "The Pre-Commit
  Hook Re-Adds Every Staged File, So Partial Staging Is Silently Undone"
- Do not edit any other file.

## Current Seam

The hook records staged ACM paths, runs `cargo fmt` and Markdown auto-fix, and then unconditionally
executes `git add` for every recorded path. A path with staged and unstaged hunks is therefore
replaced in the index even when neither formatter changed it.

## Required Change

1. Before the first formatter or other mutating command, enumerate staged added, copied, and
   modified paths using Git's NUL-delimited output. Use Bash-compatible NUL-safe iteration so
   filenames containing spaces are preserved.
2. For every staged path, compare the working tree to the index with `git diff --quiet -- "$file"`.
   Exit status 1 means the path is partially staged; any other nonzero status is a Git error and
   must also fail the hook with a distinct diagnostic.
3. Collect every partially staged path, print all of them, and exit nonzero before `cargo fmt`,
   `bunx`, maintainability, rustdoc, or `git add` runs.
4. State the safe choices: fully stage the path, unstage it, land the other work first, or use the
   documented operator-authorized manual-check recovery. Do not add or block `--no-verify` here.
5. Preserve the current formatter, maintainability, conditional rustdoc, and re-stage behavior
   when no staged path has an additional working-tree diff.
6. Keep the script portable to the repository's Linux and macOS Bash environments. Do not use
   `mapfile`, GNU-only flags, or line-delimited filename parsing.

## Constraints

- Run the partial-staging check before every command that can alter the working tree or index.
- Never stash, reset, checkout, restore, or rewrite Git objects to simulate preservation.
- Do not broaden the hook into a repository policy redesign.
- Do not run Git commands as part of implementation workflow; Git inside the hook is product code.
- Use `apply_patch` for the edit. Do not spawn subagents.

## Narrow Check

Run exactly once after editing:

```bash
bash -n loom/.githooks/pre-commit
```

The orchestrator runs the black-box fixture after W2 returns.
