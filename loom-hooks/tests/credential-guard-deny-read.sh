#!/usr/bin/env bash
# credential-guard.sh rule (b): the project's own sandbox.filesystem.denyRead
# list gates the file tools the way it already gates Bash - including the
# containment rule that lets a search rooted ABOVE a denied directory run.
set -euo pipefail
HOOK="$(cd "$(dirname "$0")/.." && pwd)/credential-guard.sh"
FAILED=0

TMPROOT=$(mktemp -d)
trap 'rm -rf "$TMPROOT"' EXIT

# A throwaway HOME - the real one is never touched by this test.
FAKE_HOME="$TMPROOT/home"
PROJECT="$TMPROOT/project"
mkdir -p "$FAKE_HOME/.ssh/keys" "$PROJECT/.claude" "$PROJECT/src" "$PROJECT/secrets"
printf 'private-key\n' >"$FAKE_HOME/.ssh/id_rsa"
printf 'fn main() {}\n' >"$PROJECT/src/main.rs"
printf 'deploy-key\n' >"$PROJECT/secrets/deploy.pem"
printf '{"sandbox":{"filesystem":{"denyRead":["~/.ssh/**","secrets/*.pem"]}}}\n' \
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

read_payload() {
	printf '{"tool_name":"Read","tool_input":{"file_path":"%s"}}' "$1"
}

glob_payload() {
	printf '{"tool_name":"Glob","tool_input":{"pattern":"**/*","path":"%s"}}' "$1"
}

check "Read under a denied ~ pattern is blocked" \
	2 "$(read_payload "$FAKE_HOME/.ssh/id_rsa")"
check "Read of an ordinary project file is allowed" \
	0 "$(read_payload "$PROJECT/src/main.rs")"
check "Read under a project-relative denied pattern is blocked" \
	2 "$(read_payload "$PROJECT/secrets/deploy.pem")"
check "Glob rooted inside a denied directory is blocked" \
	2 "$(glob_payload "$FAKE_HOME/.ssh/keys")"
check "Glob rooted above a denied directory still runs" \
	0 "$(glob_payload "$PROJECT")"

exit "$FAILED"
