#!/usr/bin/env bash
set -euo pipefail

HOOK="$(dirname "$0")/../worktree-file-guard.sh"

if ! printf '' | bash "$HOOK" >/dev/null 2>&1; then
    echo "FAIL: expected empty input to remain allowed"
    exit 1
fi

set +e
OUTPUT=$(printf '%s' '{"tool_name":"Write"' | bash "$HOOK" 2>&1)
CODE=$?
set -e

if [[ $CODE -ne 2 ]]; then
    echo "FAIL: expected malformed non-empty input to be blocked with exit 2, got $CODE"
    exit 1
fi

if [[ "$OUTPUT" != *"metadata could not be parsed"* ]]; then
    echo "FAIL: expected malformed-input block message, got: $OUTPUT"
    exit 1
fi

echo "PASS: empty input is allowed and malformed non-empty input is blocked"
