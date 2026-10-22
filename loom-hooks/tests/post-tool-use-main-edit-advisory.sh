#!/usr/bin/env bash
# post-tool-use.sh prints one size advisory when a stage's main session changes
# more than 20 lines or more than 2 files itself (CLAUDE.md Rule 7, hard stop 6).
# The advisory is PostToolUse JSON on stdout, fires once per session, counts
# Edit/MultiEdit/Write/NotebookEdit, treats one trailing newline as no extra
# line, and stays silent for subagents, non-stage sessions and exempt paths
# (.loom/, doc/, *.md, the distill scratch names, "/../"). Each scenario uses
# its own session id so ledgers never mix. Mirrors loom/tests/guidance_gate_contracts.rs
# for the bash 3.2 and BSD-tool coverage that file cannot give.
set -euo pipefail
HOOK="$(cd "$(dirname "$0")/.." && pwd)/post-tool-use.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

WT="$TMP/wt"
mkdir -p "$TMP/bin" "$TMP/home" "$TMP/work" "$WT/src" "$WT/doc" "$WT/.loom"
printf '#!/bin/sh\nexit 0\n' >"$TMP/bin/loom"
chmod 755 "$TMP/bin/loom"

fail() {
	echo "FAIL: $1"
	exit 1
}

# text N - N lines joined by newlines, no trailing newline.
text() {
	local n="$1" i out=""
	for ((i = 1; i <= n; i++)); do
		out="${out}${out:+$'\n'}line $i"
	done
	printf '%s' "$out"
}

# run_hook SESSION TYPE PAYLOAD - sets OUT, ERR and CODE. The environment is
# rebuilt from scratch so no live loom session leaks in.
OUT="" ERR="" CODE=0
run_hook() {
	local session="$1" type="$2" payload="$3"
	CODE=0
	OUT=$(printf '%s' "$payload" | env -i PATH="$TMP/bin:$PATH" HOME="$TMP/home" \
		LOOM_STAGE_ID=advisory-stage LOOM_SESSION_ID="$session" LOOM_WORK_DIR="$TMP/work" \
		LOOM_SESSION_TYPE="$type" LOOM_WORKTREE_PATH="$WT" LOOM_BIN="$TMP/bin/loom" \
		bash "$HOOK" 2>"$TMP/stderr") || CODE=$?
	ERR=$(<"$TMP/stderr")
}

# payload TOOL INPUT_JSON [EXTRA_JSON] - a main-session payload: its transcript
# is `<session_id>.jsonl`, which loom_payload_agent_verdict reads as main.
payload() {
	local extra="${3:-}"
	[[ -n "$extra" ]] || extra='{}'
	jq -nc --arg tool "$1" --argjson input "$2" --argjson extra "$extra" --arg tp "$TMP/main-1.jsonl" \
		'{tool_name: $tool, session_id: "main-1", transcript_path: $tp, tool_input: $input} + $extra'
}

edit_input() {
	jq -nc --arg f "$1" --arg o "$2" --arg n "$3" '{file_path: $f, old_string: $o, new_string: $n}'
}

# edit SESSION TYPE FILE LINES [EXTRA_JSON] - an Edit whose new_string has LINES lines.
edit() {
	run_hook "$1" "$2" "$(payload Edit "$(edit_input "$3" x "$(text "$4")")" "${5:-}")"
}

expect_silent() {
	[[ "$CODE" == "0" ]] || fail "$1: exit code $CODE, stderr: $ERR"
	[[ -z "$OUT" ]] || fail "$1: expected no stdout, got: $OUT"
	[[ -z "$ERR" ]] || fail "$1: expected no stderr, got: $ERR"
}

expect_fires() {
	local what="$1" want="$2" context
	[[ "$CODE" == "0" ]] || fail "$what: exit code $CODE, stderr: $ERR"
	[[ -z "$ERR" ]] || fail "$what: the advisory belongs on stdout, stderr was: $ERR"
	printf '%s' "$OUT" | jq -e '.hookSpecificOutput.hookEventName == "PostToolUse"' >/dev/null 2>&1 \
		|| fail "$what: stdout is not PostToolUse JSON: $OUT"
	context=$(printf '%s' "$OUT" | jq -r '.hookSpecificOutput.additionalContext')
	[[ "$context" == *LOOM_HOOK_WARN* ]] || fail "$what: missing LOOM_HOOK_WARN: $context"
	[[ "$context" == *"$want"* ]] || fail "$what: expected '$want' in: $context"
}

# MultiEdit sums its edits: 12 + 12 new lines on one file.
two_edits=$(jq -nc --arg f "$WT/src/m.rs" --arg n "$(text 12)" \
	'{file_path: $f, edits: [{old_string: "x", new_string: $n}, {old_string: "x", new_string: $n}]}')
run_hook s-multi stage "$(payload MultiEdit "$two_edits")"
expect_fires "MultiEdit 12+12" "24 lines across 1 file(s)"

# An Edit counts the larger side of its replacement.
run_hook s-old stage "$(payload Edit "$(edit_input "$WT/src/o.rs" "$(text 25)" y)")"
expect_fires "Edit with a 25-line old_string" "25 lines across 1 file(s)"

# NotebookEdit carries notebook_path and new_source.
notebook=$(jq -nc --arg f "$WT/src/n.ipynb" --arg s "$(text 25)" '{notebook_path: $f, new_source: $s}')
run_hook s-nb stage "$(payload NotebookEdit "$notebook")"
expect_fires "NotebookEdit with a 25-line new_source" "25 lines across 1 file(s)"

# A trailing newline is not a line: 20 lines ending in "\n" stay at the limit,
# one more line crosses it.
write_20=$(jq -nc --arg f "$WT/src/w.rs" --arg c "$(text 20)"$'\n' '{file_path: $f, content: $c}')
run_hook s-boundary stage "$(payload Write "$write_20")"
expect_silent "Write of 20 lines ending in a newline"
edit s-boundary stage "$WT/src/w.rs" 1
expect_fires "one more line after a 20-line Write" "21 lines across 1 file(s)"

# Only a stage session counts.
for f in a b c; do
	edit s-merge merge "$WT/src/$f.rs" 3
	expect_silent "merge session edit of $f.rs"
done

# Exempt paths are silent and not tallied: a later 1-line code edit stays silent.
for f in "$WT/.loom/state.rs" "$WT/doc/x.rs" "$WT/src/notes.md" "$WT/.distill-body-1" \
	"$WT/src/.kb_tmp_x" "$WT/../outside.rs" "$WT/src/../lib.rs"; do
	edit s-exempt stage "$f" 25
	expect_silent "exempt path $f"
done
edit s-exempt stage "$WT/src/a.rs" 1
expect_silent "a 1-line code edit after only exempt paths"

# A subagent never prints the advisory and never feeds the main ledger. The
# first payload carries only agent_id (a main-shaped transcript), so the
# agent_id guard alone must stop it.
edit s-sub stage "$WT/src/a.rs" 3
expect_silent "s-sub main a.rs"
edit s-sub stage "$WT/src/b.rs" 3
expect_silent "s-sub main b.rs"
edit s-sub stage "$WT/src/c.rs" 25 '{"agent_id": "a1"}'
expect_silent "subagent edit by agent_id alone"
sub_tp="$TMP/main-1/subagents/agent-a1.jsonl"
edit s-sub stage "$WT/src/d.rs" 25 "$(jq -nc --arg tp "$sub_tp" '{agent_id: "a1", agent_type: "loom-software-engineer", transcript_path: $tp}')"
expect_silent "subagent edit by agent_type and transcript"
ledger="$TMP/work/hooks/mainedits/s-sub/main.tsv"
[[ -r "$ledger" ]] || fail "s-sub: the main ledger $ledger was not written"
ledger_text=$(<"$ledger")
[[ "$ledger_text" != *warned* ]] || fail "s-sub: a subagent wrote the warned row: $ledger_text"
[[ "$ledger_text" != *c.rs* && "$ledger_text" != *d.rs* ]] || fail "s-sub: a subagent edit was tallied: $ledger_text"
edit s-sub stage "$WT/src/e.rs" 1
expect_fires "main edit after the subagent edits" "7 lines across 3 file(s)"

# Once warned, the session stays silent.
edit s-sub stage "$WT/src/f.rs" 1
expect_silent "edit after the warning"

# A Write outside the worktree is not the worktree's change.
outside=$(jq -nc --arg f "$TMP/scratch.rs" --arg c "$(text 100)" '{file_path: $f, content: $c}')
run_hook s-out stage "$(payload Write "$outside")"
expect_silent "Write outside the worktree"

echo "PASS"
