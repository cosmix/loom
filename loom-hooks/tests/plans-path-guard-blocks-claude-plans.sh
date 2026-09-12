#!/usr/bin/env bash
set -euo pipefail
HOOK="$(dirname "$0")/../plans-path-guard.sh"
INPUT='{"tool_name":"Write","tool_input":{"file_path":"/home/user/.claude/plans/PLAN-feature.md","content":"# Plan"}}'
set +e
echo "$INPUT" | bash "$HOOK" 2>/dev/null
CODE=$?
set -e
if [[ $CODE -eq 2 ]]; then
    echo "PASS"
else
    echo "FAIL: expected exit 2 for Write to ~/.claude/plans/, got exit $CODE"
    exit 1
fi
