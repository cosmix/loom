#!/usr/bin/env bash
set -euo pipefail
HOOK="$(dirname "$0")/../prefer-modern-tools.sh"
INPUT='{"tool_name":"Bash","tool_input":{"command":"echo \"use grep\""}}'
OUTPUT=$(echo "$INPUT" | bash "$HOOK")
if [[ -z "$OUTPUT" ]]; then
    echo "PASS"
else
    echo "FAIL: expected no warning for quoted grep in echo, got: $OUTPUT"
    exit 1
fi
