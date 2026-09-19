#!/usr/bin/env bash
set -euo pipefail
# See prefer-modern-tools-grep.sh for why the live stage vars must be unset.
unset LOOM_WORK_DIR LOOM_SESSION_ID LOOM_STAGE_ID LOOM_SESSION_TYPE
HOOK="$(dirname "$0")/../prefer-modern-tools.sh"
# Fresh TMPDIR: see prefer-modern-tools-grep.sh for why the "tools" ledger
# needs per-test isolation outside a loom stage.
TMP=$(mktemp -d "${TMPDIR:-/tmp}/pmt-find.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
INPUT='{"tool_name":"Bash","tool_input":{"command":"find . -name \"*.txt\""}}'
OUTPUT=$(echo "$INPUT" | TMPDIR="$TMP" bash "$HOOK")
if echo "$OUTPUT" | grep -q "hookSpecificOutput" && echo "$OUTPUT" | grep -q "LOOM_HOOK_WARN"; then
    echo "PASS"
else
    echo "FAIL: expected hookSpecificOutput with LOOM_HOOK_WARN, got: $OUTPUT"
    exit 1
fi
