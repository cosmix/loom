#!/usr/bin/env bash
# session-end.sh - Claude Code SessionEnd hook for loom
#
# Called when a Claude Code session ends normally.
#
# Input: JSON from stdin (if any - hook doesn't need it)
#
# Environment variables (set by loom worktree settings):
#   LOOM_STAGE_ID    - The stage being executed
#   LOOM_SESSION_ID  - The session ID
#   LOOM_WORK_DIR    - Path to the .work directory
#
# Actions:
#   1. Checks if stage was completed
#   2. If not completed, creates handoff
#   3. Logs SessionEnd event

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

# Ensure hooks directory exists
HOOKS_DIR="${LOOM_WORK_DIR}/hooks"
mkdir -p "$HOOKS_DIR" 2>/dev/null || {
	echo "Warning: Cannot create hooks directory" >&2
	exit 0
}

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
if [[ "$COMPLETED" != "true" ]] && command -v loom &>/dev/null; then
	loom handoff create --stage "${LOOM_STAGE_ID}" --session "${LOOM_SESSION_ID}" --trigger session_end 2>/dev/null || true
fi

# Build payload
PAYLOAD="{\"type\":\"SessionEnd\",\"completed\":${COMPLETED}}"

cat >>"$EVENTS_FILE" <<EOF
{"timestamp":"${TIMESTAMP}","stage_id":"${LOOM_STAGE_ID}","session_id":"${LOOM_SESSION_ID}","event":"SessionEnd","payload":${PAYLOAD}}
EOF

exit 0
