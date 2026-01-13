#!/usr/bin/env bash
# loom post-AskUserQuestion hook - runs after user answers a question
# Called by Claude Code's PostToolUse hook mechanism
#
# Environment variables (set by loom worktree settings):
#   LOOM_SESSION_ID - The session identifier
#   LOOM_STAGE_ID   - The stage being worked on
#   LOOM_WORK_DIR   - Path to .work/ directory

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
