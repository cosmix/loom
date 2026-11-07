# Worker B: Completion Rewrite (W1)

Tier: `loom-senior-software-engineer`, effort medium. Wave 2, after worker F. Read `common.md` first.

## Files you own

- `loom-hooks/loom-control-complete.sh`
- `loom-hooks/tests/loom-control-complete.sh`
- `loom-hooks/tests/loom-control-complete-heartbeat.sh`
- `loom-hooks/tests/loom-control-complete-knowledge.sh`
- `loom-hooks/tests/loom-control-complete-rewrite.sh` (new)

## Read first

- `loom-hooks/loom-control-complete.sh` whole (296 lines). It sources `_common.sh` unguarded at line 13, so `loom_record_block`, `loom_tokenize_command` and `loom_payload_agent_verdict` are available. Decision order: `is_completion_command` (208-218), `completion_rejection_reason` (220-239), main flow (241-295). Trusted binary: `resolve_trusted_loom` (39-57). Names in the file: `PINNED_COMMAND`, `LOOM_BIN`, `STAGE_ID`. Its test mode: `LOOM_CONTROL_TESTING=1` with `LOOM_CONTROL_TEST_BIN`. The exact pinned command exits 0 with empty stdout and writes `heartbeat/<stage>.json` (observed).
- `loom-hooks/spawn-guard.sh:328-343` (`emit_result`): the PreToolUse response shape with `permissionDecision` and `updatedInput`, built from the input with jq. Its header (`:37-39`) says Claude Code discards an `updatedInput` without a `permissionDecision`; that is out of date for Claude Code 2.1.319, which applies it, but send `permissionDecision` anyway.
- `loom-hooks/tests/loom-control-complete.sh:69-72` (`INVALID_COMMANDS`, whose first entry is the bare form) and `:100-130` (line 115-117 asserts the bare command is rejected with `retry with the pinned command`).
- `loom-hooks/tests/loom-control-complete-heartbeat.sh:72-78` and `loom-hooks/tests/loom-control-complete-knowledge.sh:46-52, 69-73`: both use the bare form as their rejected example.

## Claude Code facts (2.1.319 bundle, read by two reviewers)

- `updatedInput` REPLACES the whole tool input. An `updatedInput` of `{command}` alone drops the model's `timeout`, `description` and `run_in_background`; the default Bash timeout is 120,000 ms, and a completion call waits at most about 100 s (`loom/src/host_verify/wait.rs:124-131`).
- Every matching PreToolUse hook gets the model's original input and they run concurrently; none sees the rewrite. A deny (exit 2) from any hook wins over an allow, and a deny permission rule still overrides a hook allow, so `commit-filter.sh`'s subagent `loom stage complete` deny (`:308-325`) still applies to the original command.
- `additionalContext` is delivered alongside `permissionDecision: "allow"`.

## Behavior

The rewrite applies only after every existing gate in the main flow has passed (stage and session identity, checkout root, trusted binary, the PreToolUse-only check at 287), and only when `loom_payload_agent_verdict "$INPUT_JSON"` does not print `subagent` (a subagent never completes a stage; its bare form keeps today's rejection).

Match the tokens exactly (`loom_tokenize_command` output, observed at HEAD):

1. `BIN stage complete ID`, where BIN is the word `loom` or the word equal to the value of the shell variable `LOOM_BIN` (the resolved trusted path, never the literal text `$LOOM_BIN`), and ID is exactly `$STAGE_ID`;
2. then optionally the three items `2`, a sentinel whose SEPS is exactly `>&`, `1` (`2>&1` and `2 >&1` tokenize the same; `2>1` gives SEPS `>` and does not match);
3. then optionally a sentinel whose SEPS is exactly `|`, the word `tail` or `head`, and either one word `-N` or the two words `-n N`, N all digits;
4. then the end of the token stream.

Nothing else matches: no env prefix, assignment, extra flag, second pipe, redirection to a file, separator or second command. Also refuse the rewrite when the raw `COMMAND` contains `\`, `'`, `"`, `$`, a backtick or a newline.

On a match, write `loom_write_completion_heartbeat || true` (as the exact-pinned path does at line 295), then print to stdout and exit 0 the object built with jq from the payload:

```bash
jq -c --arg pinned "$PINNED_COMMAND" '{hookSpecificOutput: {
  hookEventName: "PreToolUse",
  permissionDecision: "allow",
  permissionDecisionReason: "loom: completion rewritten to the pinned command",
  updatedInput: ({command: $pinned}
    + (.tool_input | {timeout, description} | with_entries(select(.value != null)))),
  additionalContext: ("LOOM: ran the pinned completion command " + $pinned + "; its output is not piped. Read the Host job line.")}}' <<<"$INPUT_JSON"
```

Carry only `timeout` and `description` over: a model-supplied `run_in_background` or `dangerouslyDisableSandbox` never rides on an explicit hook allow. `updatedInput.command` is byte-identical to `$PINNED_COMMAND`.

Every other completion-shaped command keeps today's `fail_closed` path and reason text. Put `[loom-control-complete/not-pinned]` at the start of the `LOOM_CONTROL_ERROR` line, and call `loom_record_block loom-control-complete not-pinned Bash "$COMMAND"` before `fail_closed`.

## Existing tests that change

- `loom-control-complete.sh:71`: drop `'loom stage complete build-api'` from `INVALID_COMMANDS`, and change the 115-117 case to expect the rewrite (exit 0, `jq -r .hookSpecificOutput.updatedInput.command` equal to `$PINNED`).
- `loom-control-complete-heartbeat.sh:74-78`: keep the "a rejected attempt leaves the heartbeat alone" case, probing it with `loom stage complete build-api; true` instead of the bare form. Add a case that the bare form, now rewritten, writes the heartbeat like the pinned form.
- `loom-control-complete-knowledge.sh:48-51` and `:70-72`: the bare form in a Knowledge session and in a stage session now exits 0 with `updatedInput.command` equal to `$PINNED`; assert that. Keep the "LOOM_BIN inside the worktree is ignored" probe by asserting the rewrite names `$PINNED`, never `$WORKTREE/bin/loom`.

## New test: `loom-control-complete-rewrite.sh`

- Rewrites (exit 0; stdout JSON whose `updatedInput.command` equals the pinned command, compared with `jq -r` against the exact string):
  - `loom stage complete build-api`
  - `loom stage complete build-api 2>&1 | tail -30`
  - `loom stage complete build-api 2 >&1`
  - `loom stage complete build-api | tail -n 15`
  - `<trusted-bin> stage complete build-api 2>&1 | head -5`
  - a payload with `"timeout": 300000` and `"run_in_background": true`: `updatedInput.timeout` is 300000 and `updatedInput` has no `run_in_background`
- Still rejected (exit 2, today's reason, `[loom-control-complete/not-pinned]` on the error line):
  - `loom stage complete build-api; rm -rf .`
  - `loom stage complete build-api && echo ok`
  - `env FOO=1 loom stage complete build-api`
  - `FOO=1 loom stage complete build-api`
  - `loom stage complete other-stage`
  - `bash -c 'loom stage complete build-api'`
  - `/tmp/fake/loom stage complete build-api`
  - `loom stage complete build-api --force-unsafe`
  - `loom stage complete build-api | tail -30 | sh`
  - `loom stage complete build-api > out.txt`
  - `loom stage complete build-api 2>1`
  - `loom stage complete "build-api"`
  - `loom stage complete build-api | tail -$(id)`
  - `'$LOOM_BIN' stage complete build-api` (the literal text)
  - the bare form from a subagent payload (`agent_type` set)
- The exact pinned command still exits 0 with no stdout, as today.

## Done when

`bash loom-hooks/tests/loom-control-complete-rewrite.sh` passes, run once. The orchestrator runs the other completion suites (`-heartbeat`, `-heredoc`, `-knowledge`, and `loom-control-complete.sh`) at the gate; list every change you made to them.
