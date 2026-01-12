#!/usr/bin/env bash
# stop.sh - Claude Code Stop hook for loom
#
# Called when a Claude Code session is stopping.
# Validates that learning files haven't been damaged.
#
# Environment variables:
#   LOOM_STAGE_ID    - The stage being executed
#   LOOM_SESSION_ID  - The session ID
#   LOOM_WORK_DIR    - Path to the .work directory
#
# Actions:
#   1. Validates learning files are intact
#   2. Logs Stop event
#   3. Exits with error if learnings were damaged (blocking exit)

set -euo pipefail

# Validate required environment variables
if [[ -z "${LOOM_STAGE_ID:-}" ]] || [[ -z "${LOOM_SESSION_ID:-}" ]] || [[ -z "${LOOM_WORK_DIR:-}" ]]; then
    echo "Error: Missing required environment variables" >&2
    exit 1
fi

# Ensure hooks directory exists
HOOKS_DIR="${LOOM_WORK_DIR}/hooks"
mkdir -p "$HOOKS_DIR"

# Log event to events.jsonl
EVENTS_FILE="${HOOKS_DIR}/events.jsonl"
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")

# Validate learning files if loom is available
LEARNINGS_INTACT=true
ERROR_MSG=""

if command -v loom &> /dev/null; then
    # Call loom verify learnings command
    if ! VERIFY_OUTPUT=$(loom verify learnings --session "${LOOM_SESSION_ID}" 2>&1); then
        LEARNINGS_INTACT=false
        ERROR_MSG="$VERIFY_OUTPUT"
    fi
fi

# Build payload
if [[ -n "$ERROR_MSG" ]]; then
    # Escape quotes in error message for JSON
    ERROR_MSG_ESCAPED=$(echo "$ERROR_MSG" | sed 's/"/\\"/g' | tr '\n' ' ')
    PAYLOAD="{\"type\":\"Stop\",\"learnings_intact\":${LEARNINGS_INTACT},\"error\":\"${ERROR_MSG_ESCAPED}\"}"
else
    PAYLOAD="{\"type\":\"Stop\",\"learnings_intact\":${LEARNINGS_INTACT}}"
fi

cat >> "$EVENTS_FILE" << EOF
{"timestamp":"${TIMESTAMP}","stage_id":"${LOOM_STAGE_ID}","session_id":"${LOOM_SESSION_ID}","event":"Stop","payload":${PAYLOAD}}
EOF

# Exit with error if learnings were damaged
# This will block the session from exiting cleanly
if [[ "$LEARNINGS_INTACT" != "true" ]]; then
    echo "ERROR: Learning files were damaged during session." >&2
    echo "Files have been restored from snapshot." >&2
    echo "Please review and ensure learnings are preserved." >&2
    exit 1
fi

exit 0
