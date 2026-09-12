#!/usr/bin/env bash
set -euo pipefail
HOOK="$(dirname "$0")/../plans-path-guard.sh"

# doc/plans/ is the correct location - must be allowed
INPUT='{"tool_name":"Write","tool_input":{"file_path":"doc/plans/PLAN-feature.md","content":"# Plan"}}'
if ! echo "$INPUT" | bash "$HOOK" 2>/dev/null; then
    echo "FAIL: Write to doc/plans/ should be allowed"
    exit 1
fi

# Other .claude files (settings, skills) - must be allowed
INPUT='{"tool_name":"Write","tool_input":{"file_path":"/home/user/.claude/settings.json","content":"{}"}}'
if ! echo "$INPUT" | bash "$HOOK" 2>/dev/null; then
    echo "FAIL: Write to .claude/settings.json should be allowed"
    exit 1
fi

# Similar-looking segment names must not false-positive
INPUT='{"tool_name":"Write","tool_input":{"file_path":"/home/user/.claude/plans-archive/notes.md","content":"x"}}'
if ! echo "$INPUT" | bash "$HOOK" 2>/dev/null; then
    echo "FAIL: Write to .claude/plans-archive/ should be allowed"
    exit 1
fi

# Non-file tools pass through
INPUT='{"tool_name":"Bash","tool_input":{"command":"ls ~/.claude/plans"}}'
if ! echo "$INPUT" | bash "$HOOK" 2>/dev/null; then
    echo "FAIL: Bash tool calls should be allowed"
    exit 1
fi

echo "PASS"
