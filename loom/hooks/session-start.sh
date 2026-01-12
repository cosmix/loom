#!/usr/bin/env bash
# session-start.sh - Claude Code SessionStart hook for loom
#
# Called when a Claude Code session starts.
# Environment variables:
#   LOOM_STAGE_ID    - The stage being executed
#   LOOM_SESSION_ID  - The session ID
#   LOOM_WORK_DIR    - Path to the .work directory
#
# Actions:
#   1. Writes initial heartbeat to .work/heartbeat/<stage-id>.json
#   2. Logs session start event

set -euo pipefail

# Validate required environment variables
if [[ -z "${LOOM_STAGE_ID:-}" ]] || [[ -z "${LOOM_SESSION_ID:-}" ]] || [[ -z "${LOOM_WORK_DIR:-}" ]]; then
    echo "Error: Missing required environment variables" >&2
    echo "  LOOM_STAGE_ID=${LOOM_STAGE_ID:-<unset>}" >&2
    echo "  LOOM_SESSION_ID=${LOOM_SESSION_ID:-<unset>}" >&2
    echo "  LOOM_WORK_DIR=${LOOM_WORK_DIR:-<unset>}" >&2
    exit 1
fi

# Ensure directories exist
HOOKS_DIR="${LOOM_WORK_DIR}/hooks"
HEARTBEAT_DIR="${LOOM_WORK_DIR}/heartbeat"
mkdir -p "$HOOKS_DIR" "$HEARTBEAT_DIR"

# Get timestamp
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")
PID=$$

# Log event to events.jsonl
EVENTS_FILE="${HOOKS_DIR}/events.jsonl"
cat >> "$EVENTS_FILE" << EOF
{"timestamp":"${TIMESTAMP}","stage_id":"${LOOM_STAGE_ID}","session_id":"${LOOM_SESSION_ID}","event":"SessionStart","payload":{"type":"SessionStart","pid":${PID}}}
EOF

# Write heartbeat file in JSON format
# Format: {stage_id, session_id, timestamp, context_pct, last_tool, activity}
HEARTBEAT_FILE="${HEARTBEAT_DIR}/${LOOM_STAGE_ID}.json"
cat > "$HEARTBEAT_FILE" << EOF
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
