#!/usr/bin/env bash
# At or above 100% of the resolved ceiling, the main-branch governor exits 2
# with "CONTEXT CEILING REACHED" on stderr - and unlike the 80% warning, this
# is not a once-per-session notice: it must fire on EVERY subsequent tool
# call for as long as the session stays at or above the ceiling, since the
# agent may ignore one message.
set -euo pipefail
HOOK="$(dirname "$0")/../post-tool-use.sh"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

WORKDIR="$TMP/work"
mkdir -p "$WORKDIR"
cat >"$WORKDIR/config.toml" <<'EOF'
[context]
ceiling_tokens = 1000
EOF

TRANSCRIPT="$TMP/transcript.jsonl"
{
	printf '%s\n' '{"type":"user","message":{"content":"dummy first line, dropped by design"}}'
	printf '%s\n' '{"type":"assistant","message":{"usage":{"input_tokens":1000,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}'
} >"$TRANSCRIPT"

INPUT=$(jq -nc --arg tp "$TRANSCRIPT" '{tool_name:"Bash",tool_input:{command:"echo hi"},transcript_path:$tp}')

invoke() {
	set +e
	STDERR_OUT=$(printf '%s' "$INPUT" |
		env -u LOOM_MAIN_AGENT_PID -u LOOM_WORKTREE_PATH \
			LOOM_STAGE_ID="test-stage" LOOM_SESSION_ID="test-session" LOOM_WORK_DIR="$WORKDIR" \
			bash "$HOOK" 2>&1 1>/dev/null)
	CODE=$?
	set -e
}

invoke
if [[ $CODE -ne 2 ]]; then
	echo "FAIL: first call at 100% of the ceiling - expected exit 2, got $CODE (stderr: $STDERR_OUT)"
	exit 1
fi
if [[ "$STDERR_OUT" != *"CONTEXT CEILING REACHED"* ]]; then
	echo "FAIL: first call - expected 'CONTEXT CEILING REACHED' on stderr, got: $STDERR_OUT"
	exit 1
fi
if [[ "$STDERR_OUT" != *"1000 >= 1000"* ]]; then
	echo "FAIL: first call - expected the resident/ceiling numbers '1000 >= 1000' on stderr, got: $STDERR_OUT"
	exit 1
fi

# A second call, same session, still at 100% - must fire again. This is what
# distinguishes the hard block from the once-per-session 80% warning.
invoke
if [[ $CODE -ne 2 ]]; then
	echo "FAIL: second call at 100% of the ceiling - expected exit 2 again, got $CODE (stderr: $STDERR_OUT)"
	exit 1
fi
if [[ "$STDERR_OUT" != *"CONTEXT CEILING REACHED"* ]]; then
	echo "FAIL: second call - the hard block must repeat every time, got: $STDERR_OUT"
	exit 1
fi

# A third call, to make sure this genuinely never latches shut.
invoke
if [[ $CODE -ne 2 ]]; then
	echo "FAIL: third call at 100% of the ceiling - expected exit 2 again, got $CODE (stderr: $STDERR_OUT)"
	exit 1
fi

echo "PASS"
