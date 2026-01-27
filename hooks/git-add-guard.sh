#!/usr/bin/env bash
# PreToolUse hook: Block dangerous git add patterns that would stage .work
#
# This hook intercepts Bash tool calls and blocks:
# - git add -A / git add --all (stages everything including .work)
# - git add . (stages current directory including .work)
# - git add .work (explicitly staging .work)
#
# Exit codes:
#   0 - Allow the command
#   2 - Block with guidance message

set -euo pipefail

# Read stdin JSON (Claude Code provides tool input)
INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)

# Extract tool name and command
TOOL_NAME=$(echo "$INPUT_JSON" | jq -r '.tool_name // empty' 2>/dev/null || true)
COMMAND=$(echo "$INPUT_JSON" | jq -r '.tool_input.command // empty' 2>/dev/null || true)

# Only process Bash tool calls
if [[ "$TOOL_NAME" != "Bash" ]] || [[ -z "$COMMAND" ]]; then
    exit 0
fi

# Check for dangerous git add patterns
check_dangerous_patterns() {
    local cmd="$1"

    # Normalize: remove extra whitespace, convert to lowercase for matching
    local normalized
    normalized=$(echo "$cmd" | tr -s ' ')

    # Pattern 1: git add -A or git add --all (anywhere in command)
    if [[ "$normalized" =~ git[[:space:]]+add[[:space:]].*(-A|--all) ]]; then
        return 1
    fi

    # Pattern 2: git add . (stages current directory)
    # Match "git add ." but not "git add ./file" or "git add .gitignore"
    if [[ "$normalized" =~ git[[:space:]]+add[[:space:]]+\.[[:space:]]*$ ]] || \
       [[ "$normalized" =~ git[[:space:]]+add[[:space:]]+\.[[:space:]]+[^/] ]] || \
       [[ "$normalized" =~ git[[:space:]]+add[[:space:]]+\.[[:space:]]*\&\& ]]; then
        return 1
    fi

    # Pattern 3: Explicitly staging .work
    if [[ "$normalized" =~ git[[:space:]]+add[[:space:]].*\.work ]]; then
        return 1
    fi

    return 0
}

# Check the command
if ! check_dangerous_patterns "$COMMAND"; then
    # Block with guidance
    cat >&2 <<'EOF'

============================================================
  LOOM: BLOCKED - Dangerous git add pattern detected
============================================================

Your command would stage .work (orchestration state) which MUST NOT be committed.

BLOCKED PATTERNS:
  - git add -A / git add --all  (stages everything)
  - git add .                    (stages current directory)
  - git add .work                (explicitly stages .work)

CORRECT PATTERN:
  git add <specific-files>

Example:
  git add src/main.rs src/lib.rs

WHY: In worktrees, .work is a symlink to shared state. Committing it
     corrupts the main repository for all parallel stages.

============================================================

EOF
    exit 2
fi

# Allow the command
exit 0
