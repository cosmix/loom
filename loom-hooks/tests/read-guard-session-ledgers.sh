#!/usr/bin/env bash
# read-guard-session-ledgers.sh - outside a stage the reads ledger is one
# directory per payload session_id with one file per agent_id ("main" for the
# main agent), so an interactive session's subagents see each other's
# whole-file reads the way stage agents do. The per-kind root is created 0700
# (loom's receipt store rejects an exposed $TMPDIR/loom-reads), a "."/".."
# session id cannot name a parent directory, and an unwritable ledger
# directory never turns into a non-zero exit or stderr.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HOOK="$SCRIPT_DIR/../read-guard.sh"

TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'chmod -R u+w "$TMP" 2>/dev/null; rm -rf "$TMP"' EXIT
mkdir -p "$TMP/tmp" "$TMP/files"
ROOT="$TMP/tmp/loom-reads"

fail() {
	echo "FAIL: $*"
	exit 1
}

BIG="$TMP/files/big.rs"
for ((i = 1; i <= 250; i++)); do printf 'line %d\n' "$i"; done >"$BIG"
touch -t 202001010000 "$BIG"

# read_as <session_id> <agent_id|""> [stage work dir] - run read-guard.sh on a
# whole-file Read of $BIG. Without a work dir no stage variable is set. An
# empty agent_id is omitted from the payload (the main agent). Prints
# stdout; fails the test on a non-zero exit or any stderr.
read_as() {
	local sid="$1" agent="$2" work="${3:-}" input out err code
	local -a stage=()
	[[ -n "$work" ]] && stage=(LOOM_WORK_DIR="$work" LOOM_SESSION_ID=sess-1 LOOM_STAGE_ID=stage-x)
	input=$(jq -nc --arg s "$sid" --arg a "$agent" --arg fp "$BIG" \
		'{tool_name:"Read",tool_input:{file_path:$fp},session_id:$s} + (if $a == "" then {} else {agent_id:$a} end)')
	set +e
	out=$(printf '%s' "$input" | env -i HOME="$HOME" PATH="$PATH" TMPDIR="$TMP/tmp" \
		LOOM_BIN="$TMP/no-loom" ${stage[@]+"${stage[@]}"} bash "$HOOK" 2>"$TMP/stderr")
	code=$?
	set -e
	err=$(<"$TMP/stderr")
	[[ $code -eq 0 ]] || fail "session $sid agent '$agent' exited $code: $err"
	[[ -z "$err" ]] || fail "session $sid agent '$agent' wrote stderr: $err"
	printf '%s' "$out"
}

mode_of() {
	stat -c '%a' "$1" 2>/dev/null || stat -f '%Lp' "$1"
}

# --- (5) outside a stage, two agent_ids in one session_id: warning -----------
[[ -z "$(read_as sess-out "")" ]] || fail "(5) the main agent's first whole read must be silent"
[[ -f "$ROOT/sess-out/main.tsv" ]] || fail "(5) main agent ledger not at $ROOT/sess-out/main.tsv"
OUT=$(read_as sess-out agent-b)
CTX=$(jq -r '.hookSpecificOutput.additionalContext // empty' <<<"$OUT")
[[ "$CTX" == *"was read whole by the main agent"* ]] || fail "(5) no sibling advisory outside a stage: $OUT"
[[ -f "$ROOT/sess-out/agent-b.tsv" ]] || fail "(5) subagent ledger not at $ROOT/sess-out/agent-b.tsv"
[[ -f "$ROOT/sess-out/_shared.tsv" ]] || fail "(5) _shared.tsv not in the session directory"
[[ "$(mode_of "$ROOT")" == "700" ]] || fail "(5) $ROOT is mode $(mode_of "$ROOT"), not 700"
[[ "$(mode_of "$ROOT/sess-out")" == "700" ]] || fail "(5) session directory is not mode 700"

# Another session never sees these reads.
[[ -z "$(read_as sess-other agent-c)" ]] || fail "(5) a different session_id must not see sess-out's reads"

# A ".." session id is not a directory component.
read_as .. agent-x >/dev/null
[[ -f "$ROOT/main/agent-x.tsv" ]] || fail "(5) session id '..' was not mapped to 'main'"
[[ ! -e "$TMP/tmp/agent-x.tsv" ]] || fail "(5) session id '..' escaped the ledger root"

# --- (6) unwritable ledger directory: exit 0, no stderr ----------------------
if [[ "$(id -u)" != "0" ]]; then
	LOCKED="$TMP/locked"
	mkdir -p "$LOCKED/hooks"
	chmod 500 "$LOCKED/hooks"
	[[ -z "$(read_as sess-x agent-a "$LOCKED")" ]] || fail "(6) an unwritable ledger root must stay silent"
	[[ ! -e "$LOCKED/hooks/reads" ]] || fail "(6) a ledger directory appeared under a read-only root"

	# A session directory whose files cannot be written (the hook re-chmods a
	# directory it owns, so read-only files stand in for another owner's): the
	# sibling advisory still prints, the failed ledger and _shared.tsv appends
	# stay quiet.
	STALE="$TMP/stale"
	mkdir -p "$STALE"
	read_as sess-x agent-a "$STALE" >/dev/null
	SDIR="$STALE/hooks/reads/sess-1"
	: >"$SDIR/_shared.tsv"
	: >"$SDIR/agent-b.tsv"
	chmod 400 "$SDIR/_shared.tsv" "$SDIR/agent-b.tsv"
	OUT=$(read_as sess-x agent-b "$STALE")
	jq -e '.hookSpecificOutput.additionalContext | contains("read whole by agent agent-a")' >/dev/null <<<"$OUT" ||
		fail "(6) no sibling advisory beside unwritable ledger files: $OUT"
	[[ ! -s "$SDIR/_shared.tsv" && ! -s "$SDIR/agent-b.tsv" ]] || fail "(6) a read-only ledger file was written"
fi

echo "PASS: out-of-stage ledgers are per session and per agent, private, contained, and failure-silent"
