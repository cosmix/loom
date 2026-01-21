#!/usr/bin/env bash
# loom post-AskUserQuestion hook - runs after user answers a question
# Called by Claude Code's PostToolUse hook mechanism
#
# Input: JSON from stdin (Claude Code passes tool info via stdin)
#   {"tool_name": "AskUserQuestion", "tool_input": {...}, "tool_result": {...}, ...}
#
# Environment variables (set by loom worktree settings):
#   LOOM_SESSION_ID - The session identifier
#   LOOM_STAGE_ID   - The stage being worked on
#   LOOM_WORK_DIR   - Path to .work/ directory

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

# Change to the project directory (parent of .work/)
if [ -n "$LOOM_WORK_DIR" ]; then
	cd "$(dirname "$LOOM_WORK_DIR")" 2>/dev/null || exit 0
fi

# Resume stage execution after user input
loom stage resume "$LOOM_STAGE_ID" 2>&1 || {
	echo "Note: Could not resume stage (loom not available)"
}

exit 0
