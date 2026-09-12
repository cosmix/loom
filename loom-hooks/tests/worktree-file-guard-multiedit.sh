#!/usr/bin/env bash
# MultiEdit writes files exactly as Edit does, so the worktree boundary must
# hold for it identically: an absolute path outside the worktree is blocked, a
# path inside it is allowed. The allow case is what proves the guard discerns
# the boundary rather than rejecting every MultiEdit outright.
set -euo pipefail

HOOK="$(cd "$(dirname "$0")/.." && pwd)/worktree-file-guard.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

WORKTREE="$TMP/repo/.worktrees/build-api"
OUTSIDE="$TMP/elsewhere"
mkdir -p "$WORKTREE/src" "$OUTSIDE"

run_hook() {
	local payload="$1"
	(cd "$WORKTREE" && printf '%s' "$payload" | bash "$HOOK" 2>&1)
}

edit_payload() {
	printf '{"tool_name":"MultiEdit","tool_input":{"file_path":"%s","edits":[{"old_string":"a","new_string":"b"}]}}' "$1"
}

# Case 1: absolute path outside the worktree -> BLOCKED (exit 2)
set +e
OUTPUT=$(run_hook "$(edit_payload "$OUTSIDE/secrets.txt")")
CODE=$?
set -e
if [[ $CODE -ne 2 ]]; then
	echo "FAIL: expected exit 2 for MultiEdit outside the worktree, got exit $CODE"
	exit 1
fi
if [[ "$OUTPUT" != *"outside the current worktree"* ]]; then
	echo "FAIL: expected the out-of-worktree block message, got: $OUTPUT"
	exit 1
fi

# Case 2: parent-directory traversal -> BLOCKED (exit 2)
set +e
run_hook "$(edit_payload "../build-api-other/src/lib.rs")" >/dev/null
CODE=$?
set -e
if [[ $CODE -ne 2 ]]; then
	echo "FAIL: expected exit 2 for MultiEdit with a '..' component, got exit $CODE"
	exit 1
fi

# Case 3: relative path inside the worktree -> allowed (exit 0)
if ! run_hook "$(edit_payload "src/main.rs")" >/dev/null; then
	echo "FAIL: expected MultiEdit inside the worktree to be allowed"
	exit 1
fi

# Case 4: absolute path inside the worktree -> allowed (exit 0)
if ! run_hook "$(edit_payload "$WORKTREE/src/main.rs")" >/dev/null; then
	echo "FAIL: expected an absolute in-worktree MultiEdit path to be allowed"
	exit 1
fi

echo "PASS"
