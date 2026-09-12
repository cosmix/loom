#!/usr/bin/env bash
# Regression: even an empty tool result must not create a shared output log.
set -euo pipefail
HOOK="$(dirname "$0")/../post-tool-use.sh"
TMPDIR_TEST=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMPDIR_TEST"' EXIT

export LOOM_STAGE_ID="test-stage"
export LOOM_SESSION_ID="test-session"
export LOOM_WORK_DIR="$TMPDIR_TEST"

INPUT='{"tool_name":"Bash","tool_input":{"command":"true"},"tool_result":{"output":"","is_error":false}}'

bash "$HOOK" <<< "$INPUT"

EVENTS="$TMPDIR_TEST/tool-events.jsonl"
if [[ -e "$EVENTS" || -L "$EVENTS" ]]; then
    echo "FAIL: empty tool output created tool-events.jsonl"
    exit 1
fi

HEARTBEAT="$TMPDIR_TEST/heartbeat/test-stage.json"
if [[ ! -f "$HEARTBEAT" ]]; then
    echo "FAIL: heartbeat file not created"
    exit 1
fi

echo "PASS"
