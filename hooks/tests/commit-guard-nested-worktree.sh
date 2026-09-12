#!/usr/bin/env bash
# commit-guard.sh must attribute a session to the INNERMOST `.worktrees/<id>`
# enclosing its cwd. A repo can itself live inside an outer loom worktree (loom
# developing loom); taking the outermost segment named a stage with no status
# file, so the Stop-hook checklist silently skipped a still-executing stage.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
HOOK="$ROOT/hooks/commit-guard.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

STAGE_ID="build-api"
PROJECT_ROOT="$TMP/outer/.worktrees/outer-stage/repo"
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

# The hook is advisory (always exit 0), so the stage it resolved is read from
# its checklist on stderr. GIT_CEILING_DIRECTORIES stops `git status` from
# finding whatever repository encloses $TMP, keeping the run TMPDIR-independent.
OUTPUT=$(cd "$WORKTREE" && printf '{}' |
	env -u LOOM_MERGE_SESSION GIT_CEILING_DIRECTORIES="$TMP" bash "$HOOK" 2>&1)

if [[ "$OUTPUT" != *"COMPLETION CHECKLIST for stage '$STAGE_ID'"* ]]; then
	echo "FAIL: expected the checklist for the innermost stage '$STAGE_ID', got: $OUTPUT"
	exit 1
fi

echo "PASS"
