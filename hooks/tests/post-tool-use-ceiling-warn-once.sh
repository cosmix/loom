#!/usr/bin/env bash
# The 80%-of-ceiling warning is a once-per-session notice, not a repeat: two
# consecutive calls from the SAME session at 85% of the ceiling must warn on
# the first and stay silent (exit 0) on the second - otherwise the agent
# would be told to wrap up on every single tool call once it crosses 80%.
#
# A THIRD call from a DIFFERENT session (same stage, same LOOM_WORK_DIR) must
# warn again. This pins the fix this stage makes: the warn marker moves from
# being keyed on LOOM_STAGE_ID to LOOM_SESSION_ID so it stops leaking across
# a stage's successor sessions - a stage that hands off at 85% and resumes in
# a fresh session must not inherit a silenced warning it never itself
# triggered.
set -euo pipefail
HOOK="$(dirname "$0")/../post-tool-use.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

WORKDIR="$TMP/work"
mkdir -p "$WORKDIR/heartbeat"
printf '%s\n' '1000:120000' >"$WORKDIR/heartbeat/test-stage.session-a.context-ceilings"
printf '%s\n' '1000:120000' >"$WORKDIR/heartbeat/test-stage.session-b.context-ceilings"

TRANSCRIPT="$TMP/transcript.jsonl"
{
	printf '%s\n' '{"type":"user","message":{"content":"dummy first line, dropped by design"}}'
	printf '%s\n' '{"type":"assistant","message":{"usage":{"input_tokens":850,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}'
} >"$TRANSCRIPT"

INPUT=$(jq -nc --arg tp "$TRANSCRIPT" '{tool_name:"Bash",tool_input:{command:"echo hi"},transcript_path:$tp}')

invoke() {
	local session_id="$1"
	set +e
	STDERR_OUT=$(printf '%s' "$INPUT" |
		env -u LOOM_MAIN_AGENT_PID -u LOOM_WORKTREE_PATH \
			LOOM_STAGE_ID="test-stage" LOOM_SESSION_ID="$session_id" LOOM_WORK_DIR="$WORKDIR" \
			bash "$HOOK" 2>&1 1>/dev/null)
	CODE=$?
	set -e
}

invoke "session-a"
if [[ $CODE -ne 2 ]]; then
	echo "FAIL: first call at 85% - expected exit 2 (warning), got $CODE (stderr: $STDERR_OUT)"
	exit 1
fi
if [[ "$STDERR_OUT" != *"850/1000"*"80%"* ]]; then
	echo "FAIL: first call - expected the 80% warning naming 850/1000, got: $STDERR_OUT"
	exit 1
fi

invoke "session-a"
if [[ $CODE -ne 0 ]]; then
	echo "FAIL: second call, same session - expected exit 0 (already warned), got $CODE (stderr: $STDERR_OUT)"
	exit 1
fi
if [[ -n "$STDERR_OUT" ]]; then
	echo "FAIL: second call, same session - expected silence, got stderr: $STDERR_OUT"
	exit 1
fi

invoke "session-b"
if [[ $CODE -ne 2 ]]; then
	echo "FAIL: third call, a NEW session of the same stage - expected exit 2 (its own warning), got $CODE (stderr: $STDERR_OUT)"
	exit 1
fi
if [[ "$STDERR_OUT" != *"850/1000"*"80%"* ]]; then
	echo "FAIL: third call - a new session must get its own 80% warning, got: $STDERR_OUT"
	exit 1
fi

echo "PASS"
