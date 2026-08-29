#!/usr/bin/env bash
# Resident-token arithmetic: the count the governor works from is
# input_tokens + cache_creation_input_tokens + cache_read_input_tokens taken
# from the LAST assistant record carrying a usage block in the transcript
# tail. Observed here via the MAIN session's own heartbeat file, whose
# context_tokens field is written from this exact value
# (hooks/post-tool-use.sh: HB_CONTEXT_TOKENS_RAW="$RESIDENT_TOKENS" on the
# non-subagent path) - a direct, non-internal channel onto the number the
# governor actually computed.
#
# All transcripts here sit far below any ceiling (default 150000), so every
# case exits 0 and the only thing under test is the number landing in the
# heartbeat.
set -euo pipefail
HOOK="$(dirname "$0")/../post-tool-use.sh"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# usage_record <input> <cache_creation> <cache_read>
usage_record() {
	jq -nc --argjson i "$1" --argjson c "$2" --argjson r "$3" \
		'{type:"assistant",message:{usage:{input_tokens:$i,cache_creation_input_tokens:$c,cache_read_input_tokens:$r}}}'
}

# invoke <workdir> <transcript-path>
# Runs the hook for a plain Bash tool call. Sets CODE (not via a command
# substitution wrapper - a function invoked inside one runs in a subshell
# and any assignment to a bare, non-local variable is lost the moment it
# exits; calling this directly keeps CODE visible to the caller).
invoke() {
	local workdir="$1" transcript="$2"
	mkdir -p "$workdir"
	local input
	input=$(jq -nc --arg tp "$transcript" '{tool_name:"Bash",tool_input:{command:"echo hi"},transcript_path:$tp}')
	set +e
	printf '%s' "$input" |
		env -u LOOM_MAIN_AGENT_PID -u LOOM_WORKTREE_PATH \
			LOOM_STAGE_ID="test-stage" LOOM_SESSION_ID="test-session" LOOM_WORK_DIR="$workdir" \
			bash "$HOOK" >/dev/null 2>"$TMP/stderr"
	CODE=$?
	set -e
}

# --- E1: several assistant records, non-assistant records interleaved - the
# LAST assistant-with-usage record wins, not the first and not a sum of all.
WORK1="$TMP/work1"
TRANSCRIPT1="$TMP/t1.jsonl"
{
	printf '%s\n' '{"type":"user","message":{"content":"dummy first line, dropped by design"}}'
	usage_record 1000 100 11 # sum 1111
	printf '%s\n' '{"type":"user","message":{"content":"no usage field, must be filtered out"}}'
	usage_record 2000 200 22 # sum 2222
	usage_record 3000 300 33 # sum 3333 - last, must win
} >"$TRANSCRIPT1"
invoke "$WORK1" "$TRANSCRIPT1"
if [[ "$CODE" -ne 0 ]]; then
	echo "FAIL: (E1) expected exit 0, got $CODE"
	exit 1
fi
GOT1=$(jq -r '.context_tokens' "$WORK1/heartbeat/test-stage.json")
if [[ "$GOT1" != "3333" ]]; then
	echo "FAIL: (E1) expected the LAST record's sum 3333, got: $GOT1"
	exit 1
fi

# --- E2: a torn/unparseable final line must not crash the hook, and the
# prior VALID record's sum must still be used - never a wrong number.
WORK2="$TMP/work2"
TRANSCRIPT2="$TMP/t2.jsonl"
{
	printf '%s\n' '{"type":"user","message":{"content":"dummy first line, dropped by design"}}'
	usage_record 4000 200 42 # sum 4242 - last VALID record
	printf '%s\n' '{"type":"assistant","message":{"usage":{"input_to'
} >"$TRANSCRIPT2"
invoke "$WORK2" "$TRANSCRIPT2"
if [[ "$CODE" -ne 0 ]]; then
	echo "FAIL: (E2) a torn final line must not crash the hook, got exit $CODE"
	exit 1
fi
GOT2=$(jq -r '.context_tokens' "$WORK2/heartbeat/test-stage.json")
if [[ "$GOT2" != "4242" ]]; then
	echo "FAIL: (E2) expected the last VALID record's sum 4242 despite the torn line, got: $GOT2"
	exit 1
fi

# --- E3: no valid assistant record at all (everything past the dropped
# first line is garbage) - must not crash, and must not fabricate a number.
WORK3="$TMP/work3"
TRANSCRIPT3="$TMP/t3.jsonl"
{
	printf '%s\n' '{"type":"user","message":{"content":"dummy first line, dropped by design"}}'
	printf '%s\n' 'not even json'
} >"$TRANSCRIPT3"
invoke "$WORK3" "$TRANSCRIPT3"
if [[ "$CODE" -ne 0 ]]; then
	echo "FAIL: (E3) an all-garbage transcript must not crash the hook, got exit $CODE"
	exit 1
fi
GOT3=$(jq -r '.context_tokens' "$WORK3/heartbeat/test-stage.json")
if [[ "$GOT3" != "null" ]]; then
	echo "FAIL: (E3) expected context_tokens null when no usage can be determined, got: $GOT3"
	exit 1
fi

echo "PASS"
