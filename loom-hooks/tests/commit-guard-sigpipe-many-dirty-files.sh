#!/usr/bin/env bash
# get_uncommitted_changes() used to pipe `git status --porcelain` straight
# into `head -10` under `set -euo pipefail`. With enough dirty paths, head
# exits before git finishes writing, git dies with SIGPIPE (141), and
# pipefail turns that into the hook's exit status with no stderr. This test
# forces a large porcelain listing and runs the hook repeatedly, since the
# failure is a race (16 of 60 runs failed before the fix).
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

git -C "$WORKTREE" init -q
git -C "$WORKTREE" config user.email "test@example.com"
git -C "$WORKTREE" config user.name "Test User"

for i in $(seq 1 400); do
	: >"$WORKTREE/untracked-file-with-a-long-name-to-inflate-porcelain-output-$i.txt"
done

LAST_OUTPUT=""
for i in $(seq 1 40); do
	if OUTPUT=$(cd "$WORKTREE" && printf '{}' |
		env -u LOOM_MERGE_SESSION GIT_CEILING_DIRECTORIES="$TMP" bash "$HOOK" 2>&1); then
		rc=0
	else
		rc=$?
	fi
	if [[ "$rc" -ne 0 ]]; then
		echo "FAIL: hook exited $rc on iteration $i, output: $OUTPUT"
		exit 1
	fi
	LAST_OUTPUT="$OUTPUT"
done

if [[ "$LAST_OUTPUT" != *"COMPLETION CHECKLIST for stage '$STAGE_ID'"* ]]; then
	echo "FAIL: expected the completion checklist in output, got: $LAST_OUTPUT"
	exit 1
fi

if [[ "$LAST_OUTPUT" != *"Modified files:"* ]]; then
	echo "FAIL: expected 'Modified files:' in output, got: $LAST_OUTPUT"
	exit 1
fi

echo "PASS"
