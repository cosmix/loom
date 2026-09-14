#!/usr/bin/env bash
# Trusted Claude Code TeammateIdle lifecycle producer. TeammateIdle is
# advisory/nonterminal and must always exit 0.

set -uo pipefail
umask 077

source "$(dirname "${BASH_SOURCE[0]}")/_lifecycle.sh"

HOOK_NAME="teammate-idle"
if [[ -z "${LOOM_STAGE_ID:-}" || -z "${LOOM_SESSION_ID:-}" || -z "${LOOM_WORK_DIR:-}" ]]; then
	loom_debug "$HOOK_NAME: skipping - not a Loom stage session"
	exit 0
fi
if ! loom_lifecycle_safe_id "$LOOM_STAGE_ID" || ! loom_lifecycle_safe_id "$LOOM_SESSION_ID" ||
	! command -v jq &>/dev/null || ! command -v sha256sum &>/dev/null; then
	loom_debug "$HOOK_NAME: skipping - unsafe environment identity or missing dependency"
	exit 0
fi

INPUT_JSON=$(loom_lifecycle_read_input)
if printf '%s' "$INPUT_JSON" | jq -e '
	type == "object" and ([.session_id, .transcript_path, .team_name, .teammate_name] |
	all(type == "string" and length > 0 and index("\u0000") == null and
	(test("[\\r\\n]") | not)))
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
PARENT_TRANSCRIPT=$(loom_lifecycle_jq_value "$INPUT_JSON" '.transcript_path' "$HOOK_NAME" \
	"extracting transcript_path") || exit 0
TEAM_NAME=$(loom_lifecycle_jq_value "$INPUT_JSON" '.team_name' "$HOOK_NAME" \
	"extracting team_name") || exit 0
TEAMMATE_NAME=$(loom_lifecycle_jq_value "$INPUT_JSON" '.teammate_name' "$HOOK_NAME" \
	"extracting teammate_name") || exit 0
if ! loom_lifecycle_safe_id "$PARENT_SESSION_ID" || ! loom_lifecycle_safe_id "$TEAM_NAME" ||
	! loom_lifecycle_safe_id "$TEAMMATE_NAME"; then
	loom_debug "$HOOK_NAME: skipping - unsafe parent, team, or teammate identity"
	exit 0
fi
if ! WORK_DIR=$(loom_lifecycle_resolve_work_root "$LOOM_WORK_DIR") ||
	! loom_lifecycle_stage_binding "$WORK_DIR" "$LOOM_STAGE_ID" "$LOOM_SESSION_ID"; then
	loom_debug "$HOOK_NAME: skipping - work root is unsafe or stage session binding is stale"
	exit 0
fi
if ! loom_lifecycle_plain_path "$PARENT_TRANSCRIPT" file ||
	[[ "${PARENT_TRANSCRIPT##*/}" != "${PARENT_SESSION_ID}.jsonl" ]]; then
	loom_debug "$HOOK_NAME: skipping - parent transcript is unsafe or names another session"
	exit 0
fi
TRANSCRIPT_STATUS=0
loom_lifecycle_transcript_evidence "$PARENT_TRANSCRIPT" "$HOOK_NAME" || TRANSCRIPT_STATUS=$?
if ((TRANSCRIPT_STATUS != 0)); then
	if ((TRANSCRIPT_STATUS == 1)); then
		loom_debug "$HOOK_NAME: skipping - parent transcript is empty, torn, malformed, or changing"
	fi
	exit 0
fi
OBSERVED_AT=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z" 2>/dev/null || true)
if [[ -z "$OBSERVED_AT" ]]; then
	loom_debug "$HOOK_NAME: skipping - UTC timestamp unavailable"
	exit 0
fi
if ! EVENT_HEX=$(printf 'loom.lifecycle.claude_teammate_idle.v1\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s' \
	"$LOOM_STAGE_ID" "$LOOM_SESSION_ID" "$PARENT_SESSION_ID" "$TEAM_NAME" "$TEAMMATE_NAME" \
	"$PARENT_TRANSCRIPT" "$LIFECYCLE_TRANSCRIPT_BYTES" "$LIFECYCLE_FINAL_DIGEST" "$OBSERVED_AT" |
	sha256sum 2>/dev/null); then
	loom_debug "$HOOK_NAME: skipping - event id digest failed"
	exit 0
fi
EVENT_HEX=${EVENT_HEX%% *}
if [[ ! "$EVENT_HEX" =~ ^[0-9a-f]{64}$ ]]; then
	loom_debug "$HOOK_NAME: skipping - sha256sum returned an invalid digest"
	exit 0
fi
RECORD=$(jq -nc --arg event_id "sha256:$EVENT_HEX" --arg stage_id "$LOOM_STAGE_ID" \
	--arg loom_session_id "$LOOM_SESSION_ID" --arg parent_session_id "$PARENT_SESSION_ID" \
	--arg team_name "$TEAM_NAME" --arg teammate_name "$TEAMMATE_NAME" \
	--arg observed_at "$OBSERVED_AT" --arg transcript_path "$PARENT_TRANSCRIPT" \
	--argjson transcript_bytes "$LIFECYCLE_TRANSCRIPT_BYTES" \
	--arg final_record_sha256 "$LIFECYCLE_FINAL_DIGEST" '
	{version:1,event_id:$event_id,producer:"claude_teammate_idle",
	 identity:{kind:"claude_teammate",stage_id:$stage_id,loom_session_id:$loom_session_id,
	 parent_session_id:$parent_session_id,team_name:$team_name,teammate_name:$teammate_name},
	 observed_at:$observed_at,state:"idle",evidence:{transcript_path:$transcript_path,
	 transcript_bytes:$transcript_bytes,final_record_sha256:$final_record_sha256}}' 2>/dev/null)
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
	"observation" "teammate ${TEAMMATE_NAME} idle" "$HOOK_NAME" || true
exit 0
