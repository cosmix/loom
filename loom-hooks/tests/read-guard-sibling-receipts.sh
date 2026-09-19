#!/usr/bin/env bash
# read-guard-sibling-receipts.sh - in a stage session, a whole-file Read of a
# file above 200 lines that a sibling agent already read whole, unchanged
# since, is advised once per agent and path and recorded in the session
# directory's _shared.tsv. A modified file, a sibling's ranged read, and a
# 150-line file stay silent. Writer and reader are both the real hook, so the
# ledger addresses come from _loom_ledger_file itself.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HOOK="$SCRIPT_DIR/../read-guard.sh"

TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
WORK="$TMP/work"
mkdir -p "$WORK" "$TMP/tmp" "$TMP/files"
SESSION_DIR="$WORK/hooks/reads/sess-1"
SHARED="$SESSION_DIR/_shared.tsv"

fail() {
	echo "FAIL: $*"
	exit 1
}

# make_file <name> <lines> - a text file whose mtime predates every ledger row.
make_file() {
	local path="$TMP/files/$1" i
	: >"$path"
	for ((i = 1; i <= $2; i++)); do printf 'line %d\n' "$i" >>"$path"; done
	touch -t 202001010000 "$path"
	printf '%s' "$path"
}

# read_as <agent_id> <file> [offset limit] - run read-guard.sh as one agent of
# stage session sess-1 with no live loom main agent (every decision is a
# warning) and no loom binary (no outline, no receipt proof). Prints stdout;
# fails the test on a non-zero exit or any stderr.
read_as() {
	local agent="$1" file="$2" input out err code
	if (($# == 4)); then
		input=$(jq -nc --arg a "$agent" --arg fp "$file" --argjson o "$3" --argjson l "$4" \
			'{tool_name:"Read",tool_input:{file_path:$fp,offset:$o,limit:$l},agent_id:$a,session_id:"payload-sess"}')
	else
		input=$(jq -nc --arg a "$agent" --arg fp "$file" \
			'{tool_name:"Read",tool_input:{file_path:$fp},agent_id:$a,session_id:"payload-sess"}')
	fi
	set +e
	out=$(printf '%s' "$input" | env -i HOME="$HOME" PATH="$PATH" TMPDIR="$TMP/tmp" \
		LOOM_WORK_DIR="$WORK" LOOM_SESSION_ID=sess-1 LOOM_STAGE_ID=stage-x \
		LOOM_BIN="$TMP/no-loom" bash "$HOOK" 2>"$TMP/stderr")
	code=$?
	set -e
	err=$(<"$TMP/stderr")
	[[ $code -eq 0 ]] || fail "$agent reading $file exited $code: $err"
	[[ -z "$err" ]] || fail "$agent reading $file wrote stderr: $err"
	printf '%s' "$out"
}

context_of() {
	jq -r '.hookSpecificOutput.additionalContext // empty' <<<"$1"
}

# --- (1) sibling full read, unchanged file: one warning, then silent --------
BIG=$(make_file big.rs 250)
[[ -z "$(read_as agent-a "$BIG")" ]] || fail "(1) first whole read must be silent"
OUT=$(read_as agent-b "$BIG")
CTX=$(context_of "$OUT")
[[ "$CTX" == *"$BIG (250 lines) was read whole by agent agent-a"* ]] || fail "(1) no sibling advisory: $OUT"
[[ "$CTX" == *"unchanged since"* ]] || fail "(1) advisory does not say unchanged: $CTX"
[[ "$CTX" == *"loom map --outline $BIG"* ]] || fail "(1) advisory lacks the outline move: $CTX"
[[ "$CTX" == *"offset/limit"* ]] || fail "(1) advisory lacks the ranged Read: $CTX"
[[ -z "$(read_as agent-b "$BIG")" ]] || fail "(1) second attempt by the same agent must be silent"

[[ -f "$SHARED" ]] || fail "(1) $SHARED was not written"
IFS=$'\t' read -r S_PATH S_LINES S_COUNT S_TS <"$SHARED"
[[ "$S_PATH" == "$BIG" && "$S_LINES" == "250" && "$S_COUNT" == "2" && -n "$S_TS" ]] ||
	fail "(1) _shared.tsv row is not path/lines/count/timestamp: $(<"$SHARED")"

OUT=$(read_as agent-c "$BIG")
[[ "$(context_of "$OUT")" == *"and 1 other agent(s)"* ]] || fail "(1) third agent not told about both readers: $OUT"
[[ "$(wc -l <"$SHARED" | tr -d '[:space:]')" == "2" ]] || fail "(1) expected 2 _shared.tsv rows: $(<"$SHARED")"
[[ "$(tail -n 1 "$SHARED" | cut -f3)" == "3" ]] || fail "(1) latest _shared.tsv row must count 3 agents"

# The reads ledger keeps its four-column row shape.
ROW=$(head -n 1 "$SESSION_DIR/agent-a.tsv")
[[ "$(awk -F '\t' '{ print NF }' <<<"$ROW")" == "4" ]] || fail "(1) ledger row shape changed: $ROW"

# --- (2) file modified after the sibling's read: silent ----------------------
CHANGED=$(make_file changed.rs 250)
[[ -z "$(read_as agent-a "$CHANGED")" ]] || fail "(2) first whole read must be silent"
printf 'one more\n' >>"$CHANGED"
touch -t 203001010000 "$CHANGED"
[[ -z "$(read_as agent-b "$CHANGED")" ]] || fail "(2) a file modified after the sibling's read must be silent"

# --- (3) sibling ranged read: silent -----------------------------------------
RANGED=$(make_file ranged.rs 250)
[[ -z "$(read_as agent-a "$RANGED" 0 50)" ]] || fail "(3) ranged read must be silent"
[[ -z "$(read_as agent-b "$RANGED")" ]] || fail "(3) a sibling's ranged read must not qualify"

# --- (4) a 150-line file: silent ---------------------------------------------
SMALL=$(make_file small.rs 150)
[[ -z "$(read_as agent-a "$SMALL")" ]] || fail "(4) first whole read must be silent"
[[ -z "$(read_as agent-b "$SMALL")" ]] || fail "(4) a file under the 200-line floor must be silent"

[[ "$(wc -l <"$SHARED" | tr -d '[:space:]')" == "2" ]] || fail "silent cases must not add _shared.tsv rows: $(<"$SHARED")"

echo "PASS: sibling whole-file reads are advised once and recorded in _shared.tsv; changed, ranged and small files stay silent"
