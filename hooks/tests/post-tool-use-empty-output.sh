#!/usr/bin/env bash
# Regression: empty tool output must record output_bytes=0, not 1.
# Previously `echo "$OUTPUT_TEXT" | wc -c` added a trailing newline, silently
# breaking the failure-shape heuristic in tool_analysis::analyze_session.
set -euo pipefail
HOOK="$(dirname "$0")/../post-tool-use.sh"
TMPDIR_TEST=$(mktemp -d)
trap 'rm -rf "$TMPDIR_TEST"' EXIT

export LOOM_STAGE_ID="test-stage"
export LOOM_SESSION_ID="test-session"
export LOOM_WORK_DIR="$TMPDIR_TEST"

INPUT='{"tool_name":"Bash","tool_input":{"command":"true"},"tool_result":{"output":"","is_error":false}}'

bash "$HOOK" <<< "$INPUT"

EVENTS="$TMPDIR_TEST/tool-events.jsonl"
OUTPUT_BYTES=$(jq -r '.output_bytes' "$EVENTS")
if [[ "$OUTPUT_BYTES" != "0" ]]; then
    echo "FAIL: empty output recorded as output_bytes=$OUTPUT_BYTES (expected 0)"
    exit 1
fi

echo "PASS"
