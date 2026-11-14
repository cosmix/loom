#!/usr/bin/env bash
set -euo pipefail
# Outside a stage, the hook's "tools" ledger lives under ${TMPDIR:-/tmp}; a
# private TMPDIR keeps this test off a real session's ledger and off other tests.
unset LOOM_WORK_DIR LOOM_SESSION_ID LOOM_STAGE_ID LOOM_SESSION_TYPE
HOOK="$(dirname "$0")/../prefer-modern-tools.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/pmt-quoted.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
INPUT='{"tool_name":"Bash","tool_input":{"command":"echo \"use grep\""}}'
OUTPUT=$(echo "$INPUT" | TMPDIR="$TMP" bash "$HOOK")
if [[ -z "$OUTPUT" ]]; then
    echo "PASS"
else
    echo "FAIL: expected no warning for quoted grep in echo, got: $OUTPUT"
    exit 1
fi
