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
#   1. Writes initial heartbeat to session file
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

# Ensure hooks directory exists
HOOKS_DIR="${LOOM_WORK_DIR}/hooks"
mkdir -p "$HOOKS_DIR"

# Log event to events.jsonl
EVENTS_FILE="${HOOKS_DIR}/events.jsonl"
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")
PID=$$

cat >> "$EVENTS_FILE" << EOF
{"timestamp":"${TIMESTAMP}","stage_id":"${LOOM_STAGE_ID}","session_id":"${LOOM_SESSION_ID}","event":"SessionStart","payload":{"type":"SessionStart","pid":${PID}}}
EOF

# Write heartbeat file
HEARTBEAT_FILE="${LOOM_WORK_DIR}/sessions/${LOOM_SESSION_ID}.heartbeat"
mkdir -p "$(dirname "$HEARTBEAT_FILE")"
echo "$TIMESTAMP" > "$HEARTBEAT_FILE"

exit 0
