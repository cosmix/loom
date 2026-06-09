#!/usr/bin/env bash
# worktree-isolation.sh - PreToolUse hook to enforce worktree boundaries
#
# This hook intercepts tool calls and blocks operations that would violate
# worktree isolation boundaries:
#
# For Bash tool:
#   - Block `git -C`, `git --work-tree`, `git --git-dir` (accessing other git dirs)
#   - Block GIT_DIR= / GIT_WORK_TREE= env assignments that retarget git
#   - Block `eval`-reached git (the regex cannot see inside eval'd strings)
#   - Block `../../` path traversal (escaping worktree)
#   - Block `.worktrees/` access (except current worktree)
#
# SECURITY NOTE (best-effort): this is regex-on-shell, not a parser. It cannot
# catch every evasion — variable indirection (g=-C; git $g ..), $IFS tricks,
# command substitution, or a cd into a sibling repo followed by a plain `git`.
# The DURABLE boundary is the OS sandbox `Write` deny on parent paths; this hook
# is defense-in-depth that raises the cost of the obvious bypasses.
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

# Only enforce inside a genuine loom worktree. Membership is decided by the
# working directory, NOT by LOOM_STAGE_ID: that variable leaks into plain Claude
# Code sessions (e.g. a prior loom run exported it), so gating on it alone made
# this hook wrongly fire on ordinary branches like main. If we are not inside a
# loom worktree, stay inert.
CURRENT_WORKTREE=$(loom_current_worktree) || {
    debug "Not inside a loom worktree; allowing"
    exit 0
}

# Derive the stage from the worktree path itself — authoritative for which
# worktree this session owns, and immune to a stale LOOM_STAGE_ID.
CURRENT_STAGE=$(basename "$CURRENT_WORKTREE")

# === BASH VALIDATION ===
validate_bash_command() {
    local cmd="$1"
    local stripped
    stripped=$(strip_embedded_content "$cmd")

    # Pattern 1: Block git directory/work-tree overrides and eval-reached git.
    #   - `git -C <dir>` / `git --work-tree[=| ]` / `git --git-dir[=| ]`
    #   - `GIT_DIR=...` / `GIT_WORK_TREE=...` env assignments (retarget any git)
    #   - `eval ... git ...` — the regex cannot see inside an eval'd string, so we
    #     refuse the whole command rather than let it through unparsed.
    # Best-effort only — see the SECURITY NOTE in the header for known evasions.
    if echo "$stripped" | grep -qE 'git[[:space:]]+-C[[:space:]]' || \
       echo "$stripped" | grep -qE 'git[[:space:]]+--work-tree([=[:space:]]|$)' || \
       echo "$stripped" | grep -qE 'git[[:space:]]+--git-dir([=[:space:]]|$)' || \
       echo "$stripped" | grep -qE '(^|[[:space:];&|(])GIT_DIR=' || \
       echo "$stripped" | grep -qE '(^|[[:space:];&|(])GIT_WORK_TREE=' || \
       echo "$stripped" | grep -qE '(^|[[:space:];&|(])eval([[:space:]]|$)'; then
        cat >&2 <<'EOF'

============================================================
  LOOM: BLOCKED - Git directory override detected
============================================================

You tried to: Retarget git at another directory (-C / --work-tree /
--git-dir / GIT_DIR= / GIT_WORK_TREE=) or reach git via eval

This is FORBIDDEN in loom worktrees because:
  - Each worktree has its own isolated git state
  - Cross-worktree git operations corrupt state
  - eval hides the real command from isolation checks

Instead, you should:
  - Run git commands in the CURRENT worktree only
  - Use relative paths within this worktree
  - Stay confined to your assigned worktree
  - Do not wrap git in eval

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
