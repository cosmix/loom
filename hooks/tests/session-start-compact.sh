#!/usr/bin/env bash
# Test: session-start.sh emits additionalContext on compact/resume source
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HOOK="$SCRIPT_DIR/../session-start.sh"

# Set up a temp work dir with required subdirectories
TMPDIR_TEST=$(mktemp -d)
trap 'rm -rf "$TMPDIR_TEST"' EXIT

LOOM_WORK_DIR="$TMPDIR_TEST"
LOOM_STAGE_ID="test-stage"
LOOM_SESSION_ID="session-test-abc"

mkdir -p "${LOOM_WORK_DIR}/hooks" "${LOOM_WORK_DIR}/heartbeat"

EXPECTED_SIGNAL_PATH="${LOOM_WORK_DIR}/signals/${LOOM_SESSION_ID}.md"

# Run hook with compact source
OUTPUT=$(echo '{"source":"compact"}' | \
    LOOM_WORK_DIR="$LOOM_WORK_DIR" \
    LOOM_STAGE_ID="$LOOM_STAGE_ID" \
    LOOM_SESSION_ID="$LOOM_SESSION_ID" \
    bash "$HOOK")

# Assert additionalContext appears in output
if ! echo "$OUTPUT" | jq -e '.hookSpecificOutput.additionalContext' >/dev/null 2>&1; then
    echo "FAIL: additionalContext not found in output for 'compact' source"
    echo "Output was: $OUTPUT"
    exit 1
fi

# Assert the signal path is mentioned
if ! echo "$OUTPUT" | jq -r '.hookSpecificOutput.additionalContext' | grep -qF "$EXPECTED_SIGNAL_PATH"; then
    echo "FAIL: signal path '$EXPECTED_SIGNAL_PATH' not found in additionalContext"
    echo "additionalContext was: $(echo "$OUTPUT" | jq -r '.hookSpecificOutput.additionalContext')"
    exit 1
fi

# Assert the key phrase is present
if ! echo "$OUTPUT" | jq -r '.hookSpecificOutput.additionalContext' | grep -q "Understand before acting"; then
    echo "FAIL: 'Understand before acting' not found in additionalContext"
    exit 1
fi

# Assert startup source emits NO additionalContext
OUTPUT2=$(echo '{"source":"startup"}' | \
    LOOM_WORK_DIR="$LOOM_WORK_DIR" \
    LOOM_STAGE_ID="$LOOM_STAGE_ID" \
    LOOM_SESSION_ID="$LOOM_SESSION_ID" \
    bash "$HOOK")

if echo "$OUTPUT2" | jq -e '.hookSpecificOutput' >/dev/null 2>&1; then
    echo "FAIL: additionalContext should NOT be emitted for 'startup' source"
    exit 1
fi

echo "PASS: session-start compact/resume re-anchor test"
