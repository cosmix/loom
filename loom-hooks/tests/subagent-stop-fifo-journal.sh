#!/usr/bin/env bash
# A FIFO planted at a journal path must not block the SubagentStop hook. Opening
# a FIFO for append waits for a reader, so a hook that only rejects symlinks
# hangs the session's stop on stop-skips.jsonl, and loom_lifecycle_append must
# refuse a FIFO lifecycle.jsonl. Both runs sit under `timeout`, so a regression
# fails (exit 124) instead of hanging the suite.
set -euo pipefail

unset LOOM_STAGE_ID LOOM_SESSION_ID LOOM_WORK_DIR LOOM_WORKTREE_PATH LOOM_MAIN_AGENT_PID \
	LOOM_SESSION_TYPE LOOM_HOOK_PATH LOOM_HOOK_DEBUG LOOM_HOOK_CONTEXT LOOM_BIN

TIMEOUT=$(command -v timeout || command -v gtimeout || true)
if [[ -z "$TIMEOUT" ]]; then
	echo "skip: neither timeout nor gtimeout is installed, a blocked hook would hang"
	exit 0
fi

HOOKS_DIR="$(cd "$(dirname "$0")/.." && pwd)"
HOOK="$HOOKS_DIR/subagent-stop.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/subagent-stop-fifo.XXXXXX")
trap 'rm -rf -- "$TMP"' EXIT
# The hook refuses paths that cross a symlink, so use the physical path.
TMP=$(cd "$TMP" && pwd -P)

WORKDIR="$TMP/work"
STAGE_ID="test-stage"
SESSION_ID="test-session"
PARENT_SESSION_ID="parent-session"
AGENT_ID="reviewer-fifo"
JOURNAL_DIR="$WORKDIR/subagents/$STAGE_ID"
SKIPS="$JOURNAL_DIR/stop-skips.jsonl"
PROJECT="$TMP/claude-project"
PARENT_TRANSCRIPT="$PROJECT/${PARENT_SESSION_ID}.jsonl"
WORKER_TRANSCRIPT="$PROJECT/$PARENT_SESSION_ID/subagents/agent-${AGENT_ID}.jsonl"
mkdir -p "$WORKDIR/stages" "$JOURNAL_DIR" "$PROJECT/$PARENT_SESSION_ID/subagents"
printf '%s\n' '---' "id: $STAGE_ID" "session: $SESSION_ID" '---' \
	>"$WORKDIR/stages/01-${STAGE_ID}.md"
printf '%s\n' '{"type":"parent"}' >"$PARENT_TRANSCRIPT"
printf '%s\n' '{"type":"assistant"}' >"$WORKER_TRANSCRIPT"

# One reviewer stop with no SubagentStart row: the hook skips it and records the
# skip in stop-skips.jsonl. Sets HOOK_STATUS (124 when the hook blocked).
run_reviewer_stop() {
	HOOK_STATUS=0
	jq -nc --arg agent_id "$AGENT_ID" --arg session_id "$PARENT_SESSION_ID" \
		--arg transcript_path "$PARENT_TRANSCRIPT" --arg agent_transcript_path "$WORKER_TRANSCRIPT" \
		'{session_id:$session_id,hook_event_name:"SubagentStop",agent_id:$agent_id,
		  agent_type:"loom-code-reviewer",transcript_path:$transcript_path,
		  agent_transcript_path:$agent_transcript_path}' |
		LOOM_WORK_DIR="$WORKDIR" LOOM_STAGE_ID="$STAGE_ID" LOOM_SESSION_ID="$SESSION_ID" \
			"$TIMEOUT" 15 bash "$HOOK" >"$TMP/stdout" 2>"$TMP/stderr" || HOOK_STATUS=$?
}

# Control: the same stop with no obstacle appends exactly one skip row, so the
# FIFO run below reaches the append and is not skipped earlier for another reason.
run_reviewer_stop
if [[ "$HOOK_STATUS" != "0" ]] || ! jq -es 'length == 1 and .[0].reason == "no_unambiguous_start_row"' \
	"$SKIPS" >/dev/null 2>&1; then
	echo "FAIL: control run: expected exit 0 and one stop-skips.jsonl row, got exit $HOOK_STATUS"
	cat "$SKIPS" "$TMP/stdout" "$TMP/stderr" 2>/dev/null || true
	exit 1
fi

rm -f "$SKIPS"
mkfifo "$SKIPS"
run_reviewer_stop
if [[ "$HOOK_STATUS" == "124" ]]; then
	echo "FAIL: a FIFO at stop-skips.jsonl blocked the hook past its timeout"
	exit 1
fi
if [[ "$HOOK_STATUS" != "0" || -s "$TMP/stdout" || -s "$TMP/stderr" || ! -p "$SKIPS" ]]; then
	echo "FAIL: with a FIFO at stop-skips.jsonl the hook must exit 0 quietly and leave it alone, got exit $HOOK_STATUS"
	cat "$TMP/stdout" "$TMP/stderr"
	exit 1
fi

# The lifecycle journal: a FIFO there is refused by both the readiness check and
# the append, and neither waits for a reader.
JOURNAL="$JOURNAL_DIR/lifecycle.jsonl"
mkfifo "$JOURNAL"
for check in 'loom_lifecycle_journal_ready "$2"' \
	'loom_lifecycle_append "$3" "$4" "$5" "{}" test'; do
	CHECK_STATUS=0
	"$TIMEOUT" 15 bash -c 'source "$1/_lifecycle.sh" && '"$check" _ "$HOOKS_DIR" "$JOURNAL" \
		"$WORKDIR" "$STAGE_ID" "$SESSION_ID" >/dev/null 2>&1 || CHECK_STATUS=$?
	if [[ "$CHECK_STATUS" == "124" ]]; then
		echo "FAIL: '$check' waited on a FIFO lifecycle.jsonl"
		exit 1
	fi
	if [[ "$CHECK_STATUS" == "0" ]]; then
		echo "FAIL: '$check' accepted a FIFO lifecycle.jsonl"
		exit 1
	fi
done

echo "PASS"
