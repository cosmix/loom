#!/usr/bin/env bash
# prefer-modern-tools.sh - PreToolUse hook to guide CLI tool selection
#
# This hook intercepts Bash commands and provides guidance:
#
# For grep: redo the search with 'rg' (ripgrep) instead of 'grep'.
# For find: redo the search with 'fd' instead of 'find'.
#
# Claude Code's native Grep/Glob tools were removed, so 'rg' and 'fd' are
# now the canonical replacements — not just shell-pipeline fallbacks.
#
# Per CLAUDE.md rule 8:
#   "Search with `rg` (text) and `fd` (files) — never `grep` or `find`."
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

debug() {
	[[ "${PREFER_MODERN_TOOLS_DEBUG:-}" == "1" ]] || return 0
	echo "$@" >&2
}

# Read JSON input from stdin (Claude Code passes tool info via stdin)
# Cross-platform timeout: gtimeout (macOS+coreutils), timeout (Linux), or plain cat
if command -v gtimeout &>/dev/null; then
	INPUT_JSON=$(gtimeout 1 cat 2>/dev/null || true)
elif command -v timeout &>/dev/null; then
	INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)
else
	INPUT_JSON=$(cat 2>/dev/null || true)
fi

debug "=== $(date) prefer-modern-tools ==="
debug "INPUT_JSON: $INPUT_JSON"

# Parse tool_name and tool_input from JSON using jq
TOOL_NAME=$(echo "$INPUT_JSON" | jq -r '.tool_name // empty' 2>/dev/null || true)
TOOL_INPUT=$(echo "$INPUT_JSON" | jq -r '.tool_input // empty' 2>/dev/null || true)

# For Bash tool, tool_input is an object with "command" field
if [[ "$TOOL_NAME" == "Bash" ]]; then
	COMMAND=$(echo "$TOOL_INPUT" | jq -r '.command // empty' 2>/dev/null || echo "$TOOL_INPUT")
else
	COMMAND=""
fi

debug "TOOL_NAME: $TOOL_NAME"
debug "COMMAND: $COMMAND"
debug "---"

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
	debug "Skipping: loom knowledge/memory command"
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

# Check for grep usage - warn and guide to rg (ripgrep)
if uses_grep "$STRIPPED_COMMAND"; then
	debug "WARNED: grep detected"
	jq -nc --arg ctx "LOOM_HOOK_WARN: STOP — do NOT run this 'grep' command. CLAUDE.md rule 8 bans 'grep' in this project. Cancel it and redo the search NOW with 'rg' (ripgrep). Translate before retrying: grep -rn \"pat\" path → rg -n \"pat\" path" \
		'{hookSpecificOutput: {hookEventName: "PreToolUse", additionalContext: $ctx}}'
	exit 0
fi

# Check for find usage - warn and guide to fd
if uses_find "$STRIPPED_COMMAND"; then
	debug "WARNED: find detected"
	jq -nc --arg ctx "LOOM_HOOK_WARN: STOP — do NOT run this 'find' command. CLAUDE.md rule 8 bans 'find' in this project. Cancel it and redo the search NOW with 'fd'. Translate before retrying: find . -name \"*.txt\" → fd -e txt" \
		'{hookSpecificOutput: {hookEventName: "PreToolUse", additionalContext: $ctx}}'
	exit 0
fi

# Command is allowed as-is
debug "Allowing command as-is"
exit 0
