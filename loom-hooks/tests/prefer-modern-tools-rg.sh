#!/usr/bin/env bash
set -euo pipefail
HOOK="$(dirname "$0")/../prefer-modern-tools.sh"
INPUT='{"tool_name":"Bash","tool_input":{"command":"rg pattern ."}}'
OUTPUT=$(echo "$INPUT" | bash "$HOOK")
if [[ -z "$OUTPUT" ]]; then
    echo "PASS"
else
    echo "FAIL: expected empty stdout for rg command, got: $OUTPUT"
    exit 1
fi
