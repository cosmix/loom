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
#   0 - Allow the command to proceed (always; this hook is advisory only)
#
# Output format when warning:
#   {"hookSpecificOutput": {"hookEventName": "PreToolUse", "additionalContext": "LOOM_HOOK_WARN: ..."}}

set -euo pipefail

# Source shared utilities for strip_embedded_content()
source "$(dirname "$0")/_common.sh"

# Read JSON input from stdin (Claude Code passes tool info via stdin)
# Cross-platform timeout: gtimeout (macOS+coreutils), timeout (Linux), or plain cat
if command -v gtimeout &>/dev/null; then
	INPUT_JSON=$(gtimeout 1 cat 2>/dev/null || true)
elif command -v timeout &>/dev/null; then
	INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)
else
	INPUT_JSON=$(cat 2>/dev/null || true)
fi

# Debug logging (fallback to /dev/null if log path not writable)
DEBUG_LOG="/tmp/prefer-modern-debug.log"
if ! touch "$DEBUG_LOG" 2>/dev/null; then
	DEBUG_LOG=/dev/null
fi
{
	echo "=== $(date) prefer-modern-tools ==="
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

# Strip heredoc bodies and -m/--message content to avoid false positives
STRIPPED_COMMAND=$(strip_embedded_content "$COMMAND")

# Skip loom knowledge/memory commands — their text payloads often contain
# words like "find" or "grep" that are not actual command invocations
if echo "$COMMAND" | grep -qE '(^|[;&|[:space:]])loom[[:space:]]+(knowledge|memory)[[:space:]]'; then
	echo "Skipping: loom knowledge/memory command" >>"$DEBUG_LOG" 2>&1
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

# Check for grep usage - warn and guide to native tools first, then rg
if uses_grep "$STRIPPED_COMMAND"; then
	echo "WARNED: grep detected" >>"$DEBUG_LOG" 2>&1
	jq -nc --arg ctx "LOOM_HOOK_WARN: Prefer Claude Code's native Grep tool or 'rg' (ripgrep) over 'grep'. Examples: grep -r \"pattern\" . → rg \"pattern\" ." \
		'{hookSpecificOutput: {hookEventName: "PreToolUse", additionalContext: $ctx}}'
	exit 0
fi

# Check for find usage - warn and guide to native tools first, then fd
if uses_find "$STRIPPED_COMMAND"; then
	echo "WARNED: find detected" >>"$DEBUG_LOG" 2>&1
	jq -nc --arg ctx "LOOM_HOOK_WARN: Prefer Claude Code's native Glob tool or 'fd' over 'find'. Examples: find . -name \"*.txt\" → fd -e txt" \
		'{hookSpecificOutput: {hookEventName: "PreToolUse", additionalContext: $ctx}}'
	exit 0
fi

# Command is allowed as-is
echo "Allowing command as-is" >>"$DEBUG_LOG" 2>&1
exit 0
