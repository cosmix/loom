#!/usr/bin/env bash
# subagent-stop.sh - Claude Code SubagentStop hook for loom
#
# Called when a subagent completes execution.
# Logs the subagent stop event.
#
# Environment variables:
#   LOOM_STAGE_ID    - The stage being executed
#   LOOM_SESSION_ID  - The session ID
#   LOOM_WORK_DIR    - Path to the .work directory
#
# Actions:
#   1. Logs SubagentStop event

set -euo pipefail

# Drain stdin to prevent blocking (SubagentStop hooks receive JSON from Claude Code)
# Cross-platform: gtimeout (macOS+coreutils), timeout (Linux), or cat
if command -v gtimeout &>/dev/null; then
	gtimeout 1 cat >/dev/null 2>&1 || true
elif command -v timeout &>/dev/null; then
	timeout 1 cat >/dev/null 2>&1 || true
else
	cat >/dev/null 2>&1 || true
fi

# Validate required environment variables
if [[ -z "${LOOM_STAGE_ID:-}" ]] || [[ -z "${LOOM_SESSION_ID:-}" ]] || [[ -z "${LOOM_WORK_DIR:-}" ]]; then
	echo "Error: Missing required environment variables" >&2
	exit 1
fi

# Validate work directory exists and is accessible
if [[ ! -d "${LOOM_WORK_DIR}" ]]; then
	echo "Warning: Work directory does not exist: ${LOOM_WORK_DIR}" >&2
	exit 0 # Exit gracefully
fi

# Ensure hooks directory exists
HOOKS_DIR="${LOOM_WORK_DIR}/hooks"
mkdir -p "$HOOKS_DIR" 2>/dev/null || {
	echo "Warning: Cannot create hooks directory" >&2
	exit 0
}

# Log event to events.jsonl
EVENTS_FILE="${HOOKS_DIR}/events.jsonl"
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")

# Build payload
PAYLOAD="{\"type\":\"SubagentStop\"}"

cat >>"$EVENTS_FILE" <<EOF
{"timestamp":"${TIMESTAMP}","stage_id":"${LOOM_STAGE_ID}","session_id":"${LOOM_SESSION_ID}","event":"SubagentStop","payload":${PAYLOAD}}
EOF

exit 0
