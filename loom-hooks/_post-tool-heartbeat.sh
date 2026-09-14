#!/usr/bin/env bash
# _post-tool-heartbeat.sh - PostToolUse heartbeat writer helpers.

if [[ "${_LOOM_POST_TOOL_HEARTBEAT_LOADED:-}" == "1" ]]; then
	return 0
fi
_LOOM_POST_TOOL_HEARTBEAT_LOADED=1

_loom_post_tool_activity_kind() {
	if [[ "$TOOL_NAME" == "Bash" ]]; then
		loom_progress_classify_bash "$COMMAND"
	else
		printf '%s\n' "progress"
	fi
}

_loom_post_tool_build_json() {
	local timestamp="$1" progress_at="$2" activity_kind="$3" json="" subagent_json="false"
	[[ "$IS_SUBAGENT" == "1" ]] && subagent_json="true"
	if command -v jq &>/dev/null; then
		json=$(jq -n --arg stage_id "$LOOM_STAGE_ID" --arg session_id "$LOOM_SESSION_ID" \
			--arg timestamp "$timestamp" --arg progress_at "$progress_at" \
			--arg activity_kind "$activity_kind" --arg last_tool "$TOOL_NAME" \
			--arg context_tokens_raw "$HB_CONTEXT_TOKENS_RAW" \
			--arg transcript_path_raw "$HB_TRANSCRIPT_PATH_RAW" \
			--argjson subagent "$subagent_json" \
			'{stage_id: $stage_id, session_id: $session_id, timestamp: $timestamp,
			  progress_at: $progress_at, activity_kind: $activity_kind,
			  context_tokens: (if ($context_tokens_raw | test("^[0-9]+$")) then ($context_tokens_raw | tonumber) else null end),
			  transcript_path: (if $transcript_path_raw == "" then null else $transcript_path_raw end),
			  last_tool: $last_tool, activity: ("Tool executed: " + $last_tool), subagent: $subagent}' 2>/dev/null || true)
	fi
	printf '%s' "$json"
}

_loom_post_tool_write_json() {
	local timestamp="$1" progress_at="$2" activity_kind="$3"
	local context_json="null" transcript_json="null" json="" subagent_json="false"
	[[ "$IS_SUBAGENT" == "1" ]] && subagent_json="true"
	json=$(_loom_post_tool_build_json "$timestamp" "$progress_at" "$activity_kind")
	if [[ -z "$json" ]]; then
		[[ "$HB_CONTEXT_TOKENS_RAW" =~ ^[0-9]+$ ]] && context_json="$HB_CONTEXT_TOKENS_RAW"
		[[ -n "$HB_TRANSCRIPT_PATH_RAW" ]] && transcript_json="\"${HB_TRANSCRIPT_PATH_RAW}\""
		json=$(cat <<EOF
{
  "stage_id": "${LOOM_STAGE_ID}",
  "session_id": "${LOOM_SESSION_ID}",
  "timestamp": "${timestamp}",
  "progress_at": "${progress_at}",
  "activity_kind": "${activity_kind}",
  "context_tokens": ${context_json},
  "transcript_path": ${transcript_json},
  "last_tool": "${TOOL_NAME}",
  "activity": "Tool executed: ${TOOL_NAME}",
  "subagent": ${subagent_json}
}
EOF
		)
	fi
	loom_heartbeat_atomic_write "$HEARTBEAT_FILE" "$json" || \
		loom_debug "post-tool-use: skipping heartbeat refresh - atomic replacement failed"
}

_loom_post_tool_write_locked() {
	if [[ -L "$HEARTBEAT_FILE" ]]; then
		loom_debug "post-tool-use: skipping heartbeat refresh - $HEARTBEAT_FILE is a symlink"
		return 0
	fi
	if [[ "${LOOM_SESSION_TYPE:-}" != "adjudication" ]] && \
		! loom_heartbeat_owner_is_current "$LOOM_WORK_DIR" "$LOOM_STAGE_ID" "$LOOM_SESSION_ID" "$HEARTBEAT_FILE"; then
		loom_debug "post-tool-use: skipping stale heartbeat refresh for session $LOOM_SESSION_ID"
		return 0
	fi
	HB_CONTEXT_TOKENS_RAW=""; HB_TRANSCRIPT_PATH_RAW=""
	if [[ "$IS_SUBAGENT" == "1" ]]; then
		if [[ -r "$HEARTBEAT_FILE" ]] && command -v jq &>/dev/null; then
			HB_CONTEXT_TOKENS_RAW=$(jq -r '.context_tokens // empty' "$HEARTBEAT_FILE" 2>/dev/null || true)
			HB_TRANSCRIPT_PATH_RAW=$(jq -r '.transcript_path // empty' "$HEARTBEAT_FILE" 2>/dev/null || true)
		fi
	else
		HB_CONTEXT_TOKENS_RAW="$RESIDENT_TOKENS"; HB_TRANSCRIPT_PATH_RAW="$TRANSCRIPT_PATH"
	fi
	HEARTBEAT_TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")
	HEARTBEAT_ACTIVITY_KIND=$(_loom_post_tool_activity_kind)
	HEARTBEAT_PROGRESS_AT="$HEARTBEAT_TIMESTAMP"
	if [[ "$HEARTBEAT_ACTIVITY_KIND" == "observation" ]]; then
		HEARTBEAT_PROGRESS_AT=$(loom_heartbeat_prior_progress_at "$HEARTBEAT_FILE" "$HEARTBEAT_TIMESTAMP")
	fi
	_loom_post_tool_write_json "$HEARTBEAT_TIMESTAMP" "$HEARTBEAT_PROGRESS_AT" "$HEARTBEAT_ACTIVITY_KIND"
}

loom_post_tool_write_heartbeat() {
	HEARTBEAT_DIR="${LOOM_WORK_DIR}/heartbeat"
	mkdir -p -m 700 "$HEARTBEAT_DIR" 2>/dev/null || exit 0
	chmod 700 "$HEARTBEAT_DIR" 2>/dev/null || exit 0
	TRANSCRIPT_PATH=$(echo "$INPUT_JSON" | jq -r '.transcript_path // empty' 2>/dev/null || true)
	RESIDENT_TOKENS=$(_loom_ctx_last_usage_tokens "$TRANSCRIPT_PATH")
	IS_SUBAGENT=0
	PAYLOAD_AGENT_VERDICT=$(loom_payload_agent_verdict "$INPUT_JSON")
	if [[ "$PAYLOAD_AGENT_VERDICT" == "subagent" ]]; then
		IS_SUBAGENT=1
	elif [[ "$PAYLOAD_AGENT_VERDICT" == "unknown" ]] && loom_is_subagent "$INPUT_JSON"; then
		IS_SUBAGENT=1
	fi
	HEARTBEAT_FILE="${HEARTBEAT_DIR}/${LOOM_STAGE_ID}.json"
	if [[ "${LOOM_SESSION_TYPE:-}" == "adjudication" ]]; then
		HEARTBEAT_FILE="${HEARTBEAT_DIR}/${LOOM_STAGE_ID}.adjudication.json"
	fi
	HEARTBEAT_LOCK_DIR="${HEARTBEAT_FILE}.lock"
	if loom_heartbeat_lock_acquire "$HEARTBEAT_LOCK_DIR"; then
		trap 'loom_heartbeat_lock_release "$HEARTBEAT_LOCK_DIR"' EXIT
		_loom_post_tool_write_locked
		loom_heartbeat_lock_release "$HEARTBEAT_LOCK_DIR"
		trap - EXIT
	fi
}
