#!/bin/bash
# loom stage completion hook - runs when Claude Code session stops
# Called by Claude Code's Stop hook mechanism
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

# Run loom stage complete with verification
# --session updates the session status to Completed
# Verification runs acceptance criteria automatically
loom stage complete "$loom_STAGE_ID" --session "$loom_SESSION_ID" 2>&1 || {
    # If verification fails, leave stage in current state
    # The stage will remain as 'executing' and user can retry manually
    echo "Note: Stage completion deferred (acceptance criteria not met or loom not available)"
}
