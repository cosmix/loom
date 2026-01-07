#!/bin/bash
# loom post-AskUserQuestion hook - runs after user answers a question
# Called by Claude Code's PostToolUse hook mechanism
#
# Environment variables (set by loom spawner):
#   loom_SESSION_ID - The session identifier
#   loom_STAGE_ID   - The stage being worked on
#   loom_WORK_DIR   - Path to .work/ directory

# Only run if this is a loom-managed session
if [ -z "$loom_STAGE_ID" ] || [ -z "$loom_SESSION_ID" ]; then
    exit 0
fi

# Change to the project directory (parent of .work/)
if [ -n "$loom_WORK_DIR" ]; then
    cd "$(dirname "$loom_WORK_DIR")" 2>/dev/null || exit 0
fi

# Resume stage execution after user input
loom stage resume "$loom_STAGE_ID" 2>&1 || {
    echo "Note: Could not resume stage (loom not available)"
}

exit 0
