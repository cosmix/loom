#!/usr/bin/env bash
# post-tool-use.sh - Claude Code PostToolUse hook for loom
#
# Called after each tool use to update the heartbeat.
# This provides activity-based health monitoring.
#
# Input: JSON from stdin (Claude Code passes tool info via stdin)
#   {"tool_name": "Bash", "tool_input": {...}, "tool_result": {...}, ...}
#
# Environment variables (set by loom worktree settings):
#   LOOM_STAGE_ID    - The stage being executed
#   LOOM_SESSION_ID  - The session ID
#   LOOM_WORK_DIR    - Path to the .work directory
#
# Actions:
#   1. Updates heartbeat in .work/heartbeat/<stage-id>.json
#   2. After git commits in loom stages, reminds Claude to update knowledge/memory

set -euo pipefail
umask 077

# Read JSON input from stdin (Claude Code passes tool info via stdin)
# Cross-platform timeout: gtimeout (macOS+coreutils), timeout (Linux), or plain cat
if command -v gtimeout &>/dev/null; then
	INPUT_JSON=$(gtimeout 1 cat 2>/dev/null || true)
elif command -v timeout &>/dev/null; then
	INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)
else
	INPUT_JSON=$(cat 2>/dev/null || true)
fi

# Parse tool_name and tool_input from JSON using jq
TOOL_NAME=$(echo "$INPUT_JSON" | jq -r '.tool_name // empty' 2>/dev/null || true)
TOOL_NAME="${TOOL_NAME:-unknown}"
TOOL_INPUT=$(echo "$INPUT_JSON" | jq -r '.tool_input // empty' 2>/dev/null || true)

# For Bash tool, extract the command
COMMAND=""
if [[ "$TOOL_NAME" == "Bash" ]]; then
	COMMAND=$(echo "$TOOL_INPUT" | jq -r '.command // empty' 2>/dev/null || echo "$TOOL_INPUT")
fi

# Validate required environment variables
if [[ -z "${LOOM_STAGE_ID:-}" ]] || [[ -z "${LOOM_SESSION_ID:-}" ]] || [[ -z "${LOOM_WORK_DIR:-}" ]]; then
	# Silently exit if not in loom context
	exit 0
fi

case "$LOOM_STAGE_ID" in
*[!A-Za-z0-9._-]* | "") exit 0 ;;
esac

# Validate work directory exists and is accessible
if [[ ! -d "${LOOM_WORK_DIR}" ]]; then
	# Silently exit - work dir may have been cleaned up
	exit 0
fi

# Ensure heartbeat directory exists
HEARTBEAT_DIR="${LOOM_WORK_DIR}/heartbeat"
mkdir -p -m 700 "$HEARTBEAT_DIR" 2>/dev/null || exit 0
chmod 700 "$HEARTBEAT_DIR" 2>/dev/null || exit 0

# Get timestamp
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")

# Update heartbeat file in JSON format.
# Build via `jq -n --arg` so a value containing a quote/backslash (e.g. an exotic
# TOOL_NAME) can never produce malformed JSON. Fall back to the heredoc only when
# jq is unavailable — the heartbeat must never be broken by a missing dependency,
# and these values are loom-controlled.
HEARTBEAT_FILE="${HEARTBEAT_DIR}/${LOOM_STAGE_ID}.json"
[[ ! -L "$HEARTBEAT_FILE" ]] || exit 0
HEARTBEAT_JSON=""
if command -v jq &>/dev/null; then
	HEARTBEAT_JSON=$(jq -n \
		--arg stage_id "$LOOM_STAGE_ID" \
		--arg session_id "$LOOM_SESSION_ID" \
		--arg timestamp "$TIMESTAMP" \
		--arg last_tool "$TOOL_NAME" \
		'{stage_id: $stage_id, session_id: $session_id, timestamp: $timestamp, context_percent: null, last_tool: $last_tool, activity: ("Tool executed: " + $last_tool)}' \
		2>/dev/null || true)
fi

if [[ -n "$HEARTBEAT_JSON" ]]; then
	printf '%s\n' "$HEARTBEAT_JSON" >"$HEARTBEAT_FILE"
else
	cat >"$HEARTBEAT_FILE" <<EOF
{
  "stage_id": "${LOOM_STAGE_ID}",
  "session_id": "${LOOM_SESSION_ID}",
  "timestamp": "${TIMESTAMP}",
  "context_percent": null,
  "last_tool": "${TOOL_NAME}",
  "activity": "Tool executed: ${TOOL_NAME}"
}
EOF
fi
chmod 600 "$HEARTBEAT_FILE" 2>/dev/null || true

# Tool results are intentionally not persisted here. A shell hook cannot append
# to a shared path with a race-free no-follow guarantee, and even redacted
# previews risk retaining credentials or private source. The heartbeat above is
# the complete post-tool observability record.

# === POST-COMMIT KNOWLEDGE/MEMORY REMINDER ===
# After a git commit in a loom stage, remind Claude to update knowledge/memory
# This is non-blocking - just a prompt to help capture lessons learned

remind_knowledge_update() {
	cat >&2 <<'REMINDER'

┌────────────────────────────────────────────────────────────────────┐
│  📝 POST-COMMIT REMINDER: Update Knowledge & Memory                │
├────────────────────────────────────────────────────────────────────┤
│                                                                    │
│  You just committed changes. Before completing this stage:         │
│                                                                    │
│  1. RECORD any mistakes made (MANDATORY if errors occurred):       │
│     loom knowledge update mistakes "## [description]               │
│                                                                    │
│     **What happened:** [describe the mistake]                      │
│     **Why:** [root cause]                                          │
│     **How to avoid:** [prevention strategy]"                       │
│                                                                    │
│  2. CAPTURE session insights:                                      │
│     loom memory note "discovered X about Y"                        │
│     loom memory decision "chose X because Y" --context "details"   │
│                                                                    │
│  3. Before stage complete, PROMOTE valuable insights:              │
│     loom memory list                    # Review entries           │
│     loom memory promote all mistakes    # Promote to knowledge     │
│     loom memory promote decision patterns                          │
│                                                                    │
│  Knowledge persists across sessions - future agents will thank you!│
└────────────────────────────────────────────────────────────────────┘

REMINDER
}

# Check if this was a git commit command
if [[ "$TOOL_NAME" == "Bash" ]] && [[ -n "$COMMAND" ]]; then
	# Detect git commit (matches: git commit, git -C path commit, etc.)
	if echo "$COMMAND" | grep -qiE 'git\s+(-C\s+\S+\s+)?commit'; then
		remind_knowledge_update
	fi
fi

exit 0
