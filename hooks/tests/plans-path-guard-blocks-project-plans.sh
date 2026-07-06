#!/usr/bin/env bash
set -euo pipefail
HOOK="$(dirname "$0")/../plans-path-guard.sh"
INPUT='{"tool_name":"Edit","tool_input":{"file_path":"/home/user/.claude/projects/-home-user-repo/plans/PLAN-feature.md","old_string":"a","new_string":"b"}}'
set +e
echo "$INPUT" | bash "$HOOK" 2>/dev/null
CODE=$?
set -e
if [[ $CODE -eq 2 ]]; then
    echo "PASS"
else
    echo "FAIL: expected exit 2 for Edit under ~/.claude/projects/*/plans/, got exit $CODE"
    exit 1
fi
