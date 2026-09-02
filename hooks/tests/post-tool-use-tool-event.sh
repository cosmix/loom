#!/usr/bin/env bash
set -euo pipefail
HOOK="$(dirname "$0")/../post-tool-use.sh"
TMPDIR_TEST=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
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

# Tool results are not persisted because a shell append cannot provide a
# race-free no-follow guarantee on the shared path.
EVENTS="$TMPDIR_TEST/tool-events.jsonl"
if [[ -e "$EVENTS" || -L "$EVENTS" ]]; then
    echo "FAIL: tool-events.jsonl must not be created"
    exit 1
fi

echo "PASS"
