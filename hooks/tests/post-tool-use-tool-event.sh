#!/usr/bin/env bash
set -euo pipefail
HOOK="$(dirname "$0")/../post-tool-use.sh"
TMPDIR_TEST=$(mktemp -d)
trap 'rm -rf "$TMPDIR_TEST"' EXIT

export LOOM_STAGE_ID="test-stage"
export LOOM_SESSION_ID="test-session"
export LOOM_WORK_DIR="$TMPDIR_TEST"

INPUT='{"tool_name":"Bash","tool_input":{"command":"echo hello"},"tool_result":{"output":"hello","is_error":false}}'

bash "$HOOK" <<< "$INPUT"

# Check heartbeat was created
HEARTBEAT="$TMPDIR_TEST/heartbeat/test-stage.json"
if [[ ! -f "$HEARTBEAT" ]]; then
    echo "FAIL: heartbeat file not created"
    exit 1
fi

# Check tool-events.jsonl was created and is valid JSON
EVENTS="$TMPDIR_TEST/tool-events.jsonl"
if [[ ! -f "$EVENTS" ]]; then
    echo "FAIL: tool-events.jsonl not created"
    exit 1
fi

# Check the event row is valid JSON
if ! jq -e . "$EVENTS" > /dev/null 2>&1; then
    echo "FAIL: tool-events.jsonl is not valid JSON"
    exit 1
fi

# Check required fields exist
if ! jq -e '.ts and .tool and (.is_error != null) and .session_id and .stage_id' "$EVENTS" > /dev/null 2>&1; then
    echo "FAIL: tool-events.jsonl missing required fields"
    exit 1
fi

echo "PASS"
