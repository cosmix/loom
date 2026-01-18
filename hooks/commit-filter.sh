#!/usr/bin/env bash
# commit-filter.sh - PreToolUse hook to filter git commit content
#
# This hook intercepts git commit commands and blocks forbidden patterns:
#
# 1. Claude/AI attribution (Co-Authored-By lines mentioning Claude/Anthropic)
#    Per CLAUDE.md rule 8: Never mention Claude in commits.
#
# Environment variables:
#   TOOL_NAME  - Name of the tool being invoked (from Claude Code)
#   TOOL_INPUT - The tool's input (command string for Bash)
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
DEBUG_LOG="/tmp/commit-filter-debug.log"
{
  echo "=== $(date) ==="
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

# === CLAUDE ATTRIBUTION CHECK ===
# Auto-strip Co-Authored-By lines from git commits (forbidden per CLAUDE.md rule 8)
if echo "$COMMAND" | grep -qi 'git commit'; then
    {
        echo "DEBUG: Detected git commit command"
    } >> "$DEBUG_LOG" 2>&1

    if echo "$COMMAND" | grep -Ei -q 'co-authored-by.*claude|claude.*(noreply|anthropic)'; then
        {
            echo "DEBUG: Detected forbidden Co-Authored-By pattern"
        } >> "$DEBUG_LOG" 2>&1

        # Strip the Co-Authored-By line from the command
        # Use sed with actual newline matching (POSIX compatible)
        # The pattern matches: optional whitespace, "Co-Authored-By:", anything until newline
        CORRECTED_COMMAND=$(printf '%s' "$COMMAND" | sed '/[Cc]o-[Aa]uthored-[Bb]y.*[Cc]laude/d; /[Cc]o-[Aa]uthored-[Bb]y.*anthropic/d; /[Cc]o-[Aa]uthored-[Bb]y.*noreply/d')

        # Also try removing inline patterns (for single-line commit messages)
        CORRECTED_COMMAND=$(printf '%s' "$CORRECTED_COMMAND" | sed -E 's/[[:space:]]*[Cc]o-[Aa]uthored-[Bb]y:[^"]*([Cc]laude|anthropic|noreply)[^"]*//g')

        {
            echo "DEBUG: Corrected command:"
            echo "$CORRECTED_COMMAND"
        } >> "$DEBUG_LOG" 2>&1

        # Escape for JSON - must escape backslashes first, then quotes, then newlines
        ESCAPED_COMMAND="${CORRECTED_COMMAND//\\/\\\\}"
        ESCAPED_COMMAND="${ESCAPED_COMMAND//\"/\\\"}"
        # Convert actual newlines to \n for JSON
        ESCAPED_COMMAND=$(printf '%s' "$ESCAPED_COMMAND" | awk '{printf "%s\\n", $0}' | sed 's/\\n$//')

        {
            echo "DEBUG: Escaped command for JSON:"
            echo "$ESCAPED_COMMAND"
        } >> "$DEBUG_LOG" 2>&1

        # Output JSON to auto-correct the command
        printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow","permissionDecisionReason":"Auto-removed forbidden Co-Authored-By attribution (CLAUDE.md rule 8)","updatedInput":{"command":"%s"}}}\n' "$ESCAPED_COMMAND"
        exit 0
    fi
fi

# Command is allowed
exit 0
