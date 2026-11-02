#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
HOOK="$ROOT/loom-hooks/stage-terminal-guard.sh"
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

INPUT='{"tool_name":"Write","tool_input":{"file_path":"src/main.rs","content":"x"}}'

run_hook() {
	# bash 3.2 (macOS) errors under `set -u` on "${arr[@]}" for an EMPTY array,
	# so only expand extra_env when a caller actually passed something.
	local extra_env=("$@")
	(cd "$WORKTREE" && printf '%s' "$INPUT" | env ${extra_env[@]+"${extra_env[@]}"} bash "$HOOK" 2>/dev/null)
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

# Case 5: the current .loom/work layout (not just the legacy .work above) ->
# BLOCKED (exit 2) for status completed
LOOM_STAGE_ID="build-api"
LOOM_PROJECT_ROOT="$TMP/loom-repo"
LOOM_WORKTREE="$LOOM_PROJECT_ROOT/.worktrees/$LOOM_STAGE_ID"
LOOM_STAGES_DIR="$LOOM_PROJECT_ROOT/.loom/work/stages"
mkdir -p "$LOOM_WORKTREE" "$LOOM_STAGES_DIR"
cat >"$LOOM_STAGES_DIR/01-$LOOM_STAGE_ID.md" <<EOF
---
id: $LOOM_STAGE_ID
status: completed
---

# Stage
EOF
set +e
(cd "$LOOM_WORKTREE" && printf '%s' "$INPUT" | bash "$HOOK" 2>/dev/null)
CODE=$?
set -e
if [[ $CODE -ne 2 ]]; then
	echo "FAIL: expected exit 2 for status 'completed' under .loom/work layout, got exit $CODE"
	exit 1
fi

# Case 6: the project root itself sits inside an outer loom worktree (loom
# developing loom, or TMPDIR under a worktree) -> the stage id must come from
# the INNERMOST .worktrees/ segment, so status completed is still BLOCKED
# (exit 2). Taking the outermost segment yielded 'outer-stage', which has no
# stage file, so the write was silently allowed.
NESTED_ROOT="$TMP/outer/.worktrees/outer-stage/repo"
NESTED_WORKTREE="$NESTED_ROOT/.worktrees/$STAGE_ID"
NESTED_STAGES_DIR="$NESTED_ROOT/.loom/work/stages"
mkdir -p "$NESTED_WORKTREE" "$NESTED_STAGES_DIR"
cat >"$NESTED_STAGES_DIR/01-$STAGE_ID.md" <<EOF
---
id: $STAGE_ID
status: completed
---

# Stage
EOF
set +e
(cd "$NESTED_WORKTREE" && printf '%s' "$INPUT" | bash "$HOOK" 2>/dev/null)
CODE=$?
set -e
if [[ $CODE -ne 2 ]]; then
	echo "FAIL: expected exit 2 for status 'completed' with the project root inside an outer worktree, got exit $CODE"
	exit 1
fi

# Case 7: this guard is registered globally, so a LOOM_WORKTREE_PATH inherited
# from a stage's shell must not block an unrelated session's writes. Only the
# session's own location makes it a stage session.
write_stage_status "completed"
UNRELATED="$TMP/unrelated"
mkdir -p "$UNRELATED"
set +e
(cd "$UNRELATED" && printf '%s' "$INPUT" | LOOM_WORKTREE_PATH="$WORKTREE" bash "$HOOK" 2>/dev/null)
CODE=$?
set -e
if [[ $CODE -ne 0 ]]; then
	echo "FAIL: an unrelated session with an inherited LOOM_WORKTREE_PATH was blocked (exit $CODE)"
	exit 1
fi
set +e
run_hook LOOM_WORKTREE_PATH="$UNRELATED/.worktrees/other"
CODE=$?
set -e
if [[ $CODE -ne 2 ]]; then
	echo "FAIL: the session's own worktree stopped deciding the guard (exit $CODE)"
	exit 1
fi

# Case 8: a relative path has no root, so the upward walk must end instead of
# spinning forever on the last component.
walk_ended() {
	local path="$1" waited=0 pid=""
	bash -c 'source "$1"; loom_stage_work_dir "$2"' _ "$ROOT/loom-hooks/_stage_lookup.sh" "$path" \
		>/dev/null 2>&1 &
	pid=$!
	while kill -0 "$pid" 2>/dev/null; do
		if ((waited >= 50)); then
			kill "$pid" 2>/dev/null || true
			wait "$pid" 2>/dev/null || true
			return 1
		fi
		sleep 0.1
		waited=$((waited + 1))
	done
	wait "$pid" 2>/dev/null || true
}
for relative in 'wt' 'repo/.worktrees/stage' './.worktrees/stage'; do
	if ! walk_ended "$relative"; then
		echo "FAIL: loom_stage_work_dir never returned for relative path '$relative'"
		exit 1
	fi
done

# Exercise the source-only owner directly as well as the guard decision.
source "$ROOT/loom-hooks/_common.sh"
if loom_stage_work_dir "repo/.worktrees/stage" >/dev/null; then
	echo "FAIL: a relative path must resolve to no state directory"
	exit 1
fi
[[ $(loom_stage_work_dir "$WORKTREE") == "$PROJECT_ROOT/.work" ]] || {
	echo "FAIL: an absolute worktree path must still find its state directory"
	exit 1
}
write_stage_status '"completed" # terminal'
[[ $(loom_stage_status "$STAGES_DIR/01-$STAGE_ID.md") == completed ]]

# Ambiguous, symlinked and malformed records cannot establish terminal state.
write_stage_status completed
cp "$STAGES_DIR/01-$STAGE_ID.md" "$STAGES_DIR/$STAGE_ID.md"
run_hook || { echo "FAIL: ambiguous stage"; exit 1; }
rm "$STAGES_DIR/$STAGE_ID.md"
mv "$STAGES_DIR/01-$STAGE_ID.md" "$TMP/stage-target"
ln -s "$TMP/stage-target" "$STAGES_DIR/01-$STAGE_ID.md"
run_hook || { echo "FAIL: symlink stage"; exit 1; }
rm "$STAGES_DIR/01-$STAGE_ID.md"
printf '%s\n' '---' 'status: completed' >"$STAGES_DIR/01-$STAGE_ID.md"
run_hook || { echo "FAIL: unclosed frontmatter"; exit 1; }
printf '%s\n' '---' 'status: completed' 'status: executing' '---' >"$STAGES_DIR/01-$STAGE_ID.md"
run_hook || { echo "FAIL: duplicate status"; exit 1; }

# Lifecycle binding and terminal decisions use the same stage lookup and
# frontmatter boundary, while applying their own field requirements.
source "$ROOT/loom-hooks/_lifecycle.sh"
printf '%s\n' '---' "id: $STAGE_ID" 'session: owner-1' 'status: completed' '---' >"$STAGES_DIR/01-$STAGE_ID.md"
loom_lifecycle_stage_binding "$PROJECT_ROOT/.work" "$STAGE_ID" owner-1 || {
	echo "FAIL: lifecycle rejected valid shared metadata"; exit 1;
}
cp "$STAGES_DIR/01-$STAGE_ID.md" "$STAGES_DIR/$STAGE_ID.md"
if loom_lifecycle_stage_binding "$PROJECT_ROOT/.work" "$STAGE_ID" owner-1; then
	echo "FAIL: lifecycle accepted duplicate canonical and legacy stage files"; exit 1
fi
rm "$STAGES_DIR/$STAGE_ID.md"
printf '%s\n' '---' "id: $STAGE_ID" 'session: owner-1' 'session: owner-2' '---' >"$STAGES_DIR/01-$STAGE_ID.md"
if loom_lifecycle_stage_binding "$PROJECT_ROOT/.work" "$STAGE_ID" owner-1; then
	echo "FAIL: lifecycle accepted duplicate session metadata"; exit 1
fi
printf '%s\n' '---' "id: $STAGE_ID" 'session: owner-1' >"$STAGES_DIR/01-$STAGE_ID.md"
if loom_lifecycle_stage_binding "$PROJECT_ROOT/.work" "$STAGE_ID" owner-1; then
	echo "FAIL: lifecycle accepted unclosed frontmatter"; exit 1
fi
mv "$STAGES_DIR/01-$STAGE_ID.md" "$TMP/lifecycle-target"
ln -s "$TMP/lifecycle-target" "$STAGES_DIR/01-$STAGE_ID.md"
if loom_lifecycle_stage_binding "$PROJECT_ROOT/.work" "$STAGE_ID" owner-1; then
	echo "FAIL: lifecycle accepted a symlinked stage file"; exit 1
fi

echo "PASS"
