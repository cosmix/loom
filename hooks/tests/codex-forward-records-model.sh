#!/usr/bin/env bash
set -euo pipefail

# Run from the real hooks/ directory (not a copy) so codex-forward-guard.sh
# finds _common.sh beside it.
GUARD="$(cd "$(dirname "$0")/.." && pwd)/codex-forward-guard.sh"
if ! TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX") || [ -z "$TMP" ]; then
	printf '%s\n' 'FAIL: mktemp failed to create a scratch directory'
	exit 1
fi
trap 'rm -rf "$TMP"' EXIT

HOME_DIR="$TMP/home"
mkdir -p "$HOME_DIR"

VALID_CMD='~/.claude/hooks/loom/codex-forward.sh task hello --model gpt-5.6-terra --effort xhigh --write'
VALID_PAYLOAD=$(jq -nc --arg c "$VALID_CMD" \
	'{tool_name:"Bash",tool_input:{command:$c},agent_type:"loom-codex-forwarder"}')

# Case 1: a valid stage id records exactly one row with the expected fields.
WORK_DIR="$TMP/work"
LEDGER="$WORK_DIR/subagents/my-stage/codex.jsonl"

printf '%s' "$VALID_PAYLOAD" | HOME="$HOME_DIR" LOOM_WORK_DIR="$WORK_DIR" \
	LOOM_STAGE_ID="my-stage" LOOM_SESSION_ID="session-abc" \
	bash "$GUARD" >/dev/null

if [[ ! -f "$LEDGER" ]]; then
	printf '%s\n' "FAIL: ledger file was not created at $LEDGER"
	exit 1
fi
if [[ $(wc -l <"$LEDGER") -ne 1 ]]; then
	printf '%s\n' "FAIL: expected 1 ledger line, got $(wc -l <"$LEDGER")"
	exit 1
fi

model=$(jq -r '.model' "$LEDGER")
effort=$(jq -r '.effort' "$LEDGER")
stage_id=$(jq -r '.stage_id' "$LEDGER")
session_id=$(jq -r '.session_id' "$LEDGER")
ts=$(jq -r '.ts' "$LEDGER")

if [[ "$model" != 'gpt-5.6-terra' ]]; then
	printf '%s\n' "FAIL: expected model gpt-5.6-terra, got $model"
	exit 1
fi
if [[ "$effort" != 'xhigh' ]]; then
	printf '%s\n' "FAIL: expected effort xhigh, got $effort"
	exit 1
fi
if [[ "$stage_id" != 'my-stage' ]]; then
	printf '%s\n' "FAIL: expected stage_id my-stage, got $stage_id"
	exit 1
fi
if [[ "$session_id" != 'session-abc' ]]; then
	printf '%s\n' "FAIL: expected session_id session-abc, got $session_id"
	exit 1
fi
if [[ -z "$ts" || "$ts" == 'null' ]]; then
	printf '%s\n' "FAIL: expected a non-empty ts, got '$ts'"
	exit 1
fi

# Case 2: a second identical call appends a second line rather than truncating.
printf '%s' "$VALID_PAYLOAD" | HOME="$HOME_DIR" LOOM_WORK_DIR="$WORK_DIR" \
	LOOM_STAGE_ID="my-stage" LOOM_SESSION_ID="session-abc" \
	bash "$GUARD" >/dev/null

if [[ $(wc -l <"$LEDGER") -ne 2 ]]; then
	printf '%s\n' "FAIL: expected 2 ledger lines after second call, got $(wc -l <"$LEDGER")"
	exit 1
fi

# Case 3: an unsafe stage id must not create any ledger file or directory,
# even though the command itself is still authorized (exit 0).
status=0
printf '%s' "$VALID_PAYLOAD" | HOME="$HOME_DIR" LOOM_WORK_DIR="$WORK_DIR" \
	LOOM_STAGE_ID="bad/stage" LOOM_SESSION_ID="session-abc" \
	bash "$GUARD" >/dev/null || status=$?

if [[ "$status" -ne 0 ]]; then
	printf '%s\n' "FAIL: guard with unsafe stage id exited $status, expected 0"
	exit 1
fi
if [[ -e "$WORK_DIR/subagents/bad" || -e "$WORK_DIR/subagents/bad/stage" ]]; then
	printf '%s\n' 'FAIL: unsafe stage id left a ledger artifact under subagents/'
	exit 1
fi

# Case 4: an empty LOOM_WORK_DIR must not create a "subagents" directory,
# whether resolved as an absolute path or relative to the guard's own working
# directory. Run from a dedicated cwd so the second possibility has somewhere
# to be checked.
CASE4_CWD="$TMP/case4-cwd"
mkdir -p "$CASE4_CWD"
status=0
(
	cd "$CASE4_CWD"
	printf '%s' "$VALID_PAYLOAD" | HOME="$HOME_DIR" LOOM_WORK_DIR= \
		LOOM_STAGE_ID="my-stage" LOOM_SESSION_ID="session-abc" \
		bash "$GUARD" >/dev/null
) || status=$?

if [[ "$status" -ne 0 ]]; then
	printf '%s\n' "FAIL: guard with empty LOOM_WORK_DIR exited $status, expected 0"
	exit 1
fi
if [[ -e "$CASE4_CWD/subagents" ]]; then
	printf '%s\n' 'FAIL: empty LOOM_WORK_DIR created a subagents directory relative to cwd'
	exit 1
fi

# Case 5: LOOM_STAGE_ID=".." must not escape subagents/<stage_id> - it must
# neither append to work_dir/codex.jsonl (subagents/.. resolves to work_dir
# itself) nor create a subagents directory nor change the work dir's mode via
# the `mkdir -p -m 700`/`chmod 700` calls that target that resolved directory.
DOTDOT_WORK_DIR="$TMP/work-dotdot"
mkdir -m 750 "$DOTDOT_WORK_DIR"
mode_before=$(stat -c '%a' "$DOTDOT_WORK_DIR" 2>/dev/null || stat -f '%Lp' "$DOTDOT_WORK_DIR")

status=0
printf '%s' "$VALID_PAYLOAD" | HOME="$HOME_DIR" LOOM_WORK_DIR="$DOTDOT_WORK_DIR" \
	LOOM_STAGE_ID=".." LOOM_SESSION_ID="session-abc" \
	bash "$GUARD" >/dev/null || status=$?

if [[ "$status" -ne 0 ]]; then
	printf '%s\n' "FAIL: guard with LOOM_STAGE_ID=.. exited $status, expected 0"
	exit 1
fi
if [[ -e "$DOTDOT_WORK_DIR/codex.jsonl" ]]; then
	printf '%s\n' 'FAIL: LOOM_STAGE_ID=.. appended to work_dir/codex.jsonl'
	exit 1
fi
if [[ -e "$DOTDOT_WORK_DIR/subagents" ]]; then
	printf '%s\n' 'FAIL: LOOM_STAGE_ID=.. created a subagents directory'
	exit 1
fi
mode_after=$(stat -c '%a' "$DOTDOT_WORK_DIR" 2>/dev/null || stat -f '%Lp' "$DOTDOT_WORK_DIR")
if [[ "$mode_after" != "$mode_before" ]]; then
	printf '%s\n' "FAIL: LOOM_STAGE_ID=.. changed the work dir's mode from $mode_before to $mode_after"
	exit 1
fi

# Case 6: a command that FAILS validation is blocked (exit 2) and records
# nothing - only an AUTHORIZED forward may ever be recorded.
BLOCKED_WORK_DIR="$TMP/work-blocked"
BAD_CMD='~/.claude/hooks/loom/codex-forward.sh task hello --model gpt-4 --effort xhigh --write'
BAD_PAYLOAD=$(jq -nc --arg c "$BAD_CMD" \
	'{tool_name:"Bash",tool_input:{command:$c},agent_type:"loom-codex-forwarder"}')
status=0
printf '%s' "$BAD_PAYLOAD" | HOME="$HOME_DIR" LOOM_WORK_DIR="$BLOCKED_WORK_DIR" \
	LOOM_STAGE_ID="blocked-stage" LOOM_SESSION_ID="session-abc" \
	bash "$GUARD" >/dev/null 2>/dev/null || status=$?

if [[ "$status" -ne 2 ]]; then
	printf '%s\n' "FAIL: expected exit 2 for an unsupported model, got exit $status"
	exit 1
fi
if [[ -e "$BLOCKED_WORK_DIR" ]]; then
	printf '%s\n' 'FAIL: a blocked command created ledger artifacts'
	exit 1
fi

INJECT_CMD='~/.claude/hooks/loom/codex-forward.sh task hello --model gpt-5.6-terra --effort xhigh --write && echo hi'
INJECT_PAYLOAD=$(jq -nc --arg c "$INJECT_CMD" \
	'{tool_name:"Bash",tool_input:{command:$c},agent_type:"loom-codex-forwarder"}')
status=0
printf '%s' "$INJECT_PAYLOAD" | HOME="$HOME_DIR" LOOM_WORK_DIR="$BLOCKED_WORK_DIR" \
	LOOM_STAGE_ID="blocked-stage" LOOM_SESSION_ID="session-abc" \
	bash "$GUARD" >/dev/null 2>/dev/null || status=$?

if [[ "$status" -ne 2 ]]; then
	printf '%s\n' "FAIL: expected exit 2 for a trailing && echo hi, got exit $status"
	exit 1
fi
if [[ -e "$BLOCKED_WORK_DIR" ]]; then
	printf '%s\n' 'FAIL: a blocked command with a trailing operator created ledger artifacts'
	exit 1
fi

printf '%s\n' 'PASS'
