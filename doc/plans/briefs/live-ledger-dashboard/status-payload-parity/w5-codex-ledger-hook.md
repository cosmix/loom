# W5 — Record the codex model that ran each forwarded task

Tier: Claude `loom-software-engineer` (sonnet). Runs in the first wave, in parallel with W1 and W4.

## Files you own (write)

- `hooks/codex-forward.sh` — edit with the Edit tool ONLY
- `hooks/tests/codex-forward-records-model.sh` — new, create with the Write tool ONLY

Read-only: `hooks/spawn-guard.sh` (function `record_spawn`, the pattern to mirror),
`hooks/tests/codex-forward-wrapper.sh` (how the wrapper is exercised with stubbed `node`,
`sandbox-exec` and `codex`).

## Why the tool restriction

Claude Code's sandbox write-protects the project-root `hooks/` directory for every SHELL write
(`sed`, redirection, `cp`, `chmod` all fail with "Operation not permitted"). The Edit and Write
tools are not subject to that rule. Run the test with `bash hooks/tests/<name>.sh`; that only reads.

## What to build

### `hooks/codex-forward.sh`

After the effort validation (the `case "$effort"` block) and before the preamble is assembled, add
and call `record_codex_spawn`, modelled on `record_spawn` in `hooks/spawn-guard.sh`:

- Requires `LOOM_WORK_DIR` and `LOOM_STAGE_ID`; return 0 silently when either is empty.
- Reject a `LOOM_STAGE_ID` containing anything outside `[A-Za-z0-9._-]`.
- Directory `${LOOM_WORK_DIR}/subagents/${LOOM_STAGE_ID}` created `mkdir -p -m 700`, `chmod 700`.
- File `codex.jsonl`; refuse to write when the path is a symlink.
- One line built with `jq -nc --arg ...`:
  `{ts, stage_id, session_id, model, effort, mode}` where `ts` is UTC ISO-8601 with `.000Z`,
  `session_id` is `${LOOM_SESSION_ID:-}`, `model`/`effort` are the validated arguments, and `mode`
  is the literal `forward`.
- Append with `printf '%s\n' >>`, `chmod 600`, every step best-effort (`|| true`), never changes
  the wrapper's exit status, never prints to stdout (stdout is the evidence channel the
  orchestrator parses).
- Call it BEFORE codex runs so the ledger records the attempt even when codex fails.

### `hooks/tests/codex-forward-records-model.sh`

Mirror the setup of `codex-forward-wrapper.sh` lines 4-24 (temp dir, stubbed `node` and
`sandbox-exec` on `PATH`, the companion fixture). Then:

1. Run the wrapper with `LOOM_WORK_DIR="$TMP/work" LOOM_STAGE_ID=stage-a LOOM_SESSION_ID=s1` and
   `--model gpt-5.6-terra --effort xhigh --write`; assert `$TMP/work/subagents/stage-a/codex.jsonl`
   exists, has exactly one line, and that line contains `"model":"gpt-5.6-terra"`,
   `"stage_id":"stage-a"`, `"mode":"forward"`.
2. Run again with the same env; assert two lines (append, not overwrite).
3. Run with `LOOM_WORK_DIR` unset; assert no `subagents` directory appears under `$TMP`.
4. Run with `LOOM_STAGE_ID='../escape'`; assert nothing was written under `$TMP/work/subagents`
   and no `escape` directory exists under `$TMP`.
5. Assert the wrapper's stdout still carries `--- LOOM-CODEX-EVIDENCE ---` in case 1.

Print `PASS` on success; exit 1 with a `FAIL: ...` line otherwise, matching the sibling tests.

## Done means

`bash hooks/tests/codex-forward-records-model.sh` prints `PASS`, and
`bash hooks/tests/codex-forward-wrapper.sh` still prints `PASS` (it is the regression pin for the
wrapper's argv and evidence contract).

## Constraints the graph will not show you

- `hooks/codex-forward.sh` is installed to `~/.claude/hooks/loom/` by `loom install-assets`; the
  repository copy is the source of truth and no install step belongs to this task.
- Never read `.work/admin.token` or `.work/user.token`.
- Do not run `git`. Do not run the Rust test suite, the linter or the formatter.
