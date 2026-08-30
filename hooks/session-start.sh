#!/usr/bin/env bash
# session-start.sh - Claude Code SessionStart hook for loom
#
# Called when a Claude Code session starts.
#
# Input: JSON from stdin (if any - hook doesn't need it)
#
# Environment variables (set by loom worktree settings):
#   LOOM_STAGE_ID    - The stage being executed
#   LOOM_SESSION_ID  - The session ID
#   LOOM_WORK_DIR    - Path to the .work directory
#
# Actions:
#   1. Writes initial heartbeat to .work/heartbeat/<stage-id>.json
#   2. Logs session start event

set -euo pipefail
umask 077

source "$(dirname "$0")/_common.sh"

# Read stdin JSON (SessionStart may pass {"source": "compact"/"resume"/"startup"/"clear"})
# Cross-platform timeout: gtimeout (macOS+coreutils), timeout (Linux), or plain cat
if command -v gtimeout &>/dev/null; then
	INPUT_JSON=$(gtimeout 1 cat 2>/dev/null || true)
elif command -v timeout &>/dev/null; then
	INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)
else
	INPUT_JSON=$(cat 2>/dev/null || true)
fi

# Validate required environment variables
# Silently exit if not in loom context (hook runs on ALL sessions)
if [[ -z "${LOOM_STAGE_ID:-}" ]] || [[ -z "${LOOM_SESSION_ID:-}" ]] || [[ -z "${LOOM_WORK_DIR:-}" ]]; then
	exit 0
fi

# Both go into filenames below (the heartbeat path is keyed on
# LOOM_STAGE_ID) - the same character guard post-tool-use.sh, subagent-stop.sh
# and subagent-start.sh already apply to these two values.
case "$LOOM_STAGE_ID" in
*[!A-Za-z0-9._-]* | "") exit 0 ;;
esac
case "$LOOM_SESSION_ID" in
*[!A-Za-z0-9._-]* | "") exit 0 ;;
esac

# Validate work directory exists and is accessible
if [[ ! -d "${LOOM_WORK_DIR}" ]]; then
	echo "Warning: Work directory does not exist: ${LOOM_WORK_DIR}" >&2
	exit 0 # Exit gracefully
fi

# Ensure directories exist
HOOKS_DIR="${LOOM_WORK_DIR}/hooks"
mkdir -p "$HOOKS_DIR" 2>/dev/null || {
	echo "Warning: Cannot create required directories" >&2
	exit 0
}

# Same mode and failure tolerance as post-tool-use.sh's heartbeat directory
# creation - SessionStart runs first, so without this the directory would
# otherwise sit at the mkdir default (0755) until the first PostToolUse
# tightened it, an odd pairing with the heartbeat file's own chmod 600 below.
HEARTBEAT_DIR="${LOOM_WORK_DIR}/heartbeat"
mkdir -p -m 700 "$HEARTBEAT_DIR" 2>/dev/null || exit 0
chmod 700 "$HEARTBEAT_DIR" 2>/dev/null || exit 0

# Get timestamp
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")
PID=$$

# Log event to events.jsonl
EVENTS_FILE="${HOOKS_DIR}/events.jsonl"
cat >>"$EVENTS_FILE" <<EOF
{"timestamp":"${TIMESTAMP}","stage_id":"${LOOM_STAGE_ID}","session_id":"${LOOM_SESSION_ID}","event":"SessionStart","payload":{"type":"SessionStart","pid":${PID}}}
EOF

# The transcript is usually empty/absent this early, so this is normally
# empty too - recorded when present so a resumed/compacted session still
# carries its transcript path forward as soon as one exists.
TRANSCRIPT_PATH=""
if command -v jq &>/dev/null; then
	TRANSCRIPT_PATH=$(echo "$INPUT_JSON" | jq -r '.transcript_path // empty' 2>/dev/null || true)
fi

# Write heartbeat file in JSON format. All heartbeat writers share the lock:
# ownership is checked only after acquisition, then a complete same-directory
# temp file is renamed into place. In particular, an old SessionStart delayed
# behind a successor cannot replace the successor's heartbeat.
# Format: {stage_id, session_id, timestamp, context_tokens, transcript_path, last_tool, activity}
# Built via `jq -n --arg` so a transcript_path containing a quote/backslash can
# never produce malformed JSON - matches post-tool-use.sh's heartbeat write.
# A symlinked heartbeat path is refused, matching spawn-guard.sh's spawn
# record write.
HEARTBEAT_FILE="${HEARTBEAT_DIR}/${LOOM_STAGE_ID}.json"
HEARTBEAT_LOCK_DIR="${HEARTBEAT_FILE}.lock"
if loom_heartbeat_lock_acquire "$HEARTBEAT_LOCK_DIR"; then
	trap 'loom_heartbeat_lock_release "$HEARTBEAT_LOCK_DIR"' EXIT
	if [[ -L "$HEARTBEAT_FILE" ]]; then
		loom_debug "session-start: skipping heartbeat write - $HEARTBEAT_FILE is a symlink"
	elif ! loom_heartbeat_owner_is_current "$LOOM_WORK_DIR" "$LOOM_STAGE_ID" "$LOOM_SESSION_ID" "$HEARTBEAT_FILE"; then
		loom_debug "session-start: skipping stale heartbeat write for session $LOOM_SESSION_ID"
	else
		HEARTBEAT_JSON=""
		if command -v jq &>/dev/null; then
			HEARTBEAT_JSON=$(jq -n \
				--arg stage_id "$LOOM_STAGE_ID" \
				--arg session_id "$LOOM_SESSION_ID" \
				--arg timestamp "$TIMESTAMP" \
				--arg transcript_path_raw "$TRANSCRIPT_PATH" \
				'{stage_id: $stage_id, session_id: $session_id, timestamp: $timestamp,
				  context_tokens: null,
				  transcript_path: (if $transcript_path_raw == "" then null else $transcript_path_raw end),
				  last_tool: null, activity: "Session started"}' \
				2>/dev/null || true)
		fi

		if [[ -n "$HEARTBEAT_JSON" ]]; then
			loom_heartbeat_atomic_write "$HEARTBEAT_FILE" "$HEARTBEAT_JSON" || \
				loom_debug "session-start: skipping heartbeat write - atomic replacement failed"
		else
		# jq unavailable: TRANSCRIPT_PATH is only ever populated when jq
		# succeeded above, so it is guaranteed empty here and this heredoc can
		# never carry untrusted content - LOOM_STAGE_ID/LOOM_SESSION_ID are
		# already restricted to [A-Za-z0-9._-] by the guard above.
			HEARTBEAT_JSON=$(cat <<EOF
{
  "stage_id": "${LOOM_STAGE_ID}",
  "session_id": "${LOOM_SESSION_ID}",
  "timestamp": "${TIMESTAMP}",
  "context_tokens": null,
  "transcript_path": null,
  "last_tool": null,
  "activity": "Session started"
}
EOF
			)
			loom_heartbeat_atomic_write "$HEARTBEAT_FILE" "$HEARTBEAT_JSON" || \
				loom_debug "session-start: skipping heartbeat write - atomic replacement failed"
		fi
	fi
	loom_heartbeat_lock_release "$HEARTBEAT_LOCK_DIR"
	trap - EXIT
fi

# Emit re-anchor context on compaction/resume starts
if command -v jq &>/dev/null; then
	SOURCE=$(echo "$INPUT_JSON" | jq -r '.source // empty' 2>/dev/null || true)
	if [[ "$SOURCE" == "compact" ]] || [[ "$SOURCE" == "resume" ]]; then
		SIGNAL_PATH="${LOOM_WORK_DIR}/signals/${LOOM_SESSION_ID}.md"
		jq -nc \
			--arg ctx "Context was compacted. Re-anchor: re-read ${SIGNAL_PATH}, run 'loom memory list', read the latest .work/handoffs/ file. Understand before acting; do not guess." \
			'{hookSpecificOutput: {hookEventName: "SessionStart", additionalContext: $ctx}}'
	fi
fi

exit 0
