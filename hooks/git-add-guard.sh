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
#
# Debug mode:
#   Set GIT_ADD_GUARD_DEBUG=1 to see what patterns are being checked
#
# Test cases for Pattern 3 (.work detection):
#   SHOULD BLOCK:
#     git add .work          (direct .work)
#     git add .work/         (directory)
#     git add .work/foo      (subpath)
#     git add foo .work bar  (.work as middle argument)
#     git add .work other    (.work followed by other files)
#   SHOULD ALLOW:
#     git add .workspace     (.work is substring, not standalone)
#     git add .working       (.work is substring)
#     git add .workdir       (.work is substring)
#     git add doc/foo.md     (no .work at all)
#     git add network.md     (no .work at all)

set -euo pipefail

# Debug helper
debug() {
    if [[ "${GIT_ADD_GUARD_DEBUG:-}" == "1" ]]; then
        echo "[git-add-guard DEBUG] $*" >&2
    fi
}

# Read stdin JSON (Claude Code provides tool input)
INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)

# Extract tool name and command
TOOL_NAME=$(echo "$INPUT_JSON" | jq -r '.tool_name // empty' 2>/dev/null || true)
COMMAND=$(echo "$INPUT_JSON" | jq -r '.tool_input.command // empty' 2>/dev/null || true)
debug "Tool: $TOOL_NAME, Command: $COMMAND"

# Only process Bash tool calls
if [[ "$TOOL_NAME" != "Bash" ]] || [[ -z "$COMMAND" ]]; then
    debug "Skipping: not a Bash tool call"
    exit 0
fi

# Check for dangerous git add patterns
check_dangerous_patterns() {
    local cmd="$1"

    # Normalize: remove extra whitespace, convert to lowercase for matching
    local normalized
    normalized=$(echo "$cmd" | tr -s ' ')
    debug "Checking command: $normalized"

    # Pattern 1: git add -A or git add --all (anywhere in command)
    if [[ "$normalized" =~ git[[:space:]]+add[[:space:]].*(-A|--all) ]]; then
        debug "BLOCKED by Pattern 1: git add -A/--all"
        return 1
    fi

    # Pattern 2: git add . (stages current directory)
    # Match "git add ." but not "git add ./file" or "git add .gitignore"
    if [[ "$normalized" =~ git[[:space:]]+add[[:space:]]+\.[[:space:]]*$ ]] || \
       [[ "$normalized" =~ git[[:space:]]+add[[:space:]]+\.[[:space:]]+[^/] ]] || \
       [[ "$normalized" =~ git[[:space:]]+add[[:space:]]+\.[[:space:]]*\&\& ]]; then
        debug "BLOCKED by Pattern 2: git add ."
        return 1
    fi

    # Pattern 3: Explicitly staging .work directory
    # Match .work ONLY as a standalone argument (not as substring of longer name)
    # .work must be followed by: space, forward slash, or end of string
    # This prevents false positives for: .workspace, .working, .workdir, etc.
    if [[ "$normalized" =~ git[[:space:]]+add[[:space:]].*\.work([[:space:]]|/|$) ]]; then
        debug "BLOCKED by Pattern 3: .work directory"
        return 1
    fi

    debug "ALLOWED: No dangerous patterns detected"
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
