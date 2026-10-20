#!/usr/bin/env bash
# _post-tool-heartbeat.sh - Heartbeat writers for PostToolUse and the completion bridge.

if [[ "${_LOOM_POST_TOOL_HEARTBEAT_LOADED:-}" == "1" ]]; then
	return 0
fi
_LOOM_POST_TOOL_HEARTBEAT_LOADED=1

# The activity the daemon reads as "this session's pinned completion is
# running" (COMPLETION_ACTIVITY in loom/src/orchestrator/monitor/stall_line.rs).
LOOM_COMPLETION_ACTIVITY="Stage completion running"

_loom_post_tool_activity_kind() {
	if [[ "$TOOL_NAME" == "Bash" ]]; then
		loom_progress_classify_bash "$COMMAND"
	else
		printf '%s\n' "progress"
	fi
}

_loom_post_tool_build_json() {
	local timestamp="$1" progress_at="$2" activity_kind="$3" activity="$4" json="" subagent_json="false"
	[[ "$IS_SUBAGENT" == "1" ]] && subagent_json="true"
	if command -v jq &>/dev/null; then
		json=$(jq -n --arg stage_id "$LOOM_STAGE_ID" --arg session_id "$LOOM_SESSION_ID" \
			--arg timestamp "$timestamp" --arg progress_at "$progress_at" \
			--arg activity_kind "$activity_kind" --arg last_tool "$TOOL_NAME" --arg activity "$activity" \
			--arg context_tokens_raw "$HB_CONTEXT_TOKENS_RAW" \
			--arg transcript_path_raw "$HB_TRANSCRIPT_PATH_RAW" \
			--argjson subagent "$subagent_json" \
			'{stage_id: $stage_id, session_id: $session_id, timestamp: $timestamp,
			  progress_at: $progress_at, activity_kind: $activity_kind,
			  context_tokens: (if ($context_tokens_raw | test("^[0-9]+$")) then ($context_tokens_raw | tonumber) else null end),
			  transcript_path: (if $transcript_path_raw == "" then null else $transcript_path_raw end),
			  last_tool: $last_tool, activity: $activity, subagent: $subagent}' 2>/dev/null || true)
	fi
	printf '%s' "$json"
}

_loom_post_tool_write_json() {
	local timestamp="$1" progress_at="$2" activity_kind="$3" activity="${4:-Tool executed: $TOOL_NAME}"
	local context_json="null" transcript_json="null" json="" subagent_json="false"
	[[ "$IS_SUBAGENT" == "1" ]] && subagent_json="true"
	json=$(_loom_post_tool_build_json "$timestamp" "$progress_at" "$activity_kind" "$activity")
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
  "activity": "${activity}",
  "subagent": ${subagent_json}
}
EOF
		)
	fi
	loom_heartbeat_atomic_write "$HEARTBEAT_FILE" "$json" || \
		loom_debug "post-tool-use: skipping heartbeat refresh - atomic replacement failed"
}

# Keep the context reading and transcript path the heartbeat already carries,
# for a writer with no transcript reading of its own.
_loom_heartbeat_carry_context() {
	HB_CONTEXT_TOKENS_RAW=""; HB_TRANSCRIPT_PATH_RAW=""
	if [[ -r "$HEARTBEAT_FILE" ]] && command -v jq &>/dev/null; then
		HB_CONTEXT_TOKENS_RAW=$(jq -r '.context_tokens // empty' "$HEARTBEAT_FILE" 2>/dev/null || true)
		HB_TRANSCRIPT_PATH_RAW=$(jq -r '.transcript_path // empty' "$HEARTBEAT_FILE" 2>/dev/null || true)
	fi
}

# Whether this session may replace the heartbeat: never through a symlink, and
# only while it owns the stage (an adjudication session owns its own file).
_loom_heartbeat_writable() {
	if [[ -L "$HEARTBEAT_FILE" ]]; then
		loom_debug "heartbeat: skipping refresh - $HEARTBEAT_FILE is a symlink"
		return 1
	fi
	if [[ "${LOOM_SESSION_TYPE:-}" != "adjudication" ]] && \
		! loom_heartbeat_owner_is_current "$LOOM_WORK_DIR" "$LOOM_STAGE_ID" "$LOOM_SESSION_ID" "$HEARTBEAT_FILE"; then
		loom_debug "heartbeat: skipping stale refresh for session $LOOM_SESSION_ID"
		return 1
	fi
}

# Whether the heartbeat about to be replaced is this session's completion mark.
_loom_heartbeat_marks_completion() {
	[[ -r "$HEARTBEAT_FILE" ]] && command -v jq &>/dev/null || return 1
	jq -e --arg session "$LOOM_SESSION_ID" --arg activity "$LOOM_COMPLETION_ACTIVITY" \
		'.session_id == $session and .activity == $activity' "$HEARTBEAT_FILE" >/dev/null 2>&1
}

_loom_post_tool_write_locked() {
	local activity="Tool executed: $TOOL_NAME"
	_loom_heartbeat_writable || return 0
	if [[ "$IS_SUBAGENT" == "1" ]]; then
		_loom_heartbeat_carry_context
	else
		HB_CONTEXT_TOKENS_RAW="$RESIDENT_TOKENS"; HB_TRANSCRIPT_PATH_RAW="$TRANSCRIPT_PATH"
	fi
	HEARTBEAT_TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")
	HEARTBEAT_ACTIVITY_KIND=$(_loom_post_tool_activity_kind)
	HEARTBEAT_PROGRESS_AT="$HEARTBEAT_TIMESTAMP"
	if [[ "$HEARTBEAT_ACTIVITY_KIND" == "observation" ]]; then
		HEARTBEAT_PROGRESS_AT=$(loom_heartbeat_prior_progress_at "$HEARTBEAT_FILE" "$HEARTBEAT_TIMESTAMP")
	fi
	# The pinned completion writes nothing until it returns. While its mark
	# stands, a subagent's write or an observation (a background `sleep`, a
	# parallel `git status`) keeps it, so the daemon keeps the completion's line;
	# a main-agent progress write, the command's own PostToolUse among them, ends
	# it. A completion whose PostToolUse never fires keeps the mark until the
	# next main-agent progress write, and the daemon's finite line bounds that.
	if [[ "$IS_SUBAGENT" == "1" || "$HEARTBEAT_ACTIVITY_KIND" == "observation" ]] &&
		_loom_heartbeat_marks_completion; then
		activity="$LOOM_COMPLETION_ACTIVITY"
	fi
	_loom_post_tool_write_json "$HEARTBEAT_TIMESTAMP" "$HEARTBEAT_PROGRESS_AT" \
		"$HEARTBEAT_ACTIVITY_KIND" "$activity"
}

# Set HEARTBEAT_FILE for this session, creating its private directory.
_loom_heartbeat_locate() {
	HEARTBEAT_DIR="${LOOM_WORK_DIR}/heartbeat"
	mkdir -p -m 700 "$HEARTBEAT_DIR" 2>/dev/null || return 1
	chmod 700 "$HEARTBEAT_DIR" 2>/dev/null || return 1
	HEARTBEAT_FILE="${HEARTBEAT_DIR}/${LOOM_STAGE_ID}.json"
	if [[ "${LOOM_SESSION_TYPE:-}" == "adjudication" ]]; then
		HEARTBEAT_FILE="${HEARTBEAT_DIR}/${LOOM_STAGE_ID}.adjudication.json"
	fi
}

_loom_heartbeat_detect_subagent() {
	IS_SUBAGENT=0
	PAYLOAD_AGENT_VERDICT=$(loom_payload_agent_verdict "$INPUT_JSON")
	if [[ "$PAYLOAD_AGENT_VERDICT" == "subagent" ]]; then
		IS_SUBAGENT=1
	elif [[ "$PAYLOAD_AGENT_VERDICT" == "unknown" ]] && loom_is_subagent "$INPUT_JSON"; then
		IS_SUBAGENT=1
	fi
}

# _loom_heartbeat_with_lock <writer> - Run <writer> holding the heartbeat lock.
_loom_heartbeat_with_lock() {
	HEARTBEAT_LOCK_DIR="${HEARTBEAT_FILE}.lock"
	if loom_heartbeat_lock_acquire "$HEARTBEAT_LOCK_DIR"; then
		trap 'loom_heartbeat_lock_release "$HEARTBEAT_LOCK_DIR"' EXIT
		"$1"
		loom_heartbeat_lock_release "$HEARTBEAT_LOCK_DIR"
		trap - EXIT
	fi
}

loom_post_tool_write_heartbeat() {
	_loom_heartbeat_locate || exit 0
	TRANSCRIPT_PATH=$(echo "$INPUT_JSON" | jq -r '.transcript_path // empty' 2>/dev/null || true)
	RESIDENT_TOKENS=$(_loom_ctx_last_usage_tokens "$TRANSCRIPT_PATH")
	_loom_heartbeat_detect_subagent
	_loom_heartbeat_with_lock _loom_post_tool_write_locked
}

_loom_completion_write_locked() {
	_loom_heartbeat_writable || return 0
	_loom_heartbeat_carry_context
	HEARTBEAT_TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")
	_loom_post_tool_write_json "$HEARTBEAT_TIMESTAMP" "$HEARTBEAT_TIMESTAMP" "progress" "$LOOM_COMPLETION_ACTIVITY"
}

# loom_write_completion_heartbeat - Mark this session's pinned `loom stage
# complete` as running. The command is one long foreground call that writes no
# heartbeat until it returns; the daemon gives this activity a silence bounded
# by the stage's acceptance work, and the command's own PostToolUse heartbeat
# replaces it. Best effort: it never blocks the completion.
loom_write_completion_heartbeat() {
	[[ -n "${LOOM_WORK_DIR:-}" ]] || return 0
	_loom_heartbeat_locate || return 0
	TOOL_NAME=Bash
	_loom_heartbeat_detect_subagent
	_loom_heartbeat_with_lock _loom_completion_write_locked
}
