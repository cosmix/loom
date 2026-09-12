#!/usr/bin/env bash
# session-end.sh - Claude Code SessionEnd hook for loom
#
# Called when a Claude Code session ends normally.
#
# Input: JSON from stdin - the `reason` field (clear/resume/logout/
# prompt_input_exit/other) is extracted and recorded in the logged event.
#
# Environment variables (set by loom worktree settings):
#   LOOM_STAGE_ID    - The stage being executed
#   LOOM_SESSION_ID  - The session ID
#   LOOM_WORK_DIR    - Path to the state directory (.loom/work, or the
#                      legacy .work for a workspace that already resolved
#                      to it)
#
# Actions:
#   1. Checks if stage was completed
#   2. If not completed, creates handoff
#   3. Logs SessionEnd event

set -euo pipefail

# Read stdin JSON (for the `reason` field)
# Cross-platform: gtimeout (macOS+coreutils), timeout (Linux), or cat
if command -v gtimeout &>/dev/null; then
	INPUT_JSON=$(gtimeout 1 cat 2>/dev/null || true)
elif command -v timeout &>/dev/null; then
	INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)
else
	INPUT_JSON=$(cat 2>/dev/null || true)
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
# Stage files have depth prefix: e.g., 02-implement-handoff-fix.md
STAGE_FILE=""
for candidate in "${LOOM_WORK_DIR}/stages/"*"-${LOOM_STAGE_ID}.md"; do
	if [[ -f "$candidate" ]]; then
		STAGE_FILE="$candidate"
		break
	fi
done
# Fallback to exact match (no prefix)
if [[ -z "$STAGE_FILE" ]]; then
	STAGE_FILE="${LOOM_WORK_DIR}/stages/${LOOM_STAGE_ID}.md"
fi

COMPLETED=false
if [[ -n "$STAGE_FILE" ]] && [[ -f "$STAGE_FILE" ]]; then
	if grep -q "status: Completed" "$STAGE_FILE" 2>/dev/null || grep -q "status: Verified" "$STAGE_FILE" 2>/dev/null; then
		COMPLETED=true
	fi
fi

# If not completed and loom is available, try to create handoff
if [[ "$COMPLETED" != "true" ]] && command -v loom &>/dev/null; then
	loom handoff --stage "${LOOM_STAGE_ID}" --session "${LOOM_SESSION_ID}" --trigger session_end 2>/dev/null || true
fi

# Extract the reason Claude Code ended the session (clear/resume/logout/
# prompt_input_exit/other) - empty when jq is unavailable or stdin wasn't JSON
REASON=""
if command -v jq &>/dev/null; then
	REASON=$(printf '%s' "$INPUT_JSON" | jq -r '.reason // empty' 2>/dev/null || true)
fi

# Build the full event line. When jq is available it builds the whole line,
# so stage/session ids and the reason are JSON-escaped rather than
# interpolated raw into a heredoc.
if command -v jq &>/dev/null; then
	EVENT_LINE=$(jq -cn --arg ts "$TIMESTAMP" --arg stage "$LOOM_STAGE_ID" --arg session "$LOOM_SESSION_ID" --argjson completed "$COMPLETED" --arg reason "$REASON" \
		'{timestamp:$ts,stage_id:$stage,session_id:$session,event:"SessionEnd",payload:{type:"SessionEnd",completed:$completed,reason:$reason}}')
else
	EVENT_LINE="{\"timestamp\":\"${TIMESTAMP}\",\"stage_id\":\"${LOOM_STAGE_ID}\",\"session_id\":\"${LOOM_SESSION_ID}\",\"event\":\"SessionEnd\",\"payload\":{\"type\":\"SessionEnd\",\"completed\":${COMPLETED},\"reason\":\"\"}}"
fi
printf '%s\n' "$EVENT_LINE" >>"$EVENTS_FILE"

exit 0
