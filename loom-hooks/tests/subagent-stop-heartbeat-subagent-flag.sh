#!/usr/bin/env bash
# The SubagentStop lifecycle heartbeat records an event ABOUT a subagent, not
# a main-agent tool call, so it must tag subagent:true and carry no last_tool.
# Otherwise the daemon's stale-input-wait reconciler
# (orchestrator/monitor/input_wait.rs) reads it as main-agent progress and
# resumes a stage that is genuinely waiting on a person.
set -euo pipefail

HOOK="$(dirname "$0")/../subagent-stop.sh"
TMP=""
TMP=$(mktemp -d "${TMPDIR:-/tmp}/subagent-stop-flag.XXXXXX") && [[ -n "$TMP" ]] || {
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
		bash "$HOOK"

if [[ ! -f "$HEARTBEAT" ]]; then
	echo "FAIL: SubagentStop did not write a heartbeat"
	exit 1
fi

if ! jq -e '.subagent == true' "$HEARTBEAT" >/dev/null 2>&1; then
	echo "FAIL: SubagentStop heartbeat did not tag subagent:true: $(cat "$HEARTBEAT")"
	exit 1
fi

if ! jq -e '.last_tool == null' "$HEARTBEAT" >/dev/null 2>&1; then
	echo "FAIL: SubagentStop heartbeat should carry no last_tool: $(cat "$HEARTBEAT")"
	exit 1
fi

echo "PASS"
