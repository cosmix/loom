# hooks-and-delivery / w1-hook-guards: blocking hooks fail closed, hook hygiene, confinement probe gate

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/security.md`, sections CR-05, CR-66, CR-67, CR-72; `doc/plans/briefs/review-remediation/validation/delivery.md`, sections I5 and CR-78 (bullet 3a only).

## Findings covered
- CR-66: blocking PreToolUse guards treat empty or unparseable stdin as "allow" (exit 0).
- CR-67: `loom-control-complete.sh` writes the completion heartbeat for a subagent caller.
- CR-72: `ask-user-pre.sh` and `ask-user-post.sh` put `LOOM_STAGE_ID` raw into JSON and into an `osascript` script.
- CR-05: `credential-guard.sh` rule (c) canonicalises the deny prefix with the wrong helper.
- CR-78 3a: `session-end.sh` and `pre-compact.sh` call `loom handoff` with no time bound.
- I5: `loom-hooks/tests/codex-forward-channel-confinement.sh` is registered nowhere and exits 1 when `srt` is absent.

## Files
Own (write), all under `loom-hooks/`:
- `_bounded.sh`
- `codex-apply-patch.sh`, `commit-filter.sh`, `git-add-guard.sh`, `worktree-isolation.sh`, `worktree-file-guard.sh`, `plans-path-guard.sh`, `credential-guard.sh`, `loom-relay-guard.sh`, `loom-control-complete.sh`, `loom-control-subagents.sh`
- `ask-user-pre.sh`, `ask-user-post.sh`, `session-end.sh`, `pre-compact.sh`
- `tests/blocking-guards-bad-payload.sh` (new), `tests/pre-compact-handoff-bounded.sh` (new), `tests/codex-forward-channel-confinement-gate.sh` (new)
- `tests/worktree-file-guard-truncated-input.sh`, `tests/loom-control-complete-heartbeat.sh`, `tests/ask-user-hooks.sh`, `tests/credential-guard-deny-read.sh`, `tests/session-end-reason.sh`, `tests/codex-forward-channel-confinement.sh`, `tests/run-all.sh`

Read only: `_common.sh`, `_identity.sh`, `_post-tool-heartbeat.sh`, `codex-forward-guard.sh`, `tests/_path_without.sh`, `tests/_fixture_base.sh`, `tests/subagent-review-real-binary-required.sh` (the pattern for the gate test).
Frozen (never edit): `loom/tests/hook_payload_guard_contracts.rs`, `loom/tests/hook_completion_heartbeat_contracts.rs`.
`run-all.sh` has one owner, you. No other worker adds a shell test there.

## Changes

### 1. CR-66 - the shared helper
Seam: `loom-hooks/_bounded.sh`, next to `loom_read_input` (:78-80). Add two functions:
- `loom_require_payload <hook-name>`: validates the global `INPUT_JSON`. It passes when `command -v jq` succeeds and `printf '%s' "$INPUT_JSON" | jq -e 'type == "object" and (.tool_name | type == "string" and length > 0)'` exits 0 (stdout and stderr to /dev/null). Otherwise: if `declare -F loom_record_block` finds the function, call `loom_record_block "<hook-name minus .sh>" bad-payload unknown ""`; print ONE line to stderr (`printf ... >&2 || true`, so a closed stderr cannot turn exit 2 into a signal death): `[<hook-name minus .sh>/bad-payload] LOOM: BLOCKED - tool call metadata could not be parsed (the hook input was empty or not a JSON object with a tool_name). Retry the call.`; `exit 2`.
- `loom_read_payload_or_block <seconds> <hook-name>`: `INPUT_JSON=$(loom_read_input "$1" 2>/dev/null || true)` then `loom_require_payload "$2"`.
Both are called as plain statements. Never `X=$(loom_read_payload_or_block ...)`: the `exit 2` would end only the command-substitution subshell and the hook would continue. Keep Bash 3.2 syntax.

Classification of every hook (registration: `loom/src/fs/permissions/hooks/config.rs` `PRE_TOOL_HOOKS`; Codex: `loom/src/fs/permissions/codex_hooks.rs` `desired_hooks`). Blocking guard = a PreToolUse hook that can exit 2 and already fails closed without jq (`loom_require_jq`/`fail_closed`). `_common.sh:23-46` documents the split.
- Blocking, gets the helper (10): `codex-apply-patch.sh` (pre mode only), `commit-filter.sh`, `git-add-guard.sh`, `worktree-isolation.sh`, `worktree-file-guard.sh`, `plans-path-guard.sh`, `credential-guard.sh`, `loom-relay-guard.sh`, `loom-control-complete.sh`, `loom-control-subagents.sh`.
- Already closed, leave untouched: `codex-forward-guard.sh` (`block_forwarder` at :94).
- Decides without the payload: `stage-terminal-guard.sh` (reads stdin only for a debug line; leave it).
- Advisory or deterrent by documented design (`loom_warn_no_jq`): `spawn-guard.sh`, `poll-guard.sh`, `read-guard.sh`, `subagent-verify-guard.sh`, `prefer-modern-tools.sh`, `no-preexisting-failures.sh`, `ask-user-pre.sh`, `ask-user-post.sh`. They keep failing open; `ask-user-hooks.sh` cases 3 and 6 pin that. Do not touch their read.
- Not PreToolUse: `commit-guard.sh` (Stop), every PostToolUse, SessionStart/End, PreCompact and prompt hook. Untouched.
The report named `commit-guard.sh` among the siblings; it is a Stop hook with no tool_name, so it stays out.

Per hook, replace the `INPUT_JSON=$(loom_read_input N 2>/dev/null || true)` line and keep N as the seconds argument:
- `commit-filter.sh` :46-47, `git-add-guard.sh` :64-65, `worktree-isolation.sh` :57-58, `plans-path-guard.sh` :28-29 (N=1): `loom_read_payload_or_block 1 <script>.sh`.
- `worktree-file-guard.sh` :16-35 and `credential-guard.sh` :45-64 (N=5): `loom_read_payload_or_block 5 <script>.sh`, then delete the `[[ -n "$INPUT_JSON" && -z "$TOOL_NAME" ]]` block and its heredoc (the helper replaces it). Keep the `TOOL_NAME=` jq line. The helper's first stderr line carries the `[worktree-file-guard/bad-payload]` tag and ledger row that `tests/worktree-file-guard-harness-output.sh` :128-136 pins; keep that test green.
- `codex-apply-patch.sh` :24-28: after the existing jq check, in `pre` mode call `loom_read_payload_or_block 5 codex-apply-patch.sh`; in `post` mode keep `INPUT_JSON=$(loom_read_input 5 2>/dev/null || true)` (PostToolUse fails open).
- `loom-relay-guard.sh` :37-42: keep the early `LOOM_SESSION_ID`/`LOOM_SCRATCH_DIR` gate (:32) and the existing read and jq check as they are; add `loom_require_payload loom-relay-guard.sh` right after the jq block (before the `FIELDS=$(...)` line). Do not use the read form here: the jq-missing path must keep reading stdin first. The earlier stage `confinement-boundaries` has already added a PreToolUse marker write to this script: read the file as it is now, keep that write and its position untouched, and report (do not guess) if it sits where `loom_require_payload` belongs.
- `loom-control-complete.sh` :18-21, :314-321 and `loom-control-subagents.sh` :23-26, :355-362: delete the `read_input()` function and put `source "$(dirname "${BASH_SOURCE[0]}")/_bounded.sh"` at top level after the `_common.sh` source. Move the four lines that set `STAGE_ID`, `SESSION_ID`, `WORKTREE_PATH` and gate on `[[ -n "$STAGE_ID" && -n "$SESSION_ID" ]] || exit 0` above the payload read (they do not depend on it; this keeps a non-loom session unaffected, as in the relay guard). Replace `INPUT_JSON=$(read_input)` and the `[[ -n "$INPUT_JSON" ]] || exit 0` line with `loom_read_payload_or_block 1 <script>.sh`. Keep the `tool_name == Bash` line after it. Both files are 383 and 391 lines: the change must not grow either.

### 2. CR-67 - guard the heartbeat write
`loom-control-complete.sh` :372, in the `COMMAND == PINNED_COMMAND` branch. Replace `loom_write_completion_heartbeat || true` with `loom_is_subagent "$INPUT_JSON" || loom_write_completion_heartbeat || true`. A caller whose verdict is unknown with a transcript path may be the main agent: it keeps the heartbeat, so do NOT delete the line. `loom_is_subagent` (`_identity.sh:343`) returns false outside a live loom session (`LOOM_MAIN_AGENT_PID` must name a live ancestor), so the new test must set it.

### 3. CR-72 - ask-user hooks
In both `ask-user-pre.sh` and `ask-user-post.sh`, directly after the `:28` empty check add `case "$LOOM_STAGE_ID$LOOM_SESSION_ID" in *[!A-Za-z0-9_-]*) exit 0 ;; esac` (class from `validate_id`, `loom/src/validation.rs:74-76`; not the report's `._-`). In `ask-user-pre.sh` replace the `osascript -e "display notification \"$MESSAGE\" ..."` call (:121) with `osascript -e 'on run argv' -e 'display notification (item 1 of argv) with title "loom"' -e 'end run' "$MESSAGE" 2>/dev/null`.

### 4. CR-05 - canonical deny prefix
`credential-guard.sh` :575. Replace `canonical_existing "$SPLIT_PREFIX"` with `canonical_target "$SPLIT_PREFIX"` (defined :111; it walks up to the nearest existing ancestor). Keep the raw-prefix fallback on the next line.

### 5. CR-78 3a - bounded handoff
`session-end.sh` :68 and `pre-compact.sh` :77 and :105: prefix each `loom handoff` with `loom_run_bounded 30`, keeping `LOOM_HOOK_CONTEXT=1` as an env prefix before it (`LOOM_HOOK_CONTEXT=1 loom_run_bounded 30 "${LOOM_BIN:-loom}" handoff ...`, as `pre-compact.sh:40` does). Redirections, `|| true` and the `if HANDOFF_OUTPUT=$(...)` shape stay. Both scripts already source `_bounded.sh`.

### 6. I5 - probe gate and roster
- `tests/codex-forward-channel-confinement.sh`: keep the non-Linux SKIP. Replace the `exit 1` on a missing prerequisite (:9-11) with `SKIP: missing runtime prerequisite: <tool>` and `exit 0`, unless `LOOM_HOOK_TEST_REQUIRE_SRT=1`, where it prints `FAIL:` and exits 1. Add a pre-flight right after the settings file is written: run `"$runtime" --settings "$d/settings.json" -c true`; if it fails, the same SKIP/FAIL rule applies with reason `srt cannot start a sandbox here`. This is needed because `srt` can be on PATH and still unable to run (inside another sandbox it exits 1: "Could not find an existing 'sandbox-runtime' binary"), which would make the registered test red. Header comment: state the variable and that no CI job sets it.
- New `tests/codex-forward-channel-confinement-gate.sh`, modelled on `subagent-review-real-binary-required.sh`: on Linux run the probe with `LOOM_TEST_SRT_BIN=/nonexistent/srt` and assert exit 0 with a `SKIP:` line; run again with `LOOM_HOOK_TEST_REQUIRE_SRT=1` and assert exit 1 with `FAIL:`. On other systems print `SKIP:` and exit 0.
- `run-all.sh`: register the probe after the `codex-forward-*` entries (:197-202 region) and the gate test next to it. Before `START=$SECONDS`, add a roster guard: for each `$SCRIPT_DIR/*.sh` not named `_*` and not `run-all.sh`, fail with `run-all.sh: <name> is not registered` and `exit 1` when its path is absent from `SCRIPTS` (Bash 3.2: test `" ${SCRIPTS[*]} "` against `*" $path "*`). Register the other two new tests too. Rename the entry at :165 to `worktree-file-guard: empty and malformed input are blocked`.

## Interfaces shared with other workers
`loom_require_payload <hook-name>` and `loom_read_payload_or_block <seconds> <hook-name>` in `loom-hooks/_bounded.sh`: exit 2 with one stderr line on empty, non-object or tool_name-less input; set the global `INPUT_JSON`. The contract tests call the hook scripts, not the helpers. No other worker uses them.

## Tests
- `tests/blocking-guards-bad-payload.sh` (new): for each of the 10 blocking hooks (`codex-apply-patch.sh` gets argument `pre`) run from a scratch directory with `env -i` plus `PATH`, `HOME` (temp), `LOOM_SESSION_ID`, `LOOM_SCRATCH_DIR`, `LOOM_STAGE_ID` set: stdin empty, stdin `{"tool_name":"Ba`, stdin `{}` each exit 2 with a one-line `bad-payload` message on stderr; stdin `{"tool_name":"NoSuchTool"}` exits 0 (control). Then `prefer-modern-tools.sh` and `no-preexisting-failures.sh` on empty stdin exit 0. Fails today for every blocking hook on empty input.
- `tests/worktree-file-guard-truncated-input.sh` (existing, assertions change): lines 6-9 now expect exit 2 for empty input; the malformed case and its `metadata could not be parsed` text stay (the helper message contains that phrase).
- `tests/loom-control-complete-heartbeat.sh` (extend): add `run_pre_as` that takes an extra jq payload fragment and extra env. Case A: payload with `agent_type: loom-software-engineer`, pinned command, `LOOM_MAIN_AGENT_PID=$$`: exit 0, and `$HEARTBEAT` is absent or lacks `Stage completion running`. Case B (control): same without `agent_type` and without the pid: marker present. Case A fails today.
- `tests/ask-user-hooks.sh` (extend; helper `run_hook` fixes `LOOM_STAGE_ID=stage-x`, so add a variant that takes the stage id): `LOOM_STAGE_ID='a"b'` for pre and post, once with jq and once with `LOOM_HOOK_PATH="$NOJQ_PATH"`: exit 0, no `events.jsonl` line, `loom.calls` empty. Add one pre case with `OSTYPE=darwin23` and a stub `osascript` that records its argv: no `-e` argument contains `needs your input`; the message is a separate argument. Fails today: a line that does not parse is written.
- `tests/credential-guard-deny-read.sh` (extend): add `ln -s "$TMPROOT/real" "$PROJECT/link"` (create `real/`), add `link/missing/secret.json` to the `denyRead` list in the existing settings JSON, then `check` a Write payload for `$PROJECT/link/missing/secret.json` expects 2 and one for `$PROJECT/link/other/secret.json` expects 0. Fails today (exit 0).
- `tests/session-end-reason.sh` (extend) and `tests/pre-compact-handoff-bounded.sh` (new): put shim scripts named `timeout` and `gtimeout` first on PATH; each appends its arguments to a log, then does `shift 3; exec "$@"` (the call is `-k 1 <secs> cmd...`). Assert the log holds `-k 1 30` followed by `handoff` for session-end's one call, and for both pre-compact attempts (run the hook twice with the same `LOOM_SESSION_ID`: first creates `compaction-pending/<id>`, second removes it; needs `LOOM_WORK_DIR`, `LOOM_STAGE_ID`, `LOOM_SESSION_ID`, a stub `LOOM_BIN`). Fails today: no bounded call.
- Contracts to turn green: `blocking_guards_refuse_empty_and_truncated_input`, `advisory_hook_still_allows_empty_input` (file `loom/tests/hook_payload_guard_contracts.rs`), `non_main_caller_writes_no_completion_heartbeat` (file `loom/tests/hook_completion_heartbeat_contracts.rs`).

## Proof
`bash -n` on each edited script; then run each edited or new script alone: `bash loom-hooks/tests/<name>.sh`, plus `loom-hooks/tests/worktree-file-guard-harness-output.sh`, `loom-control-complete.sh`, `loom-control-subagents.sh`, `loom-relay-guard.sh`, `codex-apply-patch.sh`, `require-jq-blocking.sh`. Finish with `bash loom-hooks/tests/run-all.sh` (about 35 s, hermetic) and, from `loom/`, `cargo test --test integration hooks_` (the commit-filter, git-add-guard and read-guard suites run these scripts).

## Traps
- `X=$(loom_read_payload_or_block ...)` hides the exit 2 (see item 1). The tests catch it only if they check the exit code of the hook, not its stderr.
- `loom-relay-guard.sh` must not read stdin after its jq check fails; use `loom_require_payload` there, not the read form.
- A Case A heartbeat test without `LOOM_MAIN_AGENT_PID` passes for the wrong reason before and after (no live session means `loom_is_subagent` is false).
- Do not make advisory hooks fail closed: `ask-user-hooks.sh` cases 3 and 6 and `require-jq-advisory.sh` pin their behaviour.
- `loom-relay-guard.sh` was edited by the stage `confinement-boundaries` before yours: anchor by symbol, never by the line numbers above, and keep its marker write.
- `worktree-file-guard.sh` is 399 lines: the change shrinks it; keep it that way.
- `loom_run_bounded` returns 124 on timeout; inside `if HANDOFF_OUTPUT=$(...)` that reads as "no handoff", which is the intended fail-open.
- Report any existing test that turns red because it feeds an empty or tool_name-less payload to a blocking guard; do not edit it beyond `worktree-file-guard-truncated-input.sh`.
