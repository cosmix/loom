#!/usr/bin/env bash
# prefer-modern-tools.sh - PreToolUse hook to guide CLI tool selection
#
# This hook intercepts Bash commands and provides guidance:
#
# For grep:
#   - Standard: Use Claude Code's native Grep tool
#   - Advanced (flags, pipes): Use 'rg' (ripgrep) instead of 'grep'
#
# For find:
#   - Standard: Use Claude Code's native Glob tool
#   - Advanced (flags, pipes): Use 'fd' instead of 'find'
#
# Per CLAUDE.md rule 6:
#   "If you must use CLI search, use `rg` or `fd` — never `grep` or `find`."
#
# Input: JSON from stdin (Claude Code passes tool info via stdin)
#   {"tool_name": "Bash", "tool_input": {"command": "..."}, ...}
#
# Exit codes:
#   0 - Allow the command to proceed
#   2 - Block the command and return guidance to Claude
#
# Output format when blocking:
#   {"continue": false, "reason": "..."}

set -euo pipefail

# Read JSON input from stdin (Claude Code passes tool info via stdin)
# Use timeout to avoid blocking if stdin is empty or kept open
INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)

# Debug logging
DEBUG_LOG="/tmp/prefer-modern-debug.log"
{
  echo "=== $(date) prefer-modern-tools ==="
  echo "INPUT_JSON: $INPUT_JSON"
} >> "$DEBUG_LOG" 2>&1

# Parse tool_name and tool_input from JSON using jq
TOOL_NAME=$(echo "$INPUT_JSON" | jq -r '.tool_name // empty' 2>/dev/null || true)
TOOL_INPUT=$(echo "$INPUT_JSON" | jq -r '.tool_input // empty' 2>/dev/null || true)

# For Bash tool, tool_input is an object with "command" field
if [[ "$TOOL_NAME" == "Bash" ]]; then
    COMMAND=$(echo "$TOOL_INPUT" | jq -r '.command // empty' 2>/dev/null || echo "$TOOL_INPUT")
else
    COMMAND=""
fi

# Debug parsed values
{
  echo "TOOL_NAME: $TOOL_NAME"
  echo "COMMAND: $COMMAND"
  echo "---"
} >> "$DEBUG_LOG" 2>&1

# Only check Bash tool uses
if [[ "$TOOL_NAME" != "Bash" ]]; then
    exit 0
fi

if [[ -z "$COMMAND" ]]; then
    exit 0
fi

# Check if command uses grep (but not rg)
uses_grep() {
    local cmd="$1"
    # Match grep but not rg (ripgrep)
    echo "$cmd" | grep -qE '(^|[|;&[:space:]])(\/usr\/bin\/|\/bin\/)?grep[[:space:]]'
}

# Check if command uses find (but not fd)
uses_find() {
    local cmd="$1"
    # Match find but not fd
    echo "$cmd" | grep -qE '(^|[|;&[:space:]])(\/usr\/bin\/|\/bin\/)?find[[:space:]]'
}

# Check for grep usage - block and guide to native tools first, then rg
if uses_grep "$COMMAND"; then
    echo "BLOCKED: grep detected" >> "$DEBUG_LOG" 2>&1
    # Output to stderr (shown to Claude) and exit 2 to block
    cat >&2 <<'EOF'
BLOCKED: Prefer Claude Code's native Grep tool for standard searches.

For simple pattern matching, use the Grep tool directly:
  Grep tool: pattern="error", path="src/", glob="*.rs"

If you need advanced features (complex regex, pipes, output processing),
use 'rg' (ripgrep) instead of 'grep':

Examples:
  grep -r "pattern" .     →  rg "pattern" .
  grep -i "pattern" file  →  rg -i "pattern" file
  grep -v "exclude" file  →  rg -v "exclude" file
  grep -l "pattern" .     →  rg -l "pattern" .

Use the native Grep tool when possible, or rewrite using rg.
EOF
    exit 2
fi

# Check for find usage - block and guide to native tools first, then fd
if uses_find "$COMMAND"; then
    echo "BLOCKED: find detected" >> "$DEBUG_LOG" 2>&1
    # Output to stderr (shown to Claude) and exit 2 to block
    cat >&2 <<'EOF'
BLOCKED: Prefer Claude Code's native Glob tool for file searches.

For finding files by pattern, use the Glob tool directly:
  Glob tool: pattern="**/*.rs", path="src/"

If you need advanced features (modification time, size, exec),
use 'fd' instead of 'find':

Examples:
  find . -name "*.txt"           →  fd -e txt
  find . -name "*.rs" -type f    →  fd -e rs -t f
  find src -name "test*"         →  fd "test" src
  find . -mtime -7               →  fd --changed-within 7d

Use the native Glob tool when possible, or rewrite using fd.
EOF
    exit 2
fi

# Command is allowed as-is
echo "Allowing command as-is" >> "$DEBUG_LOG" 2>&1
exit 0
