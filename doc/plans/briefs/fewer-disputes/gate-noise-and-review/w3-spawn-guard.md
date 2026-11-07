# gate-noise-and-review W3: the spawn guard appends the reviewer brief

Tier: sonnet (`loom-software-engineer`). Wave 2. Read
`doc/plans/briefs/fewer-disputes/gate-noise-and-review/common.md` first. The frozen contracts
`tests/reviewer_brief_guard_contracts.rs` drive the guard; read them with
`loom stage contracts show gate-noise-and-review` before you write code.

## Files owned

`../loom-hooks/_spawn_policy.sh`, `../loom-hooks/spawn-guard.sh`,
`../loom-hooks/tests/spawn-guard-reviewer-brief.sh` (new, executable) and
`../loom-hooks/tests/run-all.sh` (a static list, CONTRIBUTING.md:151: add one `run_test` line for
the new script, next to the `spawn-guard-reviewer-model.sh` line).

## Seams (locate by symbol)

- `_spawn_policy.sh::loom_worker_brief` (envelope validation in a `jq` filter; the 5 s bound is
  `loom_run_bounded 5`; fail open with `loom_debug`). `loom_reviewer_model_override` in the same
  file is the other bounded `loom hook` call.
- `spawn-guard.sh`: the `GATE_PASSED` computation, step 4b (`AGENT_TYPE_REQ == loom-code-reviewer`),
  the block after `loom_worker_brief` that appends `WORKER_BRIEF_JSON` to the prompt with `jq`,
  `emit_result`. Both hook files are embedded in the binary
  (`src/fs/permissions/constants.rs`, `HOOK_SPAWN_GUARD`, `HOOK_SPAWN_POLICY`); no new hook file
  is added, so no registration changes.
- Test pattern: `../loom-hooks/tests/spawn-guard-reviewer-model.sh` (temp dir, a stub `LOOM_BIN`
  that answers `hook ...`, `spawn <type>` helper, `fail` helper, `env -i` run). Add what it lacks:
  the gate environment (`LOOM_STAGE_ID`, `LOOM_MAIN_AGENT_PID=$$`, `LOOM_WORK_DIR` set to a
  directory under the test's temp dir, never the inherited one (CONTRIBUTING.md: hook tests run
  inside stage sessions), `LOOM_SESSION_ID`) and a calls log. Do NOT copy its
  `unset LOOM_STAGE_ID ...` line; the `env -i` run already drops the inherited values.
- The frozen guard contract's prompt starts with the first line of `_subagent-preamble.txt`, so
  the guard adds no preamble; a prompt without that line gets the preamble and a blank line
  prepended first (`spawn-guard.sh`), and the briefs follow the prompt either way.
- The Rust spawn-guard cases in `tests/integration/hooks_spawn_guard_gate.rs` (stub
  `install_worker_brief_stub`) must stay green; they only stub `worker-brief`.

The frozen guard contract covers only the append. The floor checks (other agent types never call
the hook, bad envelopes, failing and slow stubs fail open) live in the shell test, because they
already hold at the base commit and a frozen contract must fail before the work.

## Behaviour (decided)

1. New function `loom_reviewer_brief TOOL_INPUT GATE_PASSED AGENT_TYPE_REQ` in `_spawn_policy.sh`
   sets `LOOM_SPAWN_REVIEWER_BRIEF_JSON` (empty when none). It runs only when `GATE_PASSED` is 1,
   `AGENT_TYPE_REQ` is exactly `loom-code-reviewer`, the binary exists, and `TOOL_INPUT` passes the
   same `jq -e 'type == "object" and (.prompt | type == "string")'` check `loom_worker_brief`
   makes (a non-string prompt cannot take the append). The call is
   `loom_run_bounded 5 "${LOOM_BIN:-loom}" hook reviewer-brief </dev/null 2>/dev/null`: stdin is
   `/dev/null` because the command reads none, and piping a payload into a command that never reads
   it dies of SIGPIPE under `pipefail`.
2. The envelope validation of `loom_worker_brief` moves into one shared function
   `_loom_brief_json ENVELOPE MARKER` (marker `loom-worker-brief` or `loom-reviewer-brief`): a
   single line, an object whose keys are exactly `brief` and `nonce`, a 32-hex nonce, a non-empty
   string brief that holds exactly one line equal to `<!-- MARKER nonce=NONCE -->`. Both callers
   use it; do not leave two copies of the filter.
3. In `spawn-guard.sh` the reviewer brief is appended AFTER the worker brief, each as
   `prompt + "\n\n" + brief`, and `PROMPT_REWRITTEN=1`. Replace the existing worker-brief append
   block with one shared helper call per brief so `spawn-guard.sh` does not grow: the file is at
   exactly 400 lines now and must end at or below 400. Move helper code into `_spawn_policy.sh`.
4. Every failure (missing binary, timeout, non-zero exit, empty output, bad envelope) changes
   nothing and calls `loom_debug`; the spawn proceeds. The guard never denies a spawn because of the
   reviewer brief.
5. Bash 3.2 compatible (no `mapfile`, no `${var,,}`, no `[[ -v ]]`); keep `shellcheck`-clean
   quoting.

## Steps

1. Refactor `loom_worker_brief` onto `_loom_brief_json` with identical behaviour (the existing
   Rust guard tests prove it).
2. Add `loom_reviewer_brief` and the shared append helper; wire both calls in `spawn-guard.sh`.
3. Write `spawn-guard-reviewer-brief.sh` with these cases, each a numbered `fail` message:
   - valid envelope on a `loom-code-reviewer` spawn: `updatedInput.prompt` ends with the brief,
     exactly one `loom-reviewer-brief` marker line;
   - worker brief and reviewer brief together: worker first, reviewer last, both once;
   - `loom-software-engineer` and `Explore` spawns: the stub's calls log has no `hook
     reviewer-brief`, prompt has no reviewer marker;
   - a `loom-code-reviewer` payload whose `prompt` is not a string: no `hook reviewer-brief` call,
     the spawn is still allowed;
   - no gate environment: no call at all;
   - bad envelopes (marker nonce differs from `nonce`, an extra key, two lines, nonce not 32 hex,
     empty brief, not JSON): prompt unchanged, output still allows the spawn;
   - stub exits 1: unchanged; stub sleeps 7 s: unchanged within about 6 s (one slow case);
   - the stub records `cat` of its stdin to a file: the file is empty.
4. Add to `../loom-hooks/tests/run-all.sh` a line `run_test "spawn-guard: reviewer brief appended once, fails open" "$SCRIPT_DIR/spawn-guard-reviewer-brief.sh"` in the style of its neighbours.
5. Run once: `bash ../loom-hooks/tests/spawn-guard-reviewer-brief.sh` from `loom/`, then
   `bash ../loom-hooks/tests/spawn-guard-reviewer-model.sh`.

## Traps

- `jq -r` of `updatedInput.prompt` strips nothing, but `$(...)` strips trailing newlines; compare
  with `printf '%s'` and a sentinel, as the existing worker-brief tests do.
- `LOOM_MAIN_AGENT_PID` must be a live ancestor of the hook: use `$$` of the test script and run the
  hook as its child.
- Do not source anything new from `spawn-guard.sh`; `_spawn_policy.sh` is already sourced.
- The 5 s bound is the plan's decided value; do not raise it.
