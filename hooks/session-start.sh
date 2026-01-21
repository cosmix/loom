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

# Drain stdin to prevent blocking
# Cross-platform: gtimeout (macOS+coreutils), timeout (Linux), or cat
if command -v gtimeout &>/dev/null; then
	gtimeout 1 cat >/dev/null 2>&1 || true
elif command -v timeout &>/dev/null; then
	timeout 1 cat >/dev/null 2>&1 || true
else
	cat >/dev/null 2>&1 || true
fi

# Validate required environment variables
# Silently exit if not in loom context (hook runs on ALL sessions)
if [[ -z "${LOOM_STAGE_ID:-}" ]] || [[ -z "${LOOM_SESSION_ID:-}" ]] || [[ -z "${LOOM_WORK_DIR:-}" ]]; then
	exit 0
fi

# Validate work directory exists and is accessible
if [[ ! -d "${LOOM_WORK_DIR}" ]]; then
	echo "Warning: Work directory does not exist: ${LOOM_WORK_DIR}" >&2
	exit 0 # Exit gracefully
fi

# Ensure directories exist
HOOKS_DIR="${LOOM_WORK_DIR}/hooks"
HEARTBEAT_DIR="${LOOM_WORK_DIR}/heartbeat"
mkdir -p "$HOOKS_DIR" "$HEARTBEAT_DIR" 2>/dev/null || {
	echo "Warning: Cannot create required directories" >&2
	exit 0
}

# Get timestamp
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")
PID=$$

# Log event to events.jsonl
EVENTS_FILE="${HOOKS_DIR}/events.jsonl"
cat >>"$EVENTS_FILE" <<EOF
{"timestamp":"${TIMESTAMP}","stage_id":"${LOOM_STAGE_ID}","session_id":"${LOOM_SESSION_ID}","event":"SessionStart","payload":{"type":"SessionStart","pid":${PID}}}
EOF

# Write heartbeat file in JSON format
# Format: {stage_id, session_id, timestamp, context_pct, last_tool, activity}
HEARTBEAT_FILE="${HEARTBEAT_DIR}/${LOOM_STAGE_ID}.json"
cat >"$HEARTBEAT_FILE" <<EOF
{
  "stage_id": "${LOOM_STAGE_ID}",
  "session_id": "${LOOM_SESSION_ID}",
  "timestamp": "${TIMESTAMP}",
  "context_percent": null,
  "last_tool": null,
  "activity": "Session started"
}
EOF

exit 0
