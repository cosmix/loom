#!/usr/bin/env bash
# A blocking PreToolUse guard refuses (exit 2, one bad-payload line on stderr)
# an empty, truncated or tool_name-less payload and allows an unrelated tool;
# the advisory hooks keep allowing empty input.
set -euo pipefail
HOOKS=$(cd "$(dirname "$0")/.." && pwd)
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-bad-payload.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/home" "$TMP/cwd" "$TMP/scratch"

fail() {
	echo "FAIL: $1"
	exit 1
}

# run_hook <script> <stdin> [arg] - sets RC and ERR (stderr only).
run_hook() {
	local hook="$1" body="$2"
	shift 2
	RC=0
	ERR=$(cd "$TMP/cwd" && printf '%s' "$body" | env -i PATH="$PATH" HOME="$TMP/home" \
		LOOM_SESSION_ID=session-1 LOOM_SCRATCH_DIR="$TMP/scratch" LOOM_STAGE_ID=stage-1 \
		bash "$HOOKS/$hook" "$@" 2>&1 >/dev/null) || RC=$?
}

# "script [arg]" entries; codex-apply-patch.sh runs in its pre mode.
BLOCKING=(
	"codex-apply-patch.sh pre" commit-filter.sh git-add-guard.sh worktree-isolation.sh
	worktree-file-guard.sh plans-path-guard.sh credential-guard.sh loom-relay-guard.sh
	loom-control-complete.sh loom-control-subagents.sh
)
BODIES=('' '{"tool_name":"Ba' '{}')
for entry in "${BLOCKING[@]}"; do
	# Word splitting is intended: it separates the script from its argument.
	# shellcheck disable=SC2086
	set -- $entry
	hook=$1
	shift
	for body in "${BODIES[@]}"; do
		run_hook "$hook" "$body" "$@"
		[[ $RC -eq 2 ]] || fail "$hook must exit 2 for stdin '$body', got $RC: $ERR"
		[[ "$ERR" == *"bad-payload"* && "$(printf '%s\n' "$ERR" | wc -l | tr -d ' ')" == 1 ]] ||
			fail "$hook must print one bad-payload line for stdin '$body', got: $ERR"
	done
	run_hook "$hook" '{"tool_name":"NoSuchTool"}' "$@"
	[[ $RC -eq 0 ]] || fail "$hook must allow an unrelated tool, got $RC: $ERR"
done

for hook in prefer-modern-tools.sh no-preexisting-failures.sh; do
	run_hook "$hook" ''
	[[ $RC -eq 0 ]] || fail "advisory $hook must allow empty input, got $RC: $ERR"
done
echo "PASS"
