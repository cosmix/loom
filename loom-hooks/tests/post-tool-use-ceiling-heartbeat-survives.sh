#!/usr/bin/env bash
# The heartbeat write must happen BEFORE the ceiling check, so a ceiling
# `exit 2` never preempts it. This is the invariant that matters most: if a
# session at its ceiling stops heartbeating, the daemon reads its silence as
# HUNG rather than as "the governor is speaking to the agent" - exactly
# backwards from what should happen at the moment the governor engages.
#
# Covers both severities the governor's exit 2 fires for: the 100% hard
# block and the 80% warning. Both must still leave a fresh heartbeat with the
# resident count recorded.
set -euo pipefail
HOOK="$(dirname "$0")/../post-tool-use.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

# assert_heartbeat_survives_ceiling <desc> <workdir-name> <ceiling> <resident>
assert_heartbeat_survives_ceiling() {
	local desc="$1" workdir_name="$2" ceiling="$3" resident="$4"
	local workdir="$TMP/$workdir_name"
	mkdir -p "$workdir/heartbeat"
	printf '%s:%s\n' "$ceiling" '120000' \
		>"$workdir/heartbeat/test-stage.test-session.context-ceilings"

	local transcript="$workdir/transcript.jsonl"
	{
		printf '%s\n' '{"type":"user","message":{"content":"dummy first line, dropped by design"}}'
		jq -nc --argjson n "$resident" \
			'{type:"assistant",message:{usage:{input_tokens:$n,cache_creation_input_tokens:0,cache_read_input_tokens:0}}}'
	} >"$transcript"

	local input
	input=$(jq -nc --arg tp "$transcript" '{tool_name:"Bash",tool_input:{command:"echo hi"},transcript_path:$tp}')

	set +e
	local stderr_out
	stderr_out=$(printf '%s' "$input" |
		env -u LOOM_MAIN_AGENT_PID -u LOOM_WORKTREE_PATH \
			LOOM_STAGE_ID="test-stage" LOOM_SESSION_ID="test-session" LOOM_WORK_DIR="$workdir" \
			bash "$HOOK" 2>&1 1>/dev/null)
	local code=$?
	set -e

	if [[ $code -ne 2 ]]; then
		echo "FAIL: $desc - expected the ceiling check to fire (exit 2), got $code (stderr: $stderr_out)"
		exit 1
	fi

	local heartbeat="$workdir/heartbeat/test-stage.json"
	if [[ ! -f "$heartbeat" ]]; then
		echo "FAIL: $desc - exit 2 from the ceiling check preempted the heartbeat write entirely"
		exit 1
	fi

	local got_tokens
	got_tokens=$(jq -r '.context_tokens' "$heartbeat")
	if [[ "$got_tokens" != "$resident" ]]; then
		echo "FAIL: $desc - heartbeat exists but context_tokens is wrong: expected $resident, got $got_tokens"
		exit 1
	fi
}

assert_heartbeat_survives_ceiling "100% hard block" "work-hard-block" 1000 1000
assert_heartbeat_survives_ceiling "80% warning" "work-warn" 1000 850

echo "PASS"
