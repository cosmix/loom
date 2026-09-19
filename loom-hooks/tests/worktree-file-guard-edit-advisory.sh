#!/usr/bin/env bash
# The main agent of a stage session gets a warning (never a denial) when one
# edit call writes more than 20 lines or touches a third distinct code file,
# once per path. doc/, *.md and distill scratch files never count, a subagent
# never warns, a session outside a stage never warns, and an unwritable ledger
# never turns the advisory into a failure.
set -euo pipefail

HOOK="$(cd "$(dirname "$0")/.." && pwd)/worktree-file-guard.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'chmod -R u+w "$TMP" 2>/dev/null; rm -rf "$TMP"' EXIT

WORKTREE="$TMP/repo/.worktrees/build-api"
WORK_DIR="$TMP/repo/.loom/work"
mkdir -p "$WORKTREE/src" "$WORKTREE/doc" "$WORK_DIR"

fail() {
	echo "FAIL: $*" >&2
	exit 1
}

# edit <tool> <path> <lines> [agent_id] - One call as the main agent (or as the
# named subagent) in session $SID. Every call must be allowed.
edit() {
	local tool=$1 path=$2 n=$3 who=${4:-} body="" k payload
	for ((k = 0; k < n; k++)); do body+="line $k"$'\n'; done
	payload=$(jq -nc --arg t "$tool" --arg p "$path" --arg b "$body" --arg who "$who" --arg s "$SID" '
		{tool_name: $t, session_id: $s, transcript_path: ("/home/u/.claude/projects/p/" + $s + ".jsonl"),
		 tool_input: ({file_path: $p} + (if $t == "Write" then {content: $b}
			elif $t == "MultiEdit" then {edits: [{old_string: "a", new_string: $b}]}
			elif $t == "NotebookEdit" then {notebook_path: $p, new_source: $b}
			else {old_string: "a", new_string: $b} end))}
		+ (if $who == "" then {} else {agent_id: $who, agent_type: "loom-software-engineer"} end)')
	set +e
	OUTPUT=$(cd "$WORKTREE" && printf '%s' "$payload" |
		env -i PATH="$PATH" HOME="$TMP" LOOM_WORK_DIR="$WORK_DIR" LOOM_SESSION_ID="$SID" \
			${STAGE:+LOOM_STAGE_ID="$STAGE"} bash "$HOOK" 2>&1)
	RC=$?
	set -e
	[[ "$RC" == 0 ]] || fail "$tool $path was not allowed (rc $RC): $OUTPUT"
}

expect_warn() {
	edit "$@"
	[[ "$OUTPUT" == *'LOOM_HOOK_WARN: main-agent edit'* ]] || fail "no warning for $1 $2 ($3 lines): $OUTPUT"
	[[ "$OUTPUT" == *'at most 20 changed lines in at most 2 files'* && "$OUTPUT" == *'delegated'* ]] ||
		fail "the warning lacks the small-change test: $OUTPUT"
}

expect_quiet() {
	edit "$@"
	[[ -z "$OUTPUT" ]] || fail "unexpected output for $1 $2 ($3 lines): $OUTPUT"
}

STAGE=build-api

# 1. Distinct files: the first two are quiet, the third and later warn, once each.
SID=session-files
expect_quiet Edit src/a.rs 3
expect_quiet Write src/b.rs 5
expect_warn Edit src/c.rs 2
expect_quiet Edit src/c.rs 2
expect_warn MultiEdit src/d.rs 1
expect_quiet Edit src/b.rs 4

# 2. Size: more than 20 lines in one call warns once per path; 20 is quiet.
SID=session-size
expect_quiet Write src/twenty.rs 20
expect_warn Write src/big.rs 21
expect_quiet Write src/big.rs 40
SID=session-size-tools
expect_warn MultiEdit src/multi.rs 25
expect_warn NotebookEdit src/book.ipynb 25

# 3. doc/, *.md and distill scratch files neither warn nor count as files.
SID=session-exempt
expect_quiet Write doc/guide.txt 60
expect_quiet Write README.md 60
expect_quiet Write src/.kb_tmp_body 60
expect_quiet Write src/.distill-body-3 60
expect_quiet Edit src/e.rs 2
expect_quiet Edit src/f.rs 2

# 4. A subagent never warns and never fills the main agent's ledger.
SID=session-subagent
expect_quiet Write src/s1.rs 50 agent-1
expect_quiet Edit src/s2.rs 2 agent-1
expect_quiet Edit src/s3.rs 2 agent-1
expect_quiet Edit src/s4.rs 2 agent-2
expect_quiet Edit src/m1.rs 2
expect_quiet Edit src/m2.rs 2

# 5. Outside a stage session nothing warns.
STAGE=""
SID=session-no-stage
expect_quiet Write src/big.rs 50
STAGE=build-api

# 6. An unwritable ledger still warns and still allows.
SID=session-readonly
chmod 500 "$WORK_DIR"
expect_warn Write src/big.rs 30
chmod 700 "$WORK_DIR"

echo "PASS"
