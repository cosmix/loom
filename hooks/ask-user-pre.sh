#!/bin/bash
# loom pre-AskUserQuestion hook - runs before asking user a question
# Called by Claude Code's PreToolUse hook mechanism
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

# Mark stage as waiting for user input
loom stage waiting "$loom_STAGE_ID" 2>&1 || {
    echo "Note: Could not mark stage as waiting (loom not available)"
}

# Prepare notification message
MESSAGE="loom stage $loom_STAGE_ID needs your input"

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
