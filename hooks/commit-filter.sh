#!/usr/bin/env bash
# commit-filter.sh - PreToolUse hook to block forbidden commit patterns
#
# This hook intercepts git commit commands and BLOCKS (not modifies) forbidden patterns:
#
# 1. Claude/AI attribution (Co-Authored-By lines mentioning Claude/Anthropic)
#    Per CLAUDE.md rule 8: Never mention Claude in commits.
#
# Instead of trying to modify the command (fragile with JSON escaping),
# this hook blocks and provides guidance so Claude regenerates the command.
#
# Input: JSON from stdin (Claude Code passes tool info via stdin)
#   {"tool_name": "Bash", "tool_input": {"command": "..."}, ...}
#
# Exit codes:
#   0 - Allow the command to proceed
#   2 - Block the command and return guidance to Claude
#
# Output format when blocking:
#   Guidance message to stderr, then exit 2

set -euo pipefail

# Read JSON input from stdin (Claude Code passes tool info via stdin)
# Use gtimeout (macOS with coreutils) or timeout (Linux), or just cat
if command -v gtimeout &>/dev/null; then
	INPUT_JSON=$(gtimeout 1 cat 2>/dev/null || true)
elif command -v timeout &>/dev/null; then
	INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)
else
	# No timeout available - just read stdin (Claude Code closes it properly)
	INPUT_JSON=$(cat 2>/dev/null || true)
fi

# Debug logging
DEBUG_LOG="/tmp/commit-filter-debug.log"
{
	echo "=== $(date) ==="
	echo "INPUT_JSON: $INPUT_JSON"
} >>"$DEBUG_LOG" 2>&1

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
} >>"$DEBUG_LOG" 2>&1

# Only check Bash tool uses
if [[ "$TOOL_NAME" != "Bash" ]]; then
	exit 0
fi

if [[ -z "$COMMAND" ]]; then
	exit 0
fi

# === CLAUDE ATTRIBUTION CHECK ===
# Block git commits with Co-Authored-By lines mentioning Claude/Anthropic (per CLAUDE.md rule 8)

# Check if this is a git commit command
if echo "$COMMAND" | grep -qiE 'git[[:space:]].*commit'; then
	{
		echo "DEBUG: Detected git commit command"
	} >>"$DEBUG_LOG" 2>&1

	# Check for forbidden Co-Authored-By patterns
	if echo "$COMMAND" | grep -qiE 'co-authored-by.*(claude|anthropic|noreply@anthropic)'; then
		{
			echo "DEBUG: BLOCKED - Detected forbidden Co-Authored-By pattern"
		} >>"$DEBUG_LOG" 2>&1

		# Output guidance to stderr and block
		cat >&2 <<'EOF'
BLOCKED: Commit contains forbidden attribution (CLAUDE.md rule 8).

Your commit message includes a Co-Authored-By line mentioning Claude/Anthropic.
Per project rules, AI attribution must NEVER appear in commits.

Please rewrite your git commit command WITHOUT the Co-Authored-By line.
The commit message should only contain your actual changes description.

Example - remove lines like:
  Co-Authored-By: Claude <noreply@anthropic.com>
  Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>

Rewrite and try again.
EOF
		exit 2
	fi
fi

# Command is allowed
echo "Allowing command" >>"$DEBUG_LOG" 2>&1
exit 0
