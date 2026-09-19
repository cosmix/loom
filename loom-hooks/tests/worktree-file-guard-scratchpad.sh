#!/usr/bin/env bash
# The harness scratchpad (/tmp/claude-<uid>/<project>/<session>/scratchpad/) is
# open to every file tool for the session that owns it. Everything around it
# stays blocked: another session's scratchpad, a sibling directory, a symlinked
# component or leaf, a `.` or `..` component, a component owned by another
# uid, another uid's root, and every other /tmp path.
set -euo pipefail

HOOK="$(cd "$(dirname "$0")/.." && pwd)/worktree-file-guard.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
UID_ROOT="/tmp/claude-$(id -u)"
CREATED_ROOT=false
if [[ ! -d "$UID_ROOT" ]]; then
	mkdir -m 700 "$UID_ROOT" && CREATED_ROOT=true
fi
PROJECT=$(mktemp -d "$UID_ROOT/loom-hooktest-project.XXXXXX")
cleanup() {
	rm -rf "$TMP" "$PROJECT"
	[[ "$CREATED_ROOT" == false ]] || rmdir "$UID_ROOT" 2>/dev/null || true
}
trap cleanup EXIT

fail() {
	echo "FAIL: $*" >&2
	exit 1
}

SESSION="session-$$"
SCRATCH="$PROJECT/$SESSION/scratchpad"
WORKTREE="$TMP/repo/.worktrees/build-api"
OUTSIDE="$TMP/outside"
mkdir -p "$SCRATCH/sub" "$PROJECT/$SESSION/other" "$PROJECT/$SESSION/tasks" \
	"$PROJECT/session-other/scratchpad" "$PROJECT/session-link" "$WORKTREE" "$OUTSIDE" "$TMP/shim"
printf 'x\n' >"$SCRATCH/notes.txt"
printf 'secret\n' >"$OUTSIDE/secret"
printf 'done\n' >"$PROJECT/$SESSION/tasks/job.output"
ln -s "$OUTSIDE" "$SCRATCH/link"
ln -s "$OUTSIDE/secret" "$SCRATCH/leaf"
ln -s "$OUTSIDE" "$PROJECT/session-link/scratchpad"

# run_hook <tool> <path> [session_id] - one guard call from inside the worktree.
run_hook() {
	local tool=$1 path=$2 session=${3-$SESSION} field=file_path payload
	case "$tool" in Glob | Grep) field=path ;; NotebookEdit) field=notebook_path ;; esac
	payload=$(jq -nc --arg t "$tool" --arg f "$field" --arg p "$path" --arg s "$session" \
		'{tool_name: $t, tool_input: {($f): $p, pattern: "*", content: "x", old_string: "x", new_string: "y"}}
		 + (if $s == "" then {} else {session_id: $s} end)')
	set +e
	OUTPUT=$(cd "$WORKTREE" && printf '%s' "$payload" |
		env -i PATH="${SHIM_PATH:-}$PATH" HOME="$TMP" bash "$HOOK" 2>&1)
	RC=$?
	set -e
}

expect_allowed() {
	run_hook "$@"
	[[ "$RC" == 0 ]] || fail "$1 $2 was blocked: $OUTPUT"
}

expect_blocked() {
	run_hook "$@"
	[[ "$RC" == 2 && "$OUTPUT" == *"LOOM: BLOCKED"* ]] || fail "$1 $2 was not blocked (rc $RC): $OUTPUT"
}

# 1. The session's own scratchpad, for every file tool, new and existing paths.
expect_allowed Write "$SCRATCH/new.txt"
expect_allowed Write "$SCRATCH/sub/deeper/new.txt"
expect_allowed Read "$SCRATCH/notes.txt"
expect_allowed Edit "$SCRATCH/notes.txt"
expect_allowed MultiEdit "$SCRATCH/notes.txt"
expect_allowed NotebookEdit "$SCRATCH/book.ipynb"
expect_allowed Glob "$SCRATCH"
expect_allowed Grep "$SCRATCH/"

# 2. Everything around it stays blocked.
expect_blocked Write "$PROJECT/session-other/scratchpad/x.txt"
expect_blocked Read "$PROJECT/session-other/scratchpad"
expect_blocked Write "$SCRATCH/x.txt" ""
expect_blocked Write "$PROJECT/$SESSION/other/x.txt"
expect_blocked Write "$PROJECT/$SESSION/tasks/job.output"
expect_blocked Write "$SCRATCH/link/x.txt"
expect_blocked Read "$SCRATCH/link/secret"
expect_blocked Read "$SCRATCH/leaf"
expect_blocked Write "$PROJECT/session-link/scratchpad/x.txt" session-link
expect_blocked Write "$SCRATCH/./x.txt"
expect_blocked Write "$SCRATCH/../other/x.txt"
expect_blocked Write "/tmp/claude-424242/project/$SESSION/scratchpad/x.txt"
expect_blocked Write "$OUTSIDE/new.txt"
expect_blocked Read "$OUTSIDE/secret"

# 3. A scratchpad whose components belong to another uid stays blocked: a stat
# shim reports a foreign owner for every path under the scratchpad.
REAL_STAT=$(command -v stat)
printf '#!/usr/bin/env bash\nfor a in "$@"; do case "$a" in */scratchpad*) echo 424242; exit 0 ;; esac; done\nexec %q "$@"\n' \
	"$REAL_STAT" >"$TMP/shim/stat"
chmod +x "$TMP/shim/stat"
SHIM_PATH="$TMP/shim:" expect_blocked Write "$SCRATCH/new.txt"
SHIM_PATH="$TMP/shim:" expect_blocked Read "$SCRATCH/notes.txt"

# 4. Background task output stays Read-only (the refactored ownership helper).
if [[ ! -L /tmp ]]; then
	expect_allowed Read "$PROJECT/$SESSION/tasks/job.output"
fi

echo "PASS"
