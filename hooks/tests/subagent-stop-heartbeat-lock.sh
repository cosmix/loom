#!/usr/bin/env bash
# A SubagentStop refresh must read the parent's heartbeat only after it owns
# the shared heartbeat lock. Otherwise it can read 100, the parent can write
# 150, and the late subagent write can roll the file back to 100.
set -euo pipefail

HOOK="$(dirname "$0")/../subagent-stop.sh"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

WORKDIR="$TMP/work"
STAGE_ID="test-stage"
SESSION_ID="test-session"
AGENT_ID="agent-1"
HEARTBEAT_DIR="$WORKDIR/heartbeat"
HEARTBEAT="$HEARTBEAT_DIR/${STAGE_ID}.json"
LOCK_DIR="${HEARTBEAT}.lock"
mkdir -p "$HEARTBEAT_DIR"

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
	--arg transcript_path "$TMP/subagents/agent-${AGENT_ID}.jsonl" \
	'{agent_id:$agent_id,transcript_path:$transcript_path}')

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

if [[ ! -f "$WORKDIR/subagents/${STAGE_ID}/${AGENT_ID}.json" ]]; then
	echo "FAIL: completion record was not written"
	exit 1
fi

echo "PASS"
