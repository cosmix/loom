#!/usr/bin/env bash
# The edit-recording addition must never break the base heartbeat behaviour
# when `loom` is simply absent from PATH.
set -euo pipefail
HOOK="$(dirname "$0")/../post-tool-use.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/work"

INPUT='{"tool_name":"Write","tool_input":{"file_path":"src/foo.rs"}}'

set +e
OUTPUT=$(printf '%s' "$INPUT" |
	env -u LOOM_MAIN_AGENT_PID -u LOOM_WORKTREE_PATH \
	PATH="/usr/bin:/bin" \
	LOOM_STAGE_ID="test-stage" LOOM_SESSION_ID="test-session" LOOM_WORK_DIR="$TMP/work" \
	bash "$HOOK")
CODE=$?
set -e

if [[ $CODE -ne 0 ]]; then
	echo "FAIL: expected exit 0 when loom is absent from PATH, got $CODE"
	exit 1
fi
if [[ -n "$OUTPUT" ]]; then
	echo "FAIL: expected empty stdout, got: $OUTPUT"
	exit 1
fi

HEARTBEAT="$TMP/work/heartbeat/test-stage.json"
if [[ ! -f "$HEARTBEAT" ]]; then
	echo "FAIL: heartbeat must still be written when loom is absent from PATH"
	exit 1
fi

echo "PASS"
