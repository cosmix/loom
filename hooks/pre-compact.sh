#!/usr/bin/env bash
# pre-compact.sh - Claude Code PreCompact hook for loom
#
# Called before Claude Code compacts context (context exhaustion).
# This triggers automatic handoff creation.
#
# Input: JSON from stdin (if any - hook doesn't need it)
#
# Environment variables (set by loom worktree settings):
#   LOOM_STAGE_ID    - The stage being executed
#   LOOM_SESSION_ID  - The session ID
#   LOOM_WORK_DIR    - Path to the .work directory
#
# Actions:
#   1. Calls loom handoff create --trigger precompact
#   2. Logs PreCompact event

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

# Try to create handoff
HANDOFF_FILE=""
HANDOFF_ERROR=""

# Check if loom command is available
if command -v loom &>/dev/null; then
	# Create handoff with precompact trigger
	if HANDOFF_OUTPUT=$(loom handoff create --stage "${LOOM_STAGE_ID}" --session "${LOOM_SESSION_ID}" --trigger precompact 2>&1); then
		HANDOFF_FILE=$(echo "$HANDOFF_OUTPUT" | grep -oE '[^/]+\.md$' || echo "")
	else
		HANDOFF_ERROR="$HANDOFF_OUTPUT"
	fi
fi

# Build payload JSON
if [[ -n "$HANDOFF_FILE" ]]; then
	PAYLOAD="{\"type\":\"PreCompact\",\"handoff_file\":\"${HANDOFF_FILE}\"}"
else
	PAYLOAD="{\"type\":\"PreCompact\"}"
fi

cat >>"$EVENTS_FILE" <<EOF
{"timestamp":"${TIMESTAMP}","stage_id":"${LOOM_STAGE_ID}","session_id":"${LOOM_SESSION_ID}","event":"PreCompact","payload":${PAYLOAD}}
EOF

# Log error if handoff creation failed
if [[ -n "$HANDOFF_ERROR" ]]; then
	echo "Warning: Handoff creation failed: $HANDOFF_ERROR" >&2
fi

exit 0
