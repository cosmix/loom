#!/usr/bin/env bash
set -euo pipefail
# Run outside any live loom stage: LOOM_WORK_DIR/LOOM_SESSION_ID/LOOM_STAGE_ID
# would otherwise route the "tools" ledger to the real session directory
# instead of the fresh TMPDIR below, defeating this test's isolation.
unset LOOM_WORK_DIR LOOM_SESSION_ID LOOM_STAGE_ID LOOM_SESSION_TYPE
HOOK="$(dirname "$0")/../prefer-modern-tools.sh"
# Fresh TMPDIR: outside a loom stage the "tools" ledger falls back to
# ${TMPDIR:-/tmp}/loom-tools/<session>.tsv keyed only by session id (absent
# here, so "unknown") - shared with every other test in the same run unless
# isolated, which would silently suppress this warning as "already warned".
TMP=$(mktemp -d "${TMPDIR:-/tmp}/pmt-grep.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
INPUT='{"tool_name":"Bash","tool_input":{"command":"grep -r pattern ."}}'
OUTPUT=$(echo "$INPUT" | TMPDIR="$TMP" bash "$HOOK")
if echo "$OUTPUT" | grep -q "hookSpecificOutput" && echo "$OUTPUT" | grep -q "LOOM_HOOK_WARN"; then
    echo "PASS"
else
    echo "FAIL: expected hookSpecificOutput with LOOM_HOOK_WARN, got: $OUTPUT"
    exit 1
fi
