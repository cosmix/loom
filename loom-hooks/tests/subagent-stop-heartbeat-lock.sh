#!/usr/bin/env bash
# A SubagentStop refresh must read the parent's heartbeat only after it owns
# the shared heartbeat lock. Otherwise it can read 100, the parent can write
# 150, and the late subagent write can roll the file back to 100.
set -euo pipefail

HOOK="$(dirname "$0")/../subagent-stop.sh"
TMP=""
TMP=$(mktemp -d "${TMPDIR:-/tmp}/subagent-stop-lock.XXXXXX") && [[ -n "$TMP" ]] || {
	echo "FAIL: could not create scratch directory"
	exit 1
}
trap '[[ -n "${TMP:-}" ]] && rm -rf -- "$TMP"' EXIT

WORKDIR="$TMP/work"
STAGE_ID="test-stage"
SESSION_ID="test-session"
AGENT_ID="agent-1"
AGENT_TYPE="worker"
PARENT_SESSION_ID="parent-session"
HEARTBEAT_DIR="$WORKDIR/heartbeat"
HEARTBEAT="$HEARTBEAT_DIR/${STAGE_ID}.json"
LOCK_DIR="${HEARTBEAT}.lock"
PARENT_TRANSCRIPT="$TMP/claude-project/${PARENT_SESSION_ID}.jsonl"
WORKER_TRANSCRIPT="$TMP/claude-project/${PARENT_SESSION_ID}/subagents/agent-${AGENT_ID}.jsonl"
mkdir -p "$HEARTBEAT_DIR" "$WORKDIR/stages" "$WORKDIR/subagents/$STAGE_ID" \
	"${WORKER_TRANSCRIPT%/*}"
printf '%s\n' '---' "id: $STAGE_ID" "session: $SESSION_ID" '---' \
	>"$WORKDIR/stages/01-${STAGE_ID}.md"
printf '%s\n' '{"type":"parent","message":"waiting"}' >"$PARENT_TRANSCRIPT"
printf '%s\n' '{"type":"assistant","message":"done"}' >"$WORKER_TRANSCRIPT"
jq -nc --arg agent_id "$AGENT_ID" --arg agent_type "$AGENT_TYPE" \
	--arg stage_id "$STAGE_ID" --arg parent_session_id "$PARENT_SESSION_ID" \
	--arg loom_session_id "$SESSION_ID" \
	'{agent_id:$agent_id,agent_type:$agent_type,stage_id:$stage_id,
	  parent_session_id:$parent_session_id,loom_session_id:$loom_session_id,
	  ts:"2000-01-01T00:00:00.000Z"}' >"$WORKDIR/subagents/$STAGE_ID/starts.jsonl"

write_parent_heartbeat() {
	local tokens="$1" stamp="$2"
	jq -n \
		--arg stage_id "$STAGE_ID" \
		--arg session_id "$SESSION_ID" \
		--arg timestamp "$stamp" \
		--arg transcript_path "$TMP/parent.jsonl" \
		--argjson tokens "$tokens" \
		'{stage_id:$stage_id,session_id:$session_id,timestamp:$timestamp,
		  context_tokens:$tokens,transcript_path:$transcript_path,last_tool:"Bash",activity:"parent"}' \
		>"$HEARTBEAT"
}

write_parent_heartbeat 100 "2026-08-30T00:00:00.000Z"
mkdir -m 700 "$LOCK_DIR"

INPUT=$(jq -nc \
	--arg agent_id "$AGENT_ID" \
	--arg agent_type "$AGENT_TYPE" \
	--arg session_id "$PARENT_SESSION_ID" \
	--arg transcript_path "$PARENT_TRANSCRIPT" \
	--arg agent_transcript_path "$WORKER_TRANSCRIPT" \
	'{session_id:$session_id,hook_event_name:"SubagentStop",agent_id:$agent_id,
	  agent_type:$agent_type,transcript_path:$transcript_path,
	  agent_transcript_path:$agent_transcript_path}')

printf '%s' "$INPUT" |
	env LOOM_WORK_DIR="$WORKDIR" LOOM_STAGE_ID="$STAGE_ID" LOOM_SESSION_ID="$SESSION_ID" \
		bash "$HOOK" >"$TMP/stdout" 2>"$TMP/stderr" &
HOOK_PID=$!

# With locking, the hook is still waiting and cannot have read the old value.
sleep 0.1
if ! kill -0 "$HOOK_PID" 2>/dev/null; then
	echo "FAIL: SubagentStop did not wait for the heartbeat lock"
	exit 1
fi

# This stands in for the parent writer that owns the lock.
write_parent_heartbeat 150 "2026-08-30T00:01:00.000Z"
rmdir "$LOCK_DIR"
wait "$HOOK_PID"

GOT=$(jq -r '.context_tokens' "$HEARTBEAT")
if [[ "$GOT" != "150" ]]; then
	echo "FAIL: late subagent refresh rolled parent tokens back: expected 150, got $GOT"
	cat "$HEARTBEAT"
	exit 1
fi

JOURNAL="$WORKDIR/subagents/$STAGE_ID/lifecycle.jsonl"
if [[ ! -f "$JOURNAL" ]] || [[ "$(wc -l <"$JOURNAL")" != "1" ]]; then
	echo "FAIL: lifecycle journal line was not written exactly once"
	exit 1
fi
TRANSCRIPT_BYTES=$(wc -c <"$WORKER_TRANSCRIPT")
TRANSCRIPT_BYTES=${TRANSCRIPT_BYTES//[[:space:]]/}
FINAL_HEX=$(printf '%s' '{"type":"assistant","message":"done"}' | sha256sum)
FINAL_DIGEST="sha256:${FINAL_HEX%% *}"
EVENT_HEX=$(printf 'loom.lifecycle.claude_subagent_stop.v1\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s' \
	"$STAGE_ID" "$SESSION_ID" "$PARENT_SESSION_ID" "$AGENT_ID" "$AGENT_TYPE" \
	"$WORKER_TRANSCRIPT" "$TRANSCRIPT_BYTES" "$FINAL_DIGEST" | sha256sum)
EVENT_ID="sha256:${EVENT_HEX%% *}"
if ! jq -e --arg event_id "$EVENT_ID" --arg stage "$STAGE_ID" --arg session "$SESSION_ID" \
	--arg parent "$PARENT_SESSION_ID" --arg agent "$AGENT_ID" --arg agent_type "$AGENT_TYPE" \
	--arg transcript "$WORKER_TRANSCRIPT" --arg digest "$FINAL_DIGEST" \
	--argjson bytes "$TRANSCRIPT_BYTES" '
	.version == 1 and .event_id == $event_id and .producer == "claude_subagent_stop" and
	.identity == {kind:"claude_subagent",stage_id:$stage,loom_session_id:$session,
	 parent_session_id:$parent,agent_id:$agent,agent_type:$agent_type,transcript_path:$transcript} and
	.state == "completed" and (.observed_at | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T")) and
	.evidence == {transcript_bytes:$bytes,final_record_sha256:$digest}' "$JOURNAL" >/dev/null; then
	echo "FAIL: lifecycle journal line does not match the trusted stop contract"
	cat "$JOURNAL"
	exit 1
fi

echo "PASS"
