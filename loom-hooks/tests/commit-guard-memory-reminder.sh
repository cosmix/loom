#!/usr/bin/env bash
# commit-guard.sh prints the Memory Recording Reminder when the stage's memory
# directory holds no file with more than 10 lines, and stays quiet when one
# does or when there is no memory directory. The line count is stripped of the
# padding BSD/macOS `wc -l` adds (run-all.sh in BSD mode puts the padding shim
# first on PATH), so the reminder logic reads the same number on both.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
HOOK="$ROOT/loom-hooks/commit-guard.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

STAGE_ID="build-api"
PROJECT_ROOT="$TMP/repo"
WORKTREE="$PROJECT_ROOT/.worktrees/$STAGE_ID"
STAGES_DIR="$PROJECT_ROOT/.loom/work/stages"
MEMORY_DIR="$PROJECT_ROOT/.loom/work/memory"
REMINDER="Memory Recording Reminder"
mkdir -p "$WORKTREE" "$STAGES_DIR"
cat >"$STAGES_DIR/01-$STAGE_ID.md" <<EOF
---
id: $STAGE_ID
status: executing
---

# Stage
EOF

# Acceptance runs carry no LOOM_* variables and a loom session carries its own,
# so scrub them all. GIT_CEILING_DIRECTORIES stops `git status` from finding
# whatever repository encloses $TMP, keeping the run TMPDIR-independent.
run_hook() {
	(cd "$WORKTREE" && printf '{}' |
		env -u LOOM_MERGE_SESSION -u LOOM_SESSION_TYPE -u LOOM_STAGE_ID -u LOOM_SESSION_ID \
			-u LOOM_WORK_DIR -u LOOM_WORKTREE_PATH -u LOOM_MAIN_AGENT_PID -u LOOM_HOOK_DEBUG \
			-u LOOM_HOOK_PATH -u LOOM_BIN -u LOOM_HOOK_CONTEXT -u LOOM_SCRATCH_DIR \
			GIT_CEILING_DIRECTORIES="$TMP" bash "$HOOK" 2>&1)
}

# write_lines <file> <count>: a memory file of exactly <count> lines.
write_lines() {
	local i
	: >"$1"
	for ((i = 1; i <= $2; i++)); do printf 'entry %d\n' "$i" >>"$1"; done
}

expect_reminder() {
	local label="$1" output
	output=$(run_hook)
	if [[ "$output" != *"$REMINDER"* || "$output" != *'loom memory decision "choice"'* ]]; then
		echo "FAIL: $label: expected the memory reminder, got: $output"
		exit 1
	fi
}

expect_quiet() {
	local label="$1" output
	output=$(run_hook)
	if [[ "$output" == *"$REMINDER"* ]]; then
		echo "FAIL: $label: the memory reminder must not print, got: $output"
		exit 1
	fi
	if [[ "$output" != *"COMPLETION CHECKLIST for stage '$STAGE_ID'"* ]]; then
		echo "FAIL: $label: the hook did not reach its checklist, got: $output"
		exit 1
	fi
}

expect_quiet "no memory directory"

mkdir -p "$MEMORY_DIR"
expect_reminder "an empty memory directory"

write_lines "$MEMORY_DIR/session.md" 3
expect_reminder "a memory file of 3 lines"

write_lines "$MEMORY_DIR/session.md" 10
expect_reminder "a memory file of exactly 10 lines"

write_lines "$MEMORY_DIR/session.md" 11
expect_quiet "a memory file of 11 lines"

write_lines "$MEMORY_DIR/session.md" 120
expect_quiet "a memory file of 120 lines"

echo "PASS"
