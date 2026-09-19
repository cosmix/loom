---
---
# Hooks Shell Portability

> gawk/bash portability, redirects, set -e, hook test env

## gawk vs POSIX awk (2026-03-31)

**What happened:** Initial `_common.sh` used gawk-specific `match()` with array capture (3rd argument), which failed with syntax errors on standard awk and macOS default awk.
**Why:** gawk extensions are not available on all platforms. macOS ships with BSD awk.
**How to avoid:** Always use POSIX awk features only. For complex string extraction, use `substr()`+`sub()` approach instead of `match($0, pattern, arr)`.

## Hook Integration Tests Need _common.sh (2026-03-31)

**What happened:** After adding `_common.sh` as a dependency sourced by hooks, 12 integration tests in `hooks_commit_filter.rs` failed because the test setup didn't install `_common.sh` alongside the hook script.
**Why:** Hooks source `_common.sh` via `source "$(dirname "$0")/_common.sh"` — tests must install all dependencies in the temp directory.
**How to avoid:** When adding shared utilities sourced by hooks, update ALL integration test `setup_hook()` functions to also install the shared utility.

## Cross-Platform Timeout in Hooks (2026-03-31)

**What happened:** `git-add-guard.sh` used bare `timeout` command without `gtimeout` fallback, which fails silently on macOS without GNU coreutils.
**Why:** macOS doesn't have `timeout` by default; GNU coreutils provides it as `gtimeout`.
**How to avoid:** All hooks reading stdin MUST use the three-way cascade: `gtimeout` → `timeout` → `cat`.

## `${#arr[@]-0}` Is Not a Portable Empty-Array Guard (2026-08-10)

**What happened:** A macOS fix for `set -u` empty-array expansion (commit `f63f14e0`) also rewrote the _length_ checks in `install.sh` as `${#found[@]-0}` / `${#backups[@]-0}`. Bash 5 (Linux) rejects that outright — `install.sh: line 280: ${#found[@]-0}: bad substitution` — so `dev-install.sh` was fixed on macOS and broken on Linux.
**Why:** `${#param}` is the _length_ expansion and accepts no `-default` / `:-default` modifier; the two forms cannot be combined. The macOS bug was never in the length check — bash 3.2 aborts on the _value_ expansion `"${arr[@]}"` of an empty array, and the original crash report proves it (it died in the `for item in "${found_other[@]}"` loop, which is only reached _after_ both `${#...[@]}` checks evaluated cleanly).
**Prevention:** `${#arr[@]}` is safe under `set -u` in every bash including 3.2 — leave length checks alone. Guard only value expansions, with `${arr[@]+"${arr[@]}"}`. Any `${#...[@]}` with a modifier attached is a syntax error, not a portability guard.
**Fix:** Reverted both length checks to `${#arr[@]}`, kept the `+`-guarded loops, and left a comment at the loop naming both halves. Verify shell installer changes on both platforms — a bash 3.2-only fix can be a bash 5 syntax error.

## Repo hook scripts do not need the executable bit (2026-07-06)

**What happened:** After creating a new hook script, attempted `chmod +x` in the repo (blocked by the sandbox on `loom-hooks/`).
**Why:** The repo copies are sources, not the installed artifacts — `install.sh` and `fs/permissions/hooks.rs::install_hook_script` both chmod 755 at install time.
**Prevention:** Skip chmod for files under `loom-hooks/`; run tests via `bash loom-hooks/tests/run-all.sh` (invokes each script with `bash`, no exec bit needed).
**Fix:** None needed — dropped the chmod.

## The Finalization Guard Hook Scans Bash Command Text, Including Heredoc Bodies

**What happened:** `loom knowledge replace-section`/`update` calls piping prose through a heredoc
were rejected by `loom-hooks/loom-control-complete.sh` with `LOOM_CONTROL_ERROR: completion must be one
exact pinned command`, even though the actual command was a harmless knowledge write. Bisecting
showed the trigger was purely textual: a heredoc body mentioning the orchestration unit by name
("stage") somewhere earlier, and a word containing "finish"/"done"-ish vocabulary for it later in
the SAME Bash invocation, tripped it regardless of what command wrapped the heredoc — a bare
`cat`/`wc`/`python3` reproduced it identically. A literal backtick-wrapped heading argument naming
both concepts on the command line itself also tripped it. (This note deliberately avoids writing
the two trigger words themselves back-to-back in one sentence, for the obvious reason.)

**Why:** the guard tokenizer catches forged finalization commands at real command-start positions,
but the original version scanned the RAW command string, heredoc bodies and quoted arguments
included, and could not tell literal prose from live shell tokens.

**Status (2026-09-19):** the guard now strips heredoc bodies first (`is_completion_command`,
`loom-hooks/loom-control-complete.sh:143-216`) and ignores a body only when it is provably inert:
every body terminated, single fully quoted `<<'WORD'` openers, no comment, backtick, `$'`, `${`,
arithmetic or line splice, no `)` in a stripped line when `$(`/`<(`/`>(` exists, and every command on
an allowlist of inert readers (`cat tee wc head tail cd mkdir touch echo printf true :`, `loom
knowledge`, `loom memory`, `git commit`). Anything else takes the raw decision, plus the raw
substring test when the body is fed to a shell or an interpreter. Details and the residual cases:
[hook-content-stripping](../patterns/hook-content-stripping.md).

**Prevention:** prefer a quoted delimiter and an inert reader (`loom knowledge update|replace-section`,
`loom memory`, `cat`, `tee`, `git commit -F -`). The workaround of avoiding the trigger words is needed
only for unquoted delimiters, bodies inside `$( )` that contain a `)`, and commands outside the
allowlist; there, either rephrase to avoid the literal trigger words or extract dynamic values via a
prior command substitution so the trigger text never appears in the Bash command itself. A long body
belongs in a file fed with `- < file`.

## Skill Trigger Ranking Depended on Python's Per-Process Hash Seed

**What happened:** `loom-hooks/skill-trigger.sh` capped suggestions at 3 and sorted only by score. Skills tied on score kept the insertion order of a dict filled while iterating a `set` of prompt tokens, and Python seeds set iteration per process (`PYTHONHASHSEED`). The same prompt listed `loom-react` on some runs and dropped it on others, which read as "the trigger does not work" when it was a coin toss at the cut line. A second amplifier: the generic word `type` was a declared trigger of `loom-typescript`, exempt from the stopword list because the skill name starts with it, and then boosted to the name-match weight, so "types" in a prompt outranked the framework name.
**Why:** A sort key that leaves ties unresolved is deterministic only within one process. The stopword exemption for name prefixes (`test` for `loom-testing`, `debug` for `loom-debugging`) is right for the verbs and wrong for a generic noun that happens to prefix a skill name.
**Prevention:** Any ranking that feeds a truncation needs a total order (score, then a stable secondary, then name). Test it by running the hook under several `PYTHONHASHSEED` values and asserting byte-identical output, which `loom/tests/integration/hooks_skill_trigger.rs` now does. When adding a trigger that is a stopword, ask whether it would appear in prompts about anything else.
**Fix:** `loom-hooks/skill-trigger.sh` sorts by `(-score, -distinct matched keywords, name)`, lists every qualifying skill up to a flood ceiling of 8, drops the `/loom-skills` line when a domain skill already names the loader, and appends one combined `Skill(skill="loom-skills", args="a b c")` line; `type` and `error` were removed as bare triggers.

## `rg -r` Is `--replace`, Not `--recursive` (2026-08-08)

`rg -rn PATTERN` is **not** `rg -n --recursive`. `rg` has no `-r` shorthand for recursive, so `-r`
consumes the `n` as a `--replace` value and every match prints as the literal `n` — output looks like
a mangled source file (e.g. `pub n(work_dir: &Path)`) rather than an error, so it reads as a corrupt
file. `rg` is recursive by default; never pass `-r` unless you mean `--replace`.

## Capsule Ran a Python Hook Under `/bin/bash` (2026-09-14)

**What happened**: Every UserPromptSubmit in a stage session hit Claude Code's 30 s hook timeout ("UserPromptSubmit hook timed out after 30s"). The session capsule (`orchestrator/terminal/native/session_settings/contents.rs`, `in_bash_form`) rewrote every hook command as `/bin/bash <script>`; `loom-hooks/skill-trigger.sh` is a Python script (`#!/usr/bin/env python3`). Bash ran its lines as shell: `import json` executed ImageMagick's `import`, which grabs the X display the wrapper passes through (`DISPLAY`) and waits for a mouse click.

**Why**: The rewrite pinned the shell to keep hooks off the session's PATH and execute bit, but assumed every hook is bash. A `.sh` name on a Python file hid the mismatch, and outside the sandbox nothing exercised the capsule's command form. The transcript record that names the culprit is the `hook_cancelled` attachment (`command`, `durationMs`, `timedOut`).

**Prevention**: A hook command's interpreter is decided per script from its shebang, never per capsule; a preflight resolves each interpreter on the pinned hook PATH. When a hook times out, read the session transcript's `hook_cancelled` attachment first; it names the exact command. Never diagnose a hook by running it under a different environment than the wrapper's (`env -i` with the wrapper's allowlist, `DISPLAY` included).

**Fix**: `HostFacts` carries `python3` (found on `hook_path`) and the shebang-detected Python hook scripts; the capsule writes `<python3> <script>` for those and `/bin/bash <script>` for the rest, dropping a Python hook with a warning when no python3 is pinned. `python3` joined preflight check 4's `HOOK_TOOLS`.

## `BASH_SOURCE[0]` Is Unset When the Script Arrives on Stdin (2026-09-15)

**What happened**: The documented install, `curl -fsSL .../install.sh | bash`, died at `install.sh:13` with `BASH_SOURCE[0]: unbound variable` and `cd: null directory`, followed by curl's `(23) Failure writing output` once bash closed the pipe. Reported from an Ubuntu 26.04 live CD (bash 5.3.9); bash 5.2 fails the same way. The line dated from the script's creation (2025-12-21), so the remote install path had never run.

**Why**: When bash reads a script from stdin, `BASH_SOURCE` is an empty array, and under `set -u` the expansion `"${BASH_SOURCE[0]}"` aborts. Every local run used `bash ./install.sh`, which sets it, and no test fed the script on stdin.

**Prevention**: In a script meant for `curl | bash`, read `${BASH_SOURCE[0]:-}` and treat empty as "no source file". Do not fall back to `$0` or the working directory: a pipe run from inside a checkout would then take the local path. Test the pipe form itself with `bash -s -- --help < install.sh`.

**Fix**: `SCRIPT_DIR` stays empty without a source file and `is_curl_pipe` returns true when it is empty (`install.sh:13-17`, `install.sh:82-87`). `install_sh_runs_when_piped_on_stdin` in `loom/tests/integration/install_assets.rs` pipes the script to `bash -s -- --help`.

## Stop hook exited 141: `cmd | head` under pipefail (2026-09-14)

**What happened**: `loom-hooks/commit-guard.sh` piped `git status --porcelain` into `head -10` under `set -euo pipefail`. With 116 dirty paths in the completion-recovery worktree, `head` closed the pipe while git was still writing; git died with SIGPIPE (141), pipefail propagated it through the command substitution, and the hook exited 141 with no stderr. Claude Code reported `Stop hook error: Failed with non-blocking status code: No stderr output`. Racy: 16 of 60 runs failed.
**Why**: A producer that writes after `head` exits gets SIGPIPE; `pipefail` turns that into the pipeline's status, and `set -e` turns an assignment from `$(...)` into an exit.
**Prevention**: In any hook under `pipefail`, never pipe an external command directly into `head`/`sed -n 1p`/`grep -q`. Capture the full output into a variable first, then truncate with `head -n N <<<"$var"`, or append `|| true` to the producer when the exit status is not needed. A non-zero hook exit with empty stderr and exit code 141 is this bug.
**Fix**: `get_uncommitted_changes` captures the status first and truncates from a here-string; regression test `loom-hooks/tests/commit-guard-sigpipe-many-dirty-files.sh`.

## `cmd >>"$file" 2>/dev/null` Still Prints the Open Error (2026-09-19)

Redirections apply left to right, so `printf ... >>"$file" 2>/dev/null` opens `$file` BEFORE stderr is
silenced, and bash's own `Permission denied` reaches the terminal when the open fails. Write
`{ cmd >>"$file"; } 2>/dev/null`, or put `2>/dev/null` before the file redirect. The reads-ledger append
(`loom-hooks/_read_ledger.sh:126`) had this and was fixed.

## A Bash Function Ending in `cond && action` Aborts a `set -e` Script (2026-09-19)

**What happened:** `no-preexisting-failures.sh`'s `check()` was rewritten to end in `((all_exempt == 0)) &&
MATCHED="$label"`. `check()` is called at top level, not inside an `if` or `while` condition, so under `set -e`
a false `&&` list as the LAST statement makes the function itself return non-zero and aborts the sourced
script: every excuse pattern after the first was silently skipped.
**Prevention:** never end a bash function with a bare `cond && action` or `cond || action`; when the function
is called from an unguarded context, wrap the tail in `if ...; then ...; fi` so a false condition still returns 0.
**Fix:** `check()` and `_line_is_exempt` now end in `if`/`fi`.

## Hook Tests Inherit the Live Session's Environment (2026-09-19)

A hook test run from a stage session inherits `LOOM_WORK_DIR`, `LOOM_SESSION_ID`, `LOOM_STAGE_ID` and
`LOOM_HOOK_PATH`, and each one silently changes what the hook does:

- a hook that writes an in-stage ledger targets the live `.loom/work` (read-only in the stage sandbox), the
  write fails, and a test that passed standalone fails under `run-all` (`prefer-modern-tools-warn-once.sh`).
  Every ledger-exercising hook test unsets `LOOM_WORK_DIR LOOM_SESSION_ID LOOM_STAGE_ID` or points them at a
  temp dir. `hooks_skill_trigger.rs::run_hook` does this with `env_remove` and a per-`FakeHome` `TMPDIR`;
  without it the hook resolved `_loom_ledger_file`'s in-stage branch and touched the live `.loom/work` of the
  session running the suite;
- `loom-hooks/_read_discipline.sh:12` sets `PATH="${LOOM_HOOK_PATH:-$PATH}"`, so a test that builds a PATH
  without `rg`/`fd` (`loom-hooks/tests/_path_without.sh`) must also unset `LOOM_HOOK_PATH`, or a live session's
  value splices the real PATH back in and the "tool not installed" branch never runs;
- a once-per-session ledger keyed on the fallback session id `t` was shared by every test in the process
  through the ambient `TMPDIR`, and a test that reused one `FakeHome` for four `run_hook` calls saw seeds 1-3
  filtered to empty output by the dedupe. Any test that calls the hook more than once with an overlapping skill
  set uses a fresh `FakeHome` per call or expects the dedupe;
- the Rust twin: `policy_tests_stage_gate.rs::skip_reason` checked only the test process's env and
  `/proc/1/comm`, so inside a stage shell it skipped but under `loom stage complete` (env cleared, an ancestor
  pid still carrying `LOOM_*`) it ran and failed, because `codex-forward-guard.sh` classifies the session from
  ANCESTOR environ. A "no stage evidence" test skips on every evidence source the guard itself uses
  (`ancestor_stage_evidence_reason`, commit `bd901ddf`).

## Reads-Ledger Directories: Mode 0700 on Every Level, and the Owner Can Always `chmod` (2026-09-19)

The out-of-stage reads ledger root `$TMPDIR/loom-reads` is shared with the Rust receipt store, which rejects it
unless `mode & 077 == 0` (`loom/src/context/read_receipts.rs:369`). When the shell ledger layout made it an
intermediate directory, `mkdir -p -m 700 <leaf>` would have created it with umask permissions and silently
broken out-of-stage receipts, so `_loom_ledger_append` creates the parent 0700 too
(`loom-hooks/_read_ledger.sh:106`). A test that `chmod 0500`s a ledger directory to make it unwritable fails
for the same reason the design works: `_loom_ledger_append` runs `chmod 700` on its own directory and an owner
can always `chmod`. To simulate an unwritable ledger, make the parent of a not-yet-created directory read-only or
pre-create the ledger FILE read-only (`read-guard-session-ledgers.sh` case 6). The sibling-read scan is one
POSIX awk pass over at most 20 ledgers instead of an `rg -F` prefilter.

## A PreToolUse `updatedInput` Is Discarded Without a `permissionDecision` (2026-09-19)

Claude Code silently discards a PreToolUse `updatedInput` that carries no `permissionDecision`
(anthropics/claude-agent-sdk-python#381). `spawn-guard.sh` emitted the worker-brief-only rewrite without one,
so that brief never reached the subagent. Every hook output carrying `updatedInput` also sets
`permissionDecision: "allow"` (`loom-hooks/spawn-guard.sh:335`). Run a hook by its shebang, never `bash <hook>`
without reading line 1: `skill-trigger.sh` is Python, and bash runs its `import os` lines as ImageMagick's
`import`, which tries to screenshot the X display and, with a display, writes files named `os`, `json` and `re`
into the working directory.

## The Repo Pre-Commit Markdown Lint Silently Never Runs Inside a Stage (2026-09-19)

The pre-commit hook's "Linting markdown files" step fetches from `registry.npmjs.org`. In a stage there is no
network, the sandbox denies the fetch, and the commit still succeeds, so markdown lint never runs for `.md`
commits made in a stage. Detection: a `<sandbox_violations>` deny for `registry.npmjs.org` right after
`git commit`. Run the markdown lint from a networked session before merge.

## zsh Reads `$VAR:path` as a Modifier, and `>` Truncates Before the Command Fails (2026-09-19)

**What happened:** `git show "$T2:loom/maintainability-baseline.txt" > loom/maintainability-baseline.txt` in a zsh
session: zsh read `:l` as the lowercase modifier, `git show` failed, and the `>` redirect had already truncated
the tracked file (restored immediately from `git show HEAD:`).
**Prevention:** brace a variable before a colon (`"${T2}:path"`), and never redirect onto a tracked file in a
command that can fail; write to a scratch file and `mv` on success.
