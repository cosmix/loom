#!/usr/bin/env bash
# commit-guard.sh must give a contract session (LOOM_SESSION_TYPE=contract) the
# contract reminder - freeze, do not commit, do not complete - instead of the
# commit-and-complete checklist, and still exit 0. A stage session in the same
# worktree keeps the checklist.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
HOOK="$ROOT/loom-hooks/commit-guard.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

STAGE_ID="build-api"
PROJECT_ROOT="$TMP/repo"
WORKTREE="$PROJECT_ROOT/.worktrees/$STAGE_ID"
STAGES_DIR="$PROJECT_ROOT/.loom/work/stages"
mkdir -p "$WORKTREE" "$STAGES_DIR"
cat >"$STAGES_DIR/01-$STAGE_ID.md" <<EOF
---
id: $STAGE_ID
status: executing
---

# Stage
EOF

# GIT_CEILING_DIRECTORIES stops `git status` from finding whatever repository
# encloses $TMP, keeping the run TMPDIR-independent.
run_hook() {
	local session_type="$1"
	(cd "$WORKTREE" && printf '{}' |
		env -u LOOM_MERGE_SESSION LOOM_SESSION_TYPE="$session_type" LOOM_STAGE_ID="$STAGE_ID" \
			GIT_CEILING_DIRECTORIES="$TMP" bash "$HOOK" 2>&1)
}

if ! OUTPUT=$(run_hook contract); then
	echo "FAIL: the hook must exit 0 for a contract session, got: $OUTPUT"
	exit 1
fi
for expected in \
	"LOOM CONTRACT REMINDER" \
	"loom stage contracts freeze $STAGE_ID" \
	"do not commit" \
	"do not complete the stage"; do
	if [[ "$OUTPUT" != *"$expected"* ]]; then
		echo "FAIL: contract reminder is missing '$expected', got: $OUTPUT"
		exit 1
	fi
done
if [[ "$OUTPUT" == *"COMPLETION CHECKLIST"* ]]; then
	echo "FAIL: a contract session must not get the commit-and-complete checklist, got: $OUTPUT"
	exit 1
fi

OUTPUT=$(run_hook stage)
if [[ "$OUTPUT" != *"COMPLETION CHECKLIST for stage '$STAGE_ID'"* ]]; then
	echo "FAIL: a stage session must keep the completion checklist, got: $OUTPUT"
	exit 1
fi
if [[ "$OUTPUT" == *"CONTRACT REMINDER"* ]]; then
	echo "FAIL: a stage session must not get the contract reminder, got: $OUTPUT"
	exit 1
fi

echo "PASS"
