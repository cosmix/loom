#!/usr/bin/env bash
# credential-guard.sh: a `..` after a nonexistent path component must not
# bypass the guard. Claude Code's Write tool normalizes a path lexically
# (Node's path.resolve) before it ever opens the file, so
# `~/.ssh/missing/../authorized_keys` writes `~/.ssh/authorized_keys` even
# though `missing` never exists - the guard has to see the same target the
# tool actually touches, and a climb above `/` has to block outright rather
# than fall through as "unresolvable".
set -euo pipefail
HOOK="$(cd "$(dirname "$0")/.." && pwd)/credential-guard.sh"
FAILED=0

TMPROOT=$(mktemp -d)
trap 'rm -rf "$TMPROOT"' EXIT

# A throwaway HOME - the real one is never touched by this test.
FAKE_HOME="$TMPROOT/home"
PROJECT="$TMPROOT/project"
mkdir -p "$FAKE_HOME/.ssh" "$FAKE_HOME/safe" "$PROJECT/.claude"
printf '{"sandbox":{"filesystem":{"denyRead":["~/.ssh/**"]}}}\n' \
	>"$PROJECT/.claude/settings.local.json"

check() {
	local name="$1" expected="$2" payload="$3"
	local status=0
	HOME="$FAKE_HOME" CLAUDE_PROJECT_DIR="$PROJECT" bash "$HOOK" \
		<<<"$payload" >/dev/null 2>&1 || status=$?
	if [[ "$status" -eq "$expected" ]]; then
		echo "PASS: $name"
	else
		echo "FAIL: $name - expected exit $expected, got $status"
		FAILED=1
	fi
}

write_payload() {
	printf '{"tool_name":"Write","tool_input":{"file_path":"%s","content":""}}' "$1"
}

read_payload() {
	printf '{"tool_name":"Read","tool_input":{"file_path":"%s"}}' "$1"
}

check "Write past a nonexistent component's .. lands on the denied .ssh dir" \
	2 "$(write_payload "$FAKE_HOME/.ssh/missing/../authorized_keys")"
check "Write past a nonexistent component's .. into an undenied dir is allowed" \
	0 "$(write_payload "$FAKE_HOME/safe/missing/../file.txt")"
check "a .. climb above the filesystem root is blocked outright" \
	2 "$(read_payload "/../../etc/passwd")"

exit "$FAILED"
