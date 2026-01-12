#!/usr/bin/env bash
# session-end.sh - Claude Code SessionEnd hook for loom
#
# Called when a Claude Code session ends normally.
#
# Environment variables:
#   LOOM_STAGE_ID    - The stage being executed
#   LOOM_SESSION_ID  - The session ID
#   LOOM_WORK_DIR    - Path to the .work directory
#
# Actions:
#   1. Checks if stage was completed
#   2. If not completed, creates handoff
#   3. Logs SessionEnd event

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

# Check if stage was already completed
STAGE_FILE="${LOOM_WORK_DIR}/stages/${LOOM_STAGE_ID}.md"
COMPLETED=false
if [[ -f "$STAGE_FILE" ]]; then
    if grep -q "status: Completed" "$STAGE_FILE" 2>/dev/null || grep -q "status: Verified" "$STAGE_FILE" 2>/dev/null; then
        COMPLETED=true
    fi
fi

# If not completed and loom is available, try to create handoff
if [[ "$COMPLETED" != "true" ]] && command -v loom &> /dev/null; then
    loom handoff create --stage "${LOOM_STAGE_ID}" --session "${LOOM_SESSION_ID}" --trigger session_end 2>/dev/null || true
fi

# Build payload
PAYLOAD="{\"type\":\"SessionEnd\",\"completed\":${COMPLETED}}"

cat >> "$EVENTS_FILE" << EOF
{"timestamp":"${TIMESTAMP}","stage_id":"${LOOM_STAGE_ID}","session_id":"${LOOM_SESSION_ID}","event":"SessionEnd","payload":${PAYLOAD}}
EOF

exit 0
