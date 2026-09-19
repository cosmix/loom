#!/usr/bin/env bash
# `git show`/`git diff` with no path prints an unbounded diff the agent then
# re-reads from context; poll-guard.sh warns once and suggests --stat first,
# then a per-file -- <path>. Flags that already bound the output to no diff
# at all (-s/--no-patch) or to a file list (--stat/--name-only/--name-status)
# are not the anti-pattern this rule targets, and a real path argument means
# the call is already scoped.
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
hook="$root/loom-hooks/poll-guard.sh"

fail() {
	printf 'FAIL: %s\n' "$1"
	exit 1
}

run_hook() {
	local command="$1" tmp
	tmp=$(mktemp -d "${TMPDIR:-/tmp}/poll-guard-pathless.XXXXXX")
	local payload
	payload=$(jq -nc --arg command "$command" '{tool_name:"Bash",tool_input:{command:$command}}')
	printf '%s' "$payload" |
		env -u LOOM_HOOK_DEBUG -u COMMIT_FILTER_DEBUG -u LOOM_HOOK_PATH \
			-u LOOM_WORK_DIR -u LOOM_STAGE_ID -u LOOM_SESSION_ID -u LOOM_MAIN_AGENT_PID \
			-u LOOM_SESSION_TYPE \
			TMPDIR="$tmp" bash "$hook" >"$tmp/stdout" 2>"$tmp/stderr"
	HOOK_CODE=$?
	HOOK_STDOUT=$(<"$tmp/stdout")
	rm -rf "$tmp"
}

assert_warns() {
	local label="$1" command="$2"
	set +e
	run_hook "$command"
	set -e
	[[ $HOOK_CODE -eq 0 ]] || fail "$label exited $HOOK_CODE"
	[[ "$HOOK_STDOUT" == *"ran with no path"* ]] || fail "$label did not warn: $HOOK_STDOUT"
}

assert_silent() {
	local label="$1" command="$2"
	set +e
	run_hook "$command"
	set -e
	[[ $HOOK_CODE -eq 0 ]] || fail "$label exited $HOOK_CODE"
	[[ -z "$HOOK_STDOUT" ]] || fail "$label warned unexpectedly: $HOOK_STDOUT"
}

# Still warns: a real pathless git show/diff, no bounding flag at all.
assert_warns "git show <rev> with no path warns" "git show abc123"
assert_warns "git diff with no path warns" "git diff"

# Silent: -s / --no-patch print no diff at all.
assert_silent "git show -s <rev> does not warn" "git show -s abc123"
assert_silent "git diff --no-patch does not warn" "git diff --no-patch"

# Silent (pre-existing): --stat/--name-only/--name-status already bound to a
# file list, and a real path argument already scopes the call.
assert_silent "git show --stat <rev> does not warn" "git show --stat abc123"
assert_silent "git diff --name-only does not warn" "git diff --name-only"
assert_silent "git show <rev> -- <path> does not warn" "git show abc123 -- src/main.rs"

printf 'PASS\n'
