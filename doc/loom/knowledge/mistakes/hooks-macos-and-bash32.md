# Hooks Macos And Bash32

> macOS Bash 3.2 and BSD tool faults that Linux hook runs hid

## BSD `wc` Padding and Bare `sha256sum` Skipped the Reviewer Harvest on macOS, and the Hook Suite Never Ran in CI (2026-10-06)

**What happened:** on macOS no review round was ever recorded, `loom subagents watch` exited 1, and every
codex forward was blocked (issue #24).

**Why:** BSD `wc -c` left-pads its count, and five hook sites tested the raw value against `^[0-9]+$` and
returned silently on a mismatch. macOS has no `sha256sum`. The hook test suite did not run in CI at all,
and the CI runner is Linux, so a BSD form was never exercised. The `SubagentStop` hook skipping looked like
an intermittent delivery problem
([SubagentStop Delivery Is Intermittent](subagent-liveness-and-watch.md#subagentstop-delivery-is-intermittent-and-one-reviewer-can-record-two-rounds-2026-10-03)
held "the hook logic is sound" only for GNU tools).

**Prevention:** strip every `wc` count with `| tr -d "[:space:]"` before testing it; take sha256 through
`loom_lifecycle_sha256`; try the BSD `date -j -f` form before GNU `date -d`. A silent `return 0` on a
malformed value hides the bug, so a skipped reviewer stop writes a `stop-skips.jsonl` row. A contract that
runs against a shim must assert the shim really pads (exit 90 otherwise), or it passes vacuously.

**Fix:** the helpers and the stop-skips ledger in
[Portable Shell Helpers](../architecture/hook-system.md#portable-shell-helpers-and-the-subagentstop-ledgers);
`run-all.sh` runs in CI in plain and BSD-shim modes.

## A Heredoc Inside `$( )` Parses Under Bash 5 and Fails Under Bash 3.2 (2026-10-13)

**What happened:** the first `hooks-macos` CI run failed `scripts/check-hook-syntax.sh` on `loom-hooks/codex-forward.sh` (`line 387: unexpected EOF while looking for matching '''`). Every Linux job had parsed the same file cleanly, so the codex lane's wrapper was unparseable under stock macOS bash.

**Why:** the preamble was `preamble=$(cat <<'CODEX_PREAMBLE' ... )`. Bash 3.2's command-substitution lexer re-scans a heredoc body for quotes despite the quoted delimiter, so the apostrophe in "stage's" opened a single-quoted region; bash 5 parses the substitution properly. `check-hook-syntax.sh` had only ever run under bash 5. The same construct broke `spawn-guard.sh` earlier ([tests-that-cannot-fail](tests-that-cannot-fail.md)).

**Prevention:** never put a heredoc inside `$( )` in a hook or script; put it in a function and capture the function's output. Parse under bash 3.2 before pushing a hook change: `docker run --rm -v "$PWD:/r:ro" -w /r bash:3.2 bash scripts/check-hook-syntax.sh`. That proves parsing only; the image has GNU and busybox tools, not the BSD userland, so the hook suite's macOS behaviour is proven only by the `hooks-macos` job.

**Fix:** `codex-forward.sh` defines `codex_preamble()` around the heredoc and sets `preamble=$(codex_preamble)`; the captured text is byte-identical. `hooks-macos` now runs each check after a confirmed bash 3.2, whatever the others did, so a syntax failure no longer hides the hook suite.

## The First macOS Hook Run Found Bash 3.2 and BSD Faults the Linux Shims Hid (2026-10-13)

**What happened:** the first `hooks-macos` run under `/bin/bash` 3.2 failed 9 hook tests, and the
macOS Rust suite failed more through the same hooks. Production faults among them:

- `codex-apply-patch.sh` looped over an empty `"${TARGETS[@]}"` under `set -u`; bash 3.2 aborted
  the hook with exit 1, which Claude Code treats as non-blocking, so every apply_patch guard was skipped.
- `_loom_outline_covered_rows` (`_read_discipline.sh`) skipped `loom map` when neither `gtimeout`
  nor `timeout` existed, so the read guard only ever warned on stock macOS.
- `codex-forward-guard.sh` `block_forwarder` printed its message through a here-document naming
  `$HOME`; with HOME unset, `set -u` exited 1 and the call it meant to block went through.
- `_codex_forward.sh` trimmed `ps` output with a here-string. Bash 3.2 writes here-strings and
  here-documents to a temp file (`$TMPDIR`, `/tmp`, `/var/tmp`, `/usr/tmp`, then the current
  directory). With none of them writable the read fails, the padded value fails the numeric check, and
  the stage-evidence check fails open. `set -e` does not stop a script on that redirect failure.

Test-only faults: `poll-guard-subagent-waits.sh` used `$BASHPID` (bash 4+), so under 3.2 every
fixture aborted with "unbound variable" and the script still printed PASS. The BSD shims translated
BSD arguments to GNU ones even on a BSD host, and two stacked shim copies (different inodes, so
`-ef` misses them) would call each other without end, forking at every level.

**Why:** BSD behaviour was emulated on Linux only, and bash 3.2 never ran
([The Test Suite Never Ran on macOS in CI](ci-toolchain-and-cargo.md#the-test-suite-never-ran-on-macos-in-ci-2026-10-13)).

**Prevention:**

- Nothing on a blocking hook's block path may fail before `exit 2`: no here-documents or
  here-strings, `${HOME:-}` for optional variables, the message inside `{ ... } >&2 || true`, and
  `trap '' PIPE` first.
- Guard every value expansion of a possibly empty array with `${arr[@]+"${arr[@]}"}`. Use `$$`, never
  `$BASHPID`. Trim with parameter expansion, never `read <<<`.
- Bound a command with `loom_run_bounded` (`_read_ledger.sh`), never skip it or run it unbounded.
- A test shim steps aside when the host tool already speaks its dialect, and skips every candidate
  that is a shim (marker line), not only itself.
- A test that prints PASS after an error message is vacuous: read the whole output under bash 3.2.

**Fix:** each fault above; see
[Portable Shell Helpers](../architecture/hook-system.md#portable-shell-helpers-and-the-subagentstop-ledgers).

## A Shim's `--version` Probe Recursed Through a Test's Wrapper and Killed the Linux Runner (2026-10-13)

**What happened:** after the BSD shims gained a `"$real" --version` probe (to step aside on a BSD
host), CI's Linux `Hook syntax` job hung in its `LOOM_HOOK_TEST_BSD=1` pass and the runner died ("The
hosted runner lost communication with the server", no log). `worktree-file-guard-scratchpad.sh` takes
`REAL_STAT=$(command -v stat)`, which in BSD mode is the stat shim. It then puts a stub `stat` that
execs `$REAL_STAT` first on PATH. The shim picked the stub as its "real" tool (no marker, not `-ef`),
`stub --version` exec'd the shim, and the shim ran `stub --version` again as a child: an unbounded
process chain. The same loop is the probable cause of the per-user process limit (EAGAIN) hit on a Mac
while the shims were being changed.

**Why:** "the first PATH entry that is not me" accepts a wrapper that sits before the shim and hands
calls back to it. Before the probe, the shim called the wrapper with GNU arguments that it rejected, so
the loop ended at once.

**Prevention:** a wrapper resolves its real tool from the PATH entries after its own position, never
before it. Any tool that can re-enter itself through PATH carries a depth guard (an exported counter
that fails past a small bound), so a mistake costs one error, not a machine. Experiments with such
tools run under `ulimit -u` and a hard kill.

**Fix:** each shim resolves "next after me" and stops with exit 125 when `LOOM_BSD_SHIM_DEPTH`
reaches 3.
