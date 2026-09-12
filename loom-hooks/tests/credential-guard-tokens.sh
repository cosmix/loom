#!/usr/bin/env bash
# credential-guard.sh rule (a): the orchestrator capability tokens are closed to
# the file tools through the state-root symlink, with or without a settings
# file, and only for the file tools.
set -euo pipefail
HOOK="$(cd "$(dirname "$0")/.." && pwd)/credential-guard.sh"
FAILED=0

TMPROOT=$(mktemp -d)
trap 'rm -rf "$TMPROOT"' EXIT

# A main repo holding the real token file, and a worktree reaching it the way
# loom lays one out: `.work` is a symlink to the main repo's state root.
MAIN="$TMPROOT/main"
WORKTREE="$TMPROOT/worktree"
mkdir -p "$MAIN/.work" "$WORKTREE"
printf 'admin-token-value\n' >"$MAIN/.work/admin.token"
ln -s "$MAIN/.work" "$WORKTREE/.work"

# One project with a settings file, one with no .claude directory at all.
PROJECT_WITH_SETTINGS="$TMPROOT/with-settings"
PROJECT_NO_SETTINGS="$TMPROOT/no-settings"
mkdir -p "$PROJECT_WITH_SETTINGS/.claude" "$PROJECT_NO_SETTINGS"
printf '{"sandbox":{"filesystem":{"denyRead":["~/.ssh/**"]}}}\n' \
	>"$PROJECT_WITH_SETTINGS/.claude/settings.local.json"

TOKEN_PATH="$WORKTREE/.work/admin.token"

check() {
	local name="$1" expected="$2" project_dir="$3" payload="$4"
	local status=0
	CLAUDE_PROJECT_DIR="$project_dir" bash "$HOOK" <<<"$payload" >/dev/null 2>&1 || status=$?
	if [[ "$status" -eq "$expected" ]]; then
		echo "PASS: $name"
	else
		echo "FAIL: $name - expected exit $expected, got $status"
		FAILED=1
	fi
}

READ_PAYLOAD=$(printf '{"tool_name":"Read","tool_input":{"file_path":"%s"}}' "$TOKEN_PATH")
BASH_PAYLOAD=$(printf '{"tool_name":"Bash","tool_input":{"file_path":"%s"}}' "$TOKEN_PATH")

check "Read of a token through the state-root symlink is blocked" \
	2 "$PROJECT_WITH_SETTINGS" "$READ_PAYLOAD"
check "the same Read is blocked with no settings.local.json anywhere" \
	2 "$PROJECT_NO_SETTINGS" "$READ_PAYLOAD"
check "a non-file tool with the same payload is untouched" \
	0 "$PROJECT_WITH_SETTINGS" "$BASH_PAYLOAD"

exit "$FAILED"
