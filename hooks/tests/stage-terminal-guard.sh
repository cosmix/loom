#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
HOOK="$ROOT/hooks/stage-terminal-guard.sh"
TMP=$(mktemp -d)
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

INPUT='{"tool_name":"Write","tool_input":{"file_path":"src/main.rs","content":"x"}}'

run_hook() {
	local extra_env=("$@")
	(cd "$WORKTREE" && printf '%s' "$INPUT" | env "${extra_env[@]}" bash "$HOOK" 2>/dev/null)
}

# Case 1: status completed -> BLOCKED (exit 2)
write_stage_status "completed"
set +e
run_hook
CODE=$?
set -e
if [[ $CODE -ne 2 ]]; then
	echo "FAIL: expected exit 2 for status 'completed', got exit $CODE"
	exit 1
fi

# Case 2: status executing -> allowed (exit 0)
write_stage_status "executing"
if ! run_hook; then
	echo "FAIL: expected exit 0 for status 'executing'"
	exit 1
fi

# Case 3: status completed-with-failures -> allowed (exit 0), the agent must
# keep fixing and re-complete
write_stage_status "completed-with-failures"
if ! run_hook; then
	echo "FAIL: expected exit 0 for status 'completed-with-failures'"
	exit 1
fi

# Case 4: LOOM_MERGE_SESSION=1 with status completed -> allowed (exit 0)
write_stage_status "completed"
if ! run_hook LOOM_MERGE_SESSION=1; then
	echo "FAIL: expected exit 0 when LOOM_MERGE_SESSION=1 even with status 'completed'"
	exit 1
fi

echo "PASS"
