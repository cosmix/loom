#!/usr/bin/env bash
# worktree-file-guard.sh - PreToolUse hook to enforce file tool isolation
#
# This hook intercepts file tool calls (Read, Write, Edit, Glob, Grep) and
# validates that target paths are within the worktree boundary.
#
# This provides defense-in-depth because Claude Code's sandbox only isolates
# Bash commands at the OS level. The Read/Write/Edit tools use permissions.deny
# which prompts but may not block in orchestrated (headless) mode.
#
# Input: JSON from stdin (Claude Code passes tool info via stdin)
#   {"tool_name": "Read|Write|Edit|Glob|Grep", "tool_input": {...}, ...}
#
# Exit codes:
#   0 - Allow the operation
#   2 - Block with guidance message
#
# Environment:
#   LOOM_STAGE_ID - Current stage ID (set by loom)
#   LOOM_WORKTREE_PATH - Absolute path to current worktree

set -euo pipefail

# Read JSON input from stdin
if command -v gtimeout &>/dev/null; then
    INPUT_JSON=$(gtimeout 1 cat 2>/dev/null || true)
elif command -v timeout &>/dev/null; then
    INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)
else
    INPUT_JSON=$(cat 2>/dev/null || true)
fi

# Debug logging
DEBUG_LOG="/tmp/worktree-file-guard-debug.log"
{
    echo "=== $(date) worktree-file-guard ==="
    echo "INPUT_JSON: $INPUT_JSON"
    echo "LOOM_STAGE_ID: ${LOOM_STAGE_ID:-unset}"
    echo "LOOM_WORKTREE_PATH: ${LOOM_WORKTREE_PATH:-unset}"
    echo "PWD: $(pwd)"
} >>"$DEBUG_LOG" 2>&1

# Parse tool_name and tool_input from JSON
TOOL_NAME=$(echo "$INPUT_JSON" | jq -r '.tool_name // empty' 2>/dev/null || true)
TOOL_INPUT=$(echo "$INPUT_JSON" | jq -r '.tool_input // empty' 2>/dev/null || true)

{
    echo "TOOL_NAME: $TOOL_NAME"
    echo "TOOL_INPUT: $TOOL_INPUT"
} >>"$DEBUG_LOG" 2>&1

# Only run in loom worktrees (check for stage ID)
if [[ -z "${LOOM_STAGE_ID:-}" ]]; then
    exit 0
fi

# Get worktree boundary from environment
WORKTREE_PATH="${LOOM_WORKTREE_PATH:-}"
if [[ -z "$WORKTREE_PATH" ]]; then
    # Try to detect from current directory
    CURRENT_DIR=$(pwd)
    if [[ "$CURRENT_DIR" =~ \.worktrees/([^/]+) ]]; then
        # Extract the worktree root from current path
        WORKTREE_PATH="${CURRENT_DIR%%/.worktrees/*}/.worktrees/${BASH_REMATCH[1]}"
    else
        # Cannot determine boundary, allow operation
        echo "No worktree path, allowing" >>"$DEBUG_LOG" 2>&1
        exit 0
    fi
fi

# Canonicalize worktree path if it exists
if [[ -d "$WORKTREE_PATH" ]]; then
    WORKTREE_PATH=$(cd "$WORKTREE_PATH" && pwd -P)
fi

{
    echo "Resolved WORKTREE_PATH: $WORKTREE_PATH"
} >>"$DEBUG_LOG" 2>&1

# === PATH VALIDATION ===
# Check if a path is within allowed boundaries
validate_path() {
    local target_path="$1"
    local original_path="$target_path"

    # Empty path is allowed (some tools may have optional path)
    if [[ -z "$target_path" ]]; then
        return 0
    fi

    # Always allow ~/.claude/ for config access
    if [[ "$target_path" =~ ^~/\.claude/ ]] || [[ "$target_path" =~ ^"$HOME"/.claude/ ]]; then
        echo "Allowing ~/.claude/ path" >>"$DEBUG_LOG" 2>&1
        return 0
    fi

    # Block explicit path traversal patterns (../../)
    if [[ "$target_path" =~ \.\./\.\. ]] || [[ "$target_path" =~ \.\.[\\/]\.\. ]]; then
        cat >&2 <<'EOF'

============================================================
  LOOM: BLOCKED - Path traversal detected in file operation
============================================================

You tried to: Use ../../ in a file path to escape the worktree

This is FORBIDDEN in loom worktrees because:
  - You are CONFINED to this worktree
  - Accessing parent directories breaks isolation
  - Other worktrees/stages may be affected

Instead, you should:
  - Use relative paths WITHIN this worktree
  - All files you need are in the worktree
  - Context is in your signal file (.work/signals/)

Stay within your worktree boundaries.
============================================================

EOF
        return 1
    fi

    # Convert relative path to absolute
    if [[ ! "$target_path" = /* ]]; then
        # Relative path - resolve from current directory
        if [[ -e "$target_path" ]]; then
            target_path=$(cd "$(dirname "$target_path")" 2>/dev/null && pwd -P)/$(basename "$target_path")
        else
            # Path doesn't exist yet, resolve parent if possible
            local parent_dir
            parent_dir=$(dirname "$target_path")
            if [[ -d "$parent_dir" ]]; then
                target_path=$(cd "$parent_dir" 2>/dev/null && pwd -P)/$(basename "$target_path")
            else
                # Try current directory as base
                target_path="$(pwd)/$target_path"
            fi
        fi
    fi

    # Canonicalize the path (resolve symlinks)
    if [[ -e "$target_path" ]]; then
        target_path=$(cd "$(dirname "$target_path")" 2>/dev/null && pwd -P)/$(basename "$target_path")
    fi

    {
        echo "Validating: original='$original_path' resolved='$target_path'"
    } >>"$DEBUG_LOG" 2>&1

    # Allow access to .work directory (shared state via symlink)
    # .work/ should be a symlink to ../../.work in worktrees
    if [[ "$target_path" =~ /\.work/ ]] || [[ "$target_path" =~ ^\.work/ ]]; then
        echo "Allowing .work/ path" >>"$DEBUG_LOG" 2>&1
        return 0
    fi

    # Check if path is within worktree boundary
    if [[ "$target_path" = "$WORKTREE_PATH"* ]]; then
        echo "Path within worktree boundary" >>"$DEBUG_LOG" 2>&1
        return 0
    fi

    # Block access outside worktree
    cat >&2 <<EOF

============================================================
  LOOM: BLOCKED - File access outside worktree boundary
============================================================

You tried to access: $original_path
Resolved to: $target_path
Worktree boundary: $WORKTREE_PATH

This is FORBIDDEN in loom worktrees because:
  - You are CONFINED to this worktree
  - Accessing files outside the worktree breaks isolation
  - Other stages may be affected

Instead, you should:
  - Use paths relative to your worktree root
  - All files you need are in the worktree
  - Context is embedded in your signal file

ALLOWED PATHS:
  - ./ (current worktree directory)
  - .work/ (shared orchestration state)
  - ~/.claude/ (Claude configuration)

Stay within your worktree boundaries.
============================================================

EOF
    return 1
}

# Extract path from tool input based on tool type
extract_path() {
    local tool_name="$1"
    local tool_input="$2"

    case "$tool_name" in
        Read)
            echo "$tool_input" | jq -r '.file_path // empty' 2>/dev/null || true
            ;;
        Write)
            echo "$tool_input" | jq -r '.file_path // empty' 2>/dev/null || true
            ;;
        Edit)
            echo "$tool_input" | jq -r '.file_path // empty' 2>/dev/null || true
            ;;
        Glob)
            # Glob has a path parameter (directory to search in)
            echo "$tool_input" | jq -r '.path // empty' 2>/dev/null || true
            ;;
        Grep)
            # Grep has a path parameter (file or directory to search in)
            echo "$tool_input" | jq -r '.path // empty' 2>/dev/null || true
            ;;
        *)
            # Unknown tool, return empty
            true
            ;;
    esac
}

# === MAIN DISPATCH ===
case "$TOOL_NAME" in
    Read|Write|Edit|Glob|Grep)
        FILE_PATH=$(extract_path "$TOOL_NAME" "$TOOL_INPUT")
        if [[ -n "$FILE_PATH" ]]; then
            if ! validate_path "$FILE_PATH"; then
                echo "BLOCKED: $TOOL_NAME path failed validation: $FILE_PATH" >>"$DEBUG_LOG" 2>&1
                exit 2
            fi
        fi
        ;;

    *)
        # Not a file tool we validate
        ;;
esac

echo "Allowing operation" >>"$DEBUG_LOG" 2>&1
exit 0
