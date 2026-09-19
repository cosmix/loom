#!/usr/bin/env bash
# A Bash result whose stdout+stderr exceeds 20,000 characters gets one
# advisory notice pointing at a file + tail/rg instead of a raw re-read; the
# notice fires at most three times per session (ledger kind "bigout") and
# must not disturb the heartbeat write this hook also performs.
set -euo pipefail
HOOK="$(dirname "$0")/../post-tool-use.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

export LOOM_STAGE_ID="test-stage"
export LOOM_SESSION_ID="test-session"
export LOOM_WORK_DIR="$TMP"
# Never let a live loom stage's own LOOM_SESSION_TYPE leak in - this test
# always wants the ordinary (non-adjudication) heartbeat/notice path.
unset LOOM_SESSION_TYPE

BIG=$(printf 'x%.0s' $(seq 1 20050))
SMALL=$(printf 'y%.0s' $(seq 1 100))

run_hook() {
	local stdout_val="$1" input
	input=$(jq -nc --arg cmd 'cargo test' --arg out "$stdout_val" \
		'{tool_name:"Bash", tool_input:{command:$cmd}, tool_response:{stdout:$out}}')
	printf '%s' "$input" | bash "$HOOK" 2>&1 1>/dev/null || true
}

SMALL_OUT=$(run_hook "$SMALL")
if [[ -n "$SMALL_OUT" ]]; then
	echo "FAIL: small output should not trigger the notice, got: $SMALL_OUT"
	exit 1
fi

FIRST=$(run_hook "$BIG")
if [[ "$FIRST" != *"20050 characters"* || "$FIRST" != *"send verbose output to a file"* ]]; then
	echo "FAIL: first large-output call should warn with the size and guidance, got: $FIRST"
	exit 1
fi

SECOND=$(run_hook "$BIG")
if [[ "$SECOND" != *"send verbose output to a file"* ]]; then
	echo "FAIL: second large-output call (of 3 allowed) should still warn, got: $SECOND"
	exit 1
fi

THIRD=$(run_hook "$BIG")
if [[ "$THIRD" != *"send verbose output to a file"* ]]; then
	echo "FAIL: third large-output call (of 3 allowed) should still warn, got: $THIRD"
	exit 1
fi

FOURTH=$(run_hook "$BIG")
if [[ -n "$FOURTH" ]]; then
	echo "FAIL: a fourth large-output call in the same session should be silent, got: $FOURTH"
	exit 1
fi

HEARTBEAT="$TMP/heartbeat/test-stage.json"
if [[ ! -f "$HEARTBEAT" ]]; then
	echo "FAIL: heartbeat file was not written - the notice must not disturb it"
	exit 1
fi

echo "PASS"
