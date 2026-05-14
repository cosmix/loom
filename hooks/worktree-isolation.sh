#!/usr/bin/env bash
# worktree-isolation.sh - PreToolUse hook to enforce worktree boundaries
#
# This hook intercepts tool calls and blocks operations that would violate
# worktree isolation boundaries:
#
# For Bash tool:
#   - Block `git -C` or `git --work-tree` (accessing other git dirs)
#   - Block `../../` path traversal (escaping worktree)
#   - Block `.worktrees/` access (except current worktree)
#
# For Edit/Write tools:
#   - Block paths outside worktree bounds
#   - Block writes to `.work/stages/` and `.work/sessions/`
#
# Input: JSON from stdin (Claude Code passes tool info via stdin)
#   {"tool_name": "Bash|Edit|Write", "tool_input": {...}, ...}
#
# Exit codes:
#   0 - Allow the operation
#   2 - Block with guidance message
#
# Environment:
#   LOOM_STAGE_ID - Current stage ID (set by loom)
#   LOOM_WORKTREE_PATH - Path to current worktree (if set)

set -euo pipefail
source "$(dirname "$0")/_common.sh"

debug() {
    [[ "${WORKTREE_ISOLATION_DEBUG:-}" == "1" ]] || return 0
    echo "$@" >&2
}

# Read JSON input from stdin
if command -v gtimeout &>/dev/null; then
    INPUT_JSON=$(gtimeout 1 cat 2>/dev/null || true)
elif command -v timeout &>/dev/null; then
    INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)
else
    INPUT_JSON=$(cat 2>/dev/null || true)
fi

debug "=== $(date) worktree-isolation ==="
debug "INPUT_JSON: $INPUT_JSON"
debug "LOOM_STAGE_ID: ${LOOM_STAGE_ID:-unset}"
debug "PWD: $(pwd)"

# Parse tool_name and tool_input from JSON
TOOL_NAME=$(echo "$INPUT_JSON" | jq -r '.tool_name // empty' 2>/dev/null || true)
TOOL_INPUT=$(echo "$INPUT_JSON" | jq -r '.tool_input // empty' 2>/dev/null || true)

debug "TOOL_NAME: $TOOL_NAME"
debug "TOOL_INPUT: $TOOL_INPUT"

# Only run in loom worktrees (check for stage ID)
if [[ -z "${LOOM_STAGE_ID:-}" ]]; then
    exit 0
fi

# Determine current worktree from environment or current directory
CURRENT_WORKTREE="${LOOM_WORKTREE_PATH:-}"
if [[ -z "$CURRENT_WORKTREE" ]]; then
    # Try to detect from current directory
    CURRENT_DIR=$(pwd)
    if [[ "$CURRENT_DIR" =~ \.worktrees/([^/]+) ]]; then
        CURRENT_STAGE="${BASH_REMATCH[1]}"
    else
        CURRENT_STAGE="$LOOM_STAGE_ID"
    fi
else
    # Derive the stage from the worktree path itself. LOOM_STAGE_ID can be
    # stale (a prior stage's value leaking into this session's env), but
    # LOOM_WORKTREE_PATH is authoritative for which worktree this session owns.
    if [[ "$CURRENT_WORKTREE" =~ \.worktrees/([^/]+) ]]; then
        CURRENT_STAGE="${BASH_REMATCH[1]}"
    else
        CURRENT_STAGE="$LOOM_STAGE_ID"
    fi
fi

# === BASH VALIDATION ===
validate_bash_command() {
    local cmd="$1"
    local stripped
    stripped=$(strip_embedded_content "$cmd")

    # Pattern 1: Block git -C or git --work-tree (accessing other directories)
    if echo "$stripped" | grep -qE 'git[[:space:]]+-C[[:space:]]' || \
       echo "$stripped" | grep -qE 'git[[:space:]]+--work-tree'; then
        cat >&2 <<'EOF'

============================================================
  LOOM: BLOCKED - Git directory override detected
============================================================

You tried to: Use git -C or --work-tree to access another directory

This is FORBIDDEN in loom worktrees because:
  - Each worktree has its own isolated git state
  - Cross-worktree git operations corrupt state

Instead, you should:
  - Run git commands in the CURRENT worktree only
  - Use relative paths within this worktree
  - Stay confined to your assigned worktree

Git commands should operate on the current directory.
============================================================

EOF
        return 1
    fi

    # Pattern 2: Block ../../ path traversal (escaping worktree)
    if echo "$stripped" | grep -qE '\.\./\.\.' || echo "$stripped" | grep -qE '\.\.[\\/]\.\.'; then
        cat >&2 <<'EOF'

============================================================
  LOOM: BLOCKED - Path traversal detected
============================================================

You tried to: Use ../../ to escape the worktree

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

    # Pattern 3: Block .worktrees/ access (except current worktree)
    # Allow references to current stage, block others
    if echo "$stripped" | grep -qE '\.worktrees/' && \
       ! echo "$stripped" | grep -qE "\.worktrees/${CURRENT_STAGE}[/[:space:]]|\.worktrees/${CURRENT_STAGE}\$"; then
        cat >&2 <<EOF

============================================================
  LOOM: BLOCKED - Cross-worktree access detected
============================================================

You tried to: Access .worktrees/ directory (another stage's worktree)

This is FORBIDDEN because:
  - Each stage has its own isolated worktree
  - Accessing other stages' worktrees breaks isolation
  - You may corrupt another stage's work

Instead, you should:
  - Stay in YOUR worktree: .worktrees/${CURRENT_STAGE}/
  - Your files and context are all here
  - Communicate via .work/ (shared state symlink)

You can only access your own worktree.
============================================================

EOF
        return 1
    fi

    return 0
}

# === EDIT/WRITE VALIDATION ===
validate_file_path() {
    local file_path="$1"

    # Block absolute paths to .work/stages/ or .work/sessions/
    if echo "$file_path" | grep -qE '\.work/(stages|sessions)/'; then
        cat >&2 <<'EOF'

============================================================
  LOOM: BLOCKED - Protected state file access
============================================================

You tried to: Write to .work/stages/ or .work/sessions/

This is FORBIDDEN because:
  - Stage and session files are managed by loom CLI
  - Direct edits corrupt orchestrator state
  - This causes phantom completions and lost work

Instead, you should:
  - Use `loom stage complete` to complete a stage
  - Use `loom memory` to record insights
  - NEVER manually edit .work/ state files

Use loom CLI commands to manage state.
============================================================

EOF
        return 1
    fi

    # Block writes to other worktrees
    if echo "$file_path" | grep -qE '\.worktrees/' && \
       ! echo "$file_path" | grep -qE "\.worktrees/${CURRENT_STAGE}/"; then
        cat >&2 <<EOF

============================================================
  LOOM: BLOCKED - Cross-worktree file write
============================================================

You tried to: Write to another stage's worktree

This is FORBIDDEN because:
  - Each stage owns its worktree exclusively
  - Writing to other worktrees causes conflicts
  - You may overwrite another agent's work

Instead, you should:
  - Write only to files in YOUR worktree
  - Your worktree: .worktrees/${CURRENT_STAGE}/
  - Files are merged after stage completion

Stay in your assigned worktree.
============================================================

EOF
        return 1
    fi

    # Block path traversal in file paths
    if echo "$file_path" | grep -qE '\.\./\.\.' || echo "$file_path" | grep -qE '\.\.[\\/]\.\.'; then
        cat >&2 <<'EOF'

============================================================
  LOOM: BLOCKED - Path traversal in file path
============================================================

You tried to: Use ../../ in a file path to escape the worktree

This is FORBIDDEN because:
  - You must stay within your worktree
  - Path traversal could access main repo or other worktrees
  - This breaks isolation guarantees

Instead, you should:
  - Use relative paths within the worktree
  - Or use paths starting with ./ for current directory
  - All needed files are in your worktree

Use paths relative to the worktree root.
============================================================

EOF
        return 1
    fi

    return 0
}

# === MAIN DISPATCH ===
case "$TOOL_NAME" in
    Bash)
        COMMAND=$(echo "$TOOL_INPUT" | jq -r '.command // empty' 2>/dev/null || echo "$TOOL_INPUT")
        if [[ -n "$COMMAND" ]]; then
            if ! validate_bash_command "$COMMAND"; then
                debug "BLOCKED: Bash command failed validation"
                exit 2
            fi
        fi
        ;;

    Edit)
        FILE_PATH=$(echo "$TOOL_INPUT" | jq -r '.file_path // empty' 2>/dev/null || true)
        if [[ -n "$FILE_PATH" ]]; then
            if ! validate_file_path "$FILE_PATH"; then
                debug "BLOCKED: Edit path failed validation: $FILE_PATH"
                exit 2
            fi
        fi
        ;;

    Write)
        FILE_PATH=$(echo "$TOOL_INPUT" | jq -r '.file_path // empty' 2>/dev/null || true)
        if [[ -n "$FILE_PATH" ]]; then
            if ! validate_file_path "$FILE_PATH"; then
                debug "BLOCKED: Write path failed validation: $FILE_PATH"
                exit 2
            fi
        fi
        ;;

    *)
        # Not a tool we validate
        ;;
esac

debug "Allowing operation"
exit 0
