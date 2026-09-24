#!/usr/bin/env bash
# Trusted Claude Code SubagentStop lifecycle producer. Invalid, stale, or
# unsafe evidence is explained only when LOOM_HOOK_DEBUG=1 and always skipped.

set -uo pipefail
umask 077

source "$(dirname "${BASH_SOURCE[0]}")/_lifecycle.sh"
source "$(dirname "${BASH_SOURCE[0]}")/_read_ledger.sh"

HOOK_NAME="subagent-stop"
REVIEWER_AGENT_TYPE="loom-code-reviewer"

# Hand a code reviewer's stop event to `loom hook review-harvest`, which records
# its final report as a review round (DESIGN D12). Best effort: the delegate's
# output and exit status never reach this hook's own; failures are logged only
# under LOOM_HOOK_DEBUG=1.
loom_subagent_stop_review_harvest() {
	local input="" output="" status=0
	command -v "${LOOM_BIN:-loom}" &>/dev/null || {
		loom_debug "$HOOK_NAME: review harvest skipped - loom is not on PATH"
		return 0
	}
	input=$(jq -nc --arg stage_id "$LOOM_STAGE_ID" --arg session_id "$PARENT_SESSION_ID" \
		--arg agent_id "$AGENT_ID" --arg transcript_path "$WORKER_TRANSCRIPT" \
		'{stage_id:$stage_id,session_id:$session_id,agent_id:$agent_id,
		  transcript_path:$transcript_path}' 2>/dev/null) || status=$?
	if ((status != 0)) || [[ -z "$input" ]]; then
		loom_lifecycle_log_jq_failure "$HOOK_NAME" "constructing the review harvest input" "$status"
		return 0
	fi
	output=$(printf '%s' "$input" | LOOM_HOOK_CONTEXT=1 LOOM_WORK_DIR="$WORK_DIR" \
		loom_run_bounded 10 "${LOOM_BIN:-loom}" hook review-harvest 2>&1 >/dev/null) || status=$?
	if ((status != 0)); then
		loom_debug "$HOOK_NAME: review harvest failed (exit $status): $output"
	elif [[ -n "$output" ]]; then
		loom_debug "$HOOK_NAME: review harvest: $output"
	fi
	return 0
}

if [[ -z "${LOOM_STAGE_ID:-}" ]]; then
	loom_debug "$HOOK_NAME: skipping - LOOM_STAGE_ID unset (not a loom session)"
	exit 0
fi
if [[ -z "${LOOM_SESSION_ID:-}" || -z "${LOOM_WORK_DIR:-}" ]]; then
	loom_debug "$HOOK_NAME: skipping - LOOM_SESSION_ID or LOOM_WORK_DIR unset"
	exit 0
fi
if ! loom_lifecycle_safe_id "$LOOM_STAGE_ID" ||
	! loom_lifecycle_safe_id "$LOOM_SESSION_ID"; then
	loom_debug "$HOOK_NAME: skipping - unsafe stage or Loom session id"
	exit 0
fi
if ! command -v jq &>/dev/null || ! command -v sha256sum &>/dev/null; then
	loom_debug "$HOOK_NAME: skipping - jq or sha256sum unavailable"
	exit 0
fi

INPUT_JSON=$(loom_lifecycle_read_input)
if printf '%s' "$INPUT_JSON" | jq -e '
	type == "object" and
	([.session_id, .agent_id, .agent_type, .transcript_path,
	  .agent_transcript_path] | all(type == "string" and length > 0 and
	  index("\u0000") == null and (test("[\\r\\n]") | not)))
' >/dev/null 2>&1; then
	:
else
	JQ_STATUS=$?
	if ((JQ_STATUS == 2 || JQ_STATUS == 3)); then
		loom_lifecycle_log_jq_failure "$HOOK_NAME" "validating the hook payload" "$JQ_STATUS"
	else
		loom_debug "$HOOK_NAME: skipping - malformed payload or missing required string"
	fi
	exit 0
fi

PARENT_SESSION_ID=$(loom_lifecycle_jq_value "$INPUT_JSON" '.session_id' "$HOOK_NAME" \
	"extracting session_id") || exit 0
AGENT_ID=$(loom_lifecycle_jq_value "$INPUT_JSON" '.agent_id' "$HOOK_NAME" \
	"extracting agent_id") || exit 0
AGENT_TYPE=$(loom_lifecycle_jq_value "$INPUT_JSON" '.agent_type' "$HOOK_NAME" \
	"extracting agent_type") || exit 0
PARENT_TRANSCRIPT=$(loom_lifecycle_jq_value "$INPUT_JSON" '.transcript_path' "$HOOK_NAME" \
	"extracting transcript_path") || exit 0
WORKER_TRANSCRIPT=$(loom_lifecycle_jq_value "$INPUT_JSON" '.agent_transcript_path' "$HOOK_NAME" \
	"extracting agent_transcript_path") || exit 0

if ! loom_lifecycle_safe_id "$PARENT_SESSION_ID" ||
	! loom_lifecycle_safe_id "$AGENT_ID" ||
	! loom_lifecycle_safe_id "$AGENT_TYPE"; then
	loom_debug "$HOOK_NAME: skipping - unsafe parent, agent, or type identity"
	exit 0
fi
if ! WORK_DIR=$(loom_lifecycle_resolve_work_root "$LOOM_WORK_DIR"); then
	loom_debug "$HOOK_NAME: skipping - work directory is unavailable or unsafe"
	exit 0
fi
if ! loom_lifecycle_stage_binding "$WORK_DIR" "$LOOM_STAGE_ID" "$LOOM_SESSION_ID"; then
	loom_debug "$HOOK_NAME: skipping - stage is not bound to Loom session $LOOM_SESSION_ID"
	exit 0
fi
if ! loom_lifecycle_plain_path "$PARENT_TRANSCRIPT" file ||
	! loom_lifecycle_plain_path "$WORKER_TRANSCRIPT" file; then
	loom_debug "$HOOK_NAME: skipping - parent or worker transcript is not a plain normalized file"
	exit 0
fi

WORKER_NAME=${WORKER_TRANSCRIPT##*/}
WORKER_SUBAGENTS=${WORKER_TRANSCRIPT%/*}
WORKER_SESSION=${WORKER_SUBAGENTS%/*}
WORKER_PROJECT=${WORKER_SESSION%/*}
if [[ "$WORKER_NAME" != "agent-${AGENT_ID}.jsonl" ||
	"${WORKER_SUBAGENTS##*/}" != "subagents" ||
	"${WORKER_SESSION##*/}" != "$PARENT_SESSION_ID" ||
	"$PARENT_TRANSCRIPT" != "$WORKER_PROJECT/${PARENT_SESSION_ID}.jsonl" ]]; then
	loom_debug "$HOOK_NAME: skipping - transcript layout, parent UUID, or agent id differs"
	exit 0
fi

OBSERVED_AT=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z" 2>/dev/null || true)
if [[ -z "$OBSERVED_AT" ]]; then
	loom_debug "$HOOK_NAME: skipping - UTC timestamp unavailable"
	exit 0
fi
START_STATUS=0
loom_lifecycle_resolve_start "$WORK_DIR" "$LOOM_STAGE_ID" \
	"$PARENT_SESSION_ID" "$LOOM_SESSION_ID" "$AGENT_ID" \
	"$AGENT_TYPE" "$OBSERVED_AT" "$HOOK_NAME" || START_STATUS=$?
if ((START_STATUS != 0)); then
	if ((START_STATUS == 1)); then
		loom_debug "$HOOK_NAME: skipping - no unambiguous exact SubagentStart row"
	fi
	exit 0
fi
TRANSCRIPT_STATUS=0
loom_lifecycle_transcript_evidence "$WORKER_TRANSCRIPT" "$HOOK_NAME" || TRANSCRIPT_STATUS=$?
if ((TRANSCRIPT_STATUS != 0)); then
	if ((TRANSCRIPT_STATUS == 1)); then
		loom_debug "$HOOK_NAME: skipping - worker transcript is empty, torn, malformed, or changing"
	fi
	exit 0
fi

if ! EVENT_HEX=$(printf 'loom.lifecycle.claude_subagent_stop.v1\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s' \
	"$LOOM_STAGE_ID" "$LOOM_SESSION_ID" "$PARENT_SESSION_ID" "$AGENT_ID" \
	"$AGENT_TYPE" "$WORKER_TRANSCRIPT" "$LIFECYCLE_TRANSCRIPT_BYTES" \
	"$LIFECYCLE_FINAL_DIGEST" | sha256sum 2>/dev/null); then
	loom_debug "$HOOK_NAME: skipping - event id digest failed"
	exit 0
fi
EVENT_HEX=${EVENT_HEX%% *}
if [[ ! "$EVENT_HEX" =~ ^[0-9a-f]{64}$ ]]; then
	loom_debug "$HOOK_NAME: skipping - sha256sum returned an invalid digest"
	exit 0
fi

RECORD=$(jq -nc \
	--arg event_id "sha256:$EVENT_HEX" \
	--arg stage_id "$LOOM_STAGE_ID" --arg loom_session_id "$LOOM_SESSION_ID" \
	--arg parent_session_id "$PARENT_SESSION_ID" --arg agent_id "$AGENT_ID" \
	--arg agent_type "$AGENT_TYPE" --arg transcript_path "$WORKER_TRANSCRIPT" \
	--arg observed_at "$OBSERVED_AT" --argjson transcript_bytes "$LIFECYCLE_TRANSCRIPT_BYTES" \
	--arg final_record_sha256 "$LIFECYCLE_FINAL_DIGEST" \
	'{version:1,event_id:$event_id,producer:"claude_subagent_stop",
	  identity:{kind:"claude_subagent",stage_id:$stage_id,loom_session_id:$loom_session_id,
	    parent_session_id:$parent_session_id,agent_id:$agent_id,agent_type:$agent_type,
	    transcript_path:$transcript_path},observed_at:$observed_at,state:"completed",
	  evidence:{transcript_bytes:$transcript_bytes,final_record_sha256:$final_record_sha256}}' \
	2>/dev/null)
JQ_STATUS=$?
if ((JQ_STATUS != 0)); then
	loom_lifecycle_log_jq_failure "$HOOK_NAME" "constructing lifecycle JSON" "$JQ_STATUS"
	exit 0
fi
if [[ -z "$RECORD" ]]; then
	loom_debug "$HOOK_NAME: defect - jq produced no lifecycle JSON"
	exit 0
fi

loom_lifecycle_append "$WORK_DIR" "$LOOM_STAGE_ID" "$LOOM_SESSION_ID" "$RECORD" "$HOOK_NAME" || true
loom_lifecycle_refresh_heartbeat "$WORK_DIR" "$LOOM_STAGE_ID" "$LOOM_SESSION_ID" \
	"progress" "subagent ${AGENT_ID} finished" "$HOOK_NAME" || true
if [[ "$AGENT_TYPE" == "$REVIEWER_AGENT_TYPE" ]]; then
	loom_subagent_stop_review_harvest
fi
exit 0
