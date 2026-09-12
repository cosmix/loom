#!/usr/bin/env bash
# Plan mode's suggested paths must be refused for MultiEdit as well as
# Write/Edit - MultiEdit names its target at the same `.tool_input.file_path`,
# so a guard that only matched Write/Edit left the rule bypassable. The
# doc/plans allow case is asserted alongside so this distinguishes a working
# guard from one that blocks everything.
set -euo pipefail
HOOK="$(dirname "$0")/../plans-path-guard.sh"

multiedit_payload() {
	printf '{"tool_name":"MultiEdit","tool_input":{"file_path":"%s","edits":[{"old_string":"a","new_string":"b"}]}}' "$1"
}

expect_blocked() {
	local path="$1" code
	set +e
	multiedit_payload "$path" | bash "$HOOK" 2>/dev/null
	code=$?
	set -e
	if [[ $code -ne 2 ]]; then
		echo "FAIL: expected exit 2 for MultiEdit to $path, got exit $code"
		exit 1
	fi
}

expect_allowed() {
	local path="$1"
	if ! multiedit_payload "$path" | bash "$HOOK" 2>/dev/null; then
		echo "FAIL: expected MultiEdit to $path to be allowed"
		exit 1
	fi
}

expect_blocked "/home/user/.claude/plans/PLAN-feature.md"
expect_blocked "/home/user/.claude/projects/loom/plans/PLAN-feature.md"

expect_allowed "doc/plans/PLAN-feature.md"
expect_allowed "/home/user/.claude/settings.json"
expect_allowed "/home/user/.claude/plans-archive/notes.md"

echo "PASS"
