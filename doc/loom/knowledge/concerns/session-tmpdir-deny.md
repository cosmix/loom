# Session Deny on Claude Code's TMPDIR

> Read-only TMPDIR

`sandbox/control_surfaces/session_denies.rs:79` denies writes to every root of `models::forward_receipt::locator::default_task_output_roots()`, and `tests_session_denies.rs:32` pins it. One of those roots is `/tmp/claude-<uid>`, Claude Code's own `TMPDIR` (also named at `plan_session/guard/mod.rs:296`), while only its `tasks` subdirectories hold receipt channels. The deny covers the whole root, so in every sandboxed session:

- Every foreground Bash call that does not end in its own `exit` returns 1, because the harness's trailing `pwd -P >| /tmp/claude-<uid>/cwd-*` cannot write. A failed call never reaches the PostToolUse relay hook, so `loom memory`, `loom check` and commit requests sit at "pending relay" and the next call answers `Host job: refused ... the request never reached the daemon`.
- Heredocs fail (zsh writes its temp file under `TMPDIR`; `export TMPPREFIX=$LOOM_SCRATCH_DIR/zsh` fixes it), as do `mktemp` without a directory, vitest and `loom subagents watch` (`TMPDIR=$LOOM_SCRATCH_DIR` fixes them).
- The finishing command of a stage cannot be relayed: the `loom-control-complete.sh` hook pins it to one exact command, so it cannot end with `exit`, and `relay/sweep.rs` recovers only memory and telemetry tickets, never control kinds; the next clean call carrying a relay line recovers the ticket. A blocked stage cannot move straight to its final state (`transitions.rs:49`), so blocking does not help.

Direction: deny only the `tasks` subdirectories of the root (the receipt channels), not the root itself, and add a canary that a sandboxed Bash call returns the command's own exit code. Until then, end relayed commands with their own `exit` and point `TMPDIR` or `TMPPREFIX` at `$LOOM_SCRATCH_DIR`.
