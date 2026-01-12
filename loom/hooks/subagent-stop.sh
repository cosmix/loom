#!/usr/bin/env bash
# subagent-stop.sh - Claude Code SubagentStop hook for loom
#
# Called when a subagent completes execution.
# Extracts learnings from the subagent's work.
#
# Environment variables:
#   LOOM_STAGE_ID    - The stage being executed
#   LOOM_SESSION_ID  - The session ID
#   LOOM_WORK_DIR    - Path to the .work directory
#
# Actions:
#   1. Calls loom learn extract to capture learnings
#   2. Logs SubagentStop event

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

# Extract learnings if loom is available
LEARNINGS_COUNT=0

if command -v loom &> /dev/null; then
    # Try to extract learnings from subagent output
    # The loom learn pattern command parses subagent output for learnings
    if LEARN_OUTPUT=$(loom learn pattern --stage "${LOOM_STAGE_ID}" 2>&1); then
        # Count learnings extracted (if output contains count)
        LEARNINGS_COUNT=$(echo "$LEARN_OUTPUT" | grep -oE '[0-9]+ learning' | grep -oE '[0-9]+' || echo "0")
    fi
fi

# Build payload
PAYLOAD="{\"type\":\"SubagentStop\",\"learnings_count\":${LEARNINGS_COUNT}}"

cat >> "$EVENTS_FILE" << EOF
{"timestamp":"${TIMESTAMP}","stage_id":"${LOOM_STAGE_ID}","session_id":"${LOOM_SESSION_ID}","event":"SubagentStop","payload":${PAYLOAD}}
EOF

exit 0
