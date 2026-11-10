# Shared brief: residual-fixes workers

Read this file in full, then your own brief. Both are short; the plan sections they name are the specification.

## The specification

The plan is `doc/plans/IN_PROGRESS-PLAN-full-suite-at-gate.md` once `loom run` has started (`doc/plans/PLAN-full-suite-at-gate.md` before). Its `## Residual work` section holds every edit as quoted text: the text to find and its replacement, in fenced blocks. Apply those blocks verbatim. Line numbers in the plan are hints from commit `c424d0e4`; when a hint and the quoted text disagree, the quoted text wins. If a quoted "find" text is not in the file, stop and report it: never improvise a different edit.

The plan's `## Decision` section explains why the changes exist. Read it once, in under a minute; you need nothing else from the plan outside your sections.

## Paths and commands

- Your session starts at the repository root of the stage worktree. The Rust crate is `loom/`: run every `cargo` command from `loom/` (`cd loom` first, in the same Bash call).
- Write every test run to a log, then read the failures. A pipe into `tail` hides the exit status:

  ```bash
  LOG="${LOOM_SCRATCH_DIR:-${TMPDIR:-/tmp}}"
  cargo test --lib host_verify:: > "$LOG/w-host.log" 2>&1; echo "exit=$?"
  rg -n -m 20 -M 300 'FAILED|panicked|^error|test result' "$LOG/w-host.log"
  ```

- The integration target (`cargo test --test integration ...`) asserts that its temp directory has no "claude" in its path, and your session's `TMPDIR` has one. Run it like this:

  ```bash
  NC_TMP="${LOOM_SCRATCH_DIR:-/var/tmp}/nc-tmp"; mkdir -p "$NC_TMP"
  case "$NC_TMP" in *[cC][lL][aA][uU][dD][eE]*) NC_TMP=/var/tmp/loom-nc-tmp; mkdir -p "$NC_TMP" ;; esac
  TMPDIR="$NC_TMP" cargo test --test integration <filter> > "$LOG/w-int.log" 2>&1; echo "exit=$?"
  ```

  A line containing `SKIP` in that output means the sandbox hides the process tree and the test returned early. Report it; never count it as a pass.

- Use `loom map --outline <file>` or `loom map --find-all <symbol>` to locate a function before reading a file, then read only the line range you need.
- Format the Rust files you wrote with `rustfmt --edition 2021 <files>`. Never run `cargo fmt`, `cargo clippy` or the full test suite: the orchestrator's `loom check` runs them once for the whole stage.

## Rules for every worker

- Write only the files your brief lists. Two workers never write one file.
- Two files are frozen contract tests that loom wrote before you started: `loom/src/commands/stage/merge/live_job_tests.rs` and `loom/tests/integration/hooks_guard_fail_open.rs`. Never edit them. Your change should turn their contract tests green; if one stays red after your edit, report the failure output.
- No new dependencies, no new files beyond those your brief names, no edits outside the quoted ones.
- Every Rust file stays at or under 400 lines, and `loom/src/orchestrator/signals/cache.rs` stays at exactly 511 lines. Check with `wc -l`.
- No git commands. The orchestrator commits.
- Record a surprise, a mistake or a quoted text that did not match with `loom memory note "<what happened and why>"` as soon as it happens. Never `loom knowledge` and never Claude Code auto-memory.

## Your report

End with a short report:

- each file you changed and what changed in it;
- each command you ran, with its exit status and the `test result` line;
- anything you did not do, and why;
- any quoted text that did not match the tree.
