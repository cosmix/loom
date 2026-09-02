#!/usr/bin/env bash
# Work done after `loom stage complete` is lost work regardless of which tool
# does the writing, so a terminal stage must refuse MultiEdit exactly as it
# refuses Write/Edit. A non-terminal status must still allow it, otherwise the
# guard would strand a session that is still fixing.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
HOOK="$ROOT/hooks/stage-terminal-guard.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

STAGE_ID="build-api"
PROJECT_ROOT="$TMP/repo"
WORKTREE="$PROJECT_ROOT/.worktrees/$STAGE_ID"
STAGES_DIR="$PROJECT_ROOT/.work/stages"
mkdir -p "$WORKTREE" "$STAGES_DIR"

write_stage_status() {
	local status="$1"
	cat >"$STAGES_DIR/01-$STAGE_ID.md" <<EOF
---
id: $STAGE_ID
status: $status
---

# Stage
EOF
}

INPUT='{"tool_name":"MultiEdit","tool_input":{"file_path":"src/main.rs","edits":[{"old_string":"a","new_string":"b"}]}}'

run_hook() {
	(cd "$WORKTREE" && printf '%s' "$INPUT" | bash "$HOOK" 2>/dev/null)
}

# Case 1: status completed -> BLOCKED (exit 2)
write_stage_status "completed"
set +e
run_hook
CODE=$?
set -e
if [[ $CODE -ne 2 ]]; then
	echo "FAIL: expected exit 2 for MultiEdit with status 'completed', got exit $CODE"
	exit 1
fi

# Case 2: status verified -> BLOCKED (exit 2)
write_stage_status "verified"
set +e
run_hook
CODE=$?
set -e
if [[ $CODE -ne 2 ]]; then
	echo "FAIL: expected exit 2 for MultiEdit with status 'verified', got exit $CODE"
	exit 1
fi

# Case 3: status executing -> allowed (exit 0)
write_stage_status "executing"
if ! run_hook; then
	echo "FAIL: expected exit 0 for MultiEdit with status 'executing'"
	exit 1
fi

echo "PASS"
