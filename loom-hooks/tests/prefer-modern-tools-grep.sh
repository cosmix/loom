#!/usr/bin/env bash
set -euo pipefail
HOOK="$(dirname "$0")/../prefer-modern-tools.sh"
INPUT='{"tool_name":"Bash","tool_input":{"command":"grep -r pattern ."}}'
OUTPUT=$(echo "$INPUT" | bash "$HOOK")
if echo "$OUTPUT" | grep -q "hookSpecificOutput" && echo "$OUTPUT" | grep -q "LOOM_HOOK_WARN"; then
    echo "PASS"
else
    echo "FAIL: expected hookSpecificOutput with LOOM_HOOK_WARN, got: $OUTPUT"
    exit 1
fi
