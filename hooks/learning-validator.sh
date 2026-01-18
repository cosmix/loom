#!/usr/bin/env bash
# stop.sh - Claude Code Stop hook for loom
#
# Called when a Claude Code session is stopping.
# Logs the stop event.
#
# Environment variables:
#   LOOM_STAGE_ID    - The stage being executed
#   LOOM_SESSION_ID  - The session ID
#   LOOM_WORK_DIR    - Path to the .work directory
#
# Actions:
#   1. Logs Stop event

set -euo pipefail

# Drain stdin to prevent blocking (Stop hooks receive JSON from Claude Code)
timeout 1 cat >/dev/null 2>&1 || true

# Validate required environment variables
if [[ -z "${LOOM_STAGE_ID:-}" ]] || [[ -z "${LOOM_SESSION_ID:-}" ]] || [[ -z "${LOOM_WORK_DIR:-}" ]]; then
    echo "Error: Missing required environment variables" >&2
    exit 1
fi

# Validate work directory exists and is accessible
# This prevents "spawn /bin/sh ENOENT" errors when hooks run from deleted directories
if [[ ! -d "${LOOM_WORK_DIR}" ]]; then
    echo "Warning: Work directory does not exist: ${LOOM_WORK_DIR}" >&2
    exit 0  # Exit gracefully - don't block session exit for missing directory
fi

# Ensure hooks directory exists
HOOKS_DIR="${LOOM_WORK_DIR}/hooks"
mkdir -p "$HOOKS_DIR" 2>/dev/null || {
    echo "Warning: Cannot create hooks directory: ${HOOKS_DIR}" >&2
    exit 0
}

# Log event to events.jsonl
EVENTS_FILE="${HOOKS_DIR}/events.jsonl"
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")

# Build payload
PAYLOAD="{\"type\":\"Stop\"}"

cat >> "$EVENTS_FILE" << EOF
{"timestamp":"${TIMESTAMP}","stage_id":"${LOOM_STAGE_ID}","session_id":"${LOOM_SESSION_ID}","event":"Stop","payload":${PAYLOAD}}
EOF

exit 0
