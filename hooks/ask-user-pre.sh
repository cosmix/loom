#!/usr/bin/env bash
# loom pre-AskUserQuestion hook - runs before asking user a question
# Called by Claude Code's PreToolUse hook mechanism
#
# Input: JSON from stdin (Claude Code passes tool info via stdin)
#   {"tool_name": "AskUserQuestion", "tool_input": {...}, ...}
#
# Environment variables (set by loom worktree settings):
#   LOOM_SESSION_ID - The session identifier
#   LOOM_STAGE_ID   - The stage being worked on
#   LOOM_WORK_DIR   - Path to the state directory (.loom/work/, or the
#                     legacy .work/ for a workspace that already resolved
#                     to it)

# Drain stdin to prevent blocking (hook doesn't need tool input details)
# Cross-platform: gtimeout (macOS+coreutils), timeout (Linux), or cat
if command -v gtimeout &>/dev/null; then
	gtimeout 1 cat >/dev/null 2>&1 || true
elif command -v timeout &>/dev/null; then
	timeout 1 cat >/dev/null 2>&1 || true
else
	cat >/dev/null 2>&1 || true
fi

# Only run if this is a loom-managed session
if [ -z "$LOOM_STAGE_ID" ] || [ -z "$LOOM_SESSION_ID" ]; then
	exit 0
fi

# Change to the project directory (parent of the state directory). The
# current layout nests the state dir two levels below the project root
# (<root>/.loom/work); a workspace that already resolved to the legacy
# layout nests it one level below (<root>/.work) - see doc/plans,
# "Back-compat".
if [ -n "$LOOM_WORK_DIR" ]; then
	case "$LOOM_WORK_DIR" in
	*/.loom/work) cd "$(dirname "$(dirname "$LOOM_WORK_DIR")")" 2>/dev/null || exit 0 ;;
	*) cd "$(dirname "$LOOM_WORK_DIR")" 2>/dev/null || exit 0 ;;
	esac
fi

# Mark stage as waiting for user input
loom stage waiting "$LOOM_STAGE_ID" 2>&1 || {
	echo "Note: Could not mark stage as waiting (loom not available)"
}

# Prepare notification message
MESSAGE="loom stage $LOOM_STAGE_ID needs your input"

# Send desktop notification based on platform
if [[ "$OSTYPE" == "darwin"* ]]; then
	# macOS notification
	osascript -e "display notification \"$MESSAGE\" with title \"loom\"" 2>/dev/null
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
	# Linux notification
	notify-send -u critical "loom" "$MESSAGE" 2>/dev/null
fi

# Ring terminal bell
printf '\a'

exit 0
