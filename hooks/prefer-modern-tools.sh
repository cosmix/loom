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
# Detection tokenizes the (stripped) command with loom_tokenize_command and
# asks loom_tokens_invoke whether 'grep'/'find' is an actual command word at
# a command position (see _common.sh). This is what git-add-guard.sh already
# does for `git add`. Regex-matching the raw string instead used to flag a
# command that merely quoted the word "grep"/"find" as one argument - a
# codex-forward task prompt discussing "grep -n is banned", or a JavaScript
# `ARR.find((c) => ...)` call embedded in a quoted brief - even though
# neither ever invoked the real command. Token scanning also correctly
# leaves 'rg' alone, since it never matches the 'grep' basename.
#
# rg/fd are doctrine dependencies, not hard requirements: when the command
# invokes grep/find but the preferred replacement is not installed on this
# machine, the hook allows the original command through with a warning that
# names the gap instead of steering toward a tool that would just fail.
#
# Input: JSON from stdin (Claude Code passes tool info via stdin)
#   {"tool_name": "Bash", "tool_input": {"command": "..."}, ...}
#
# Exit codes:
#   0 - Allow the command to proceed (this hook is advisory only)
#   1 - jq not installed (non-blocking error)
#
# Output format when warning:
#   {"hookSpecificOutput": {"hookEventName": "PreToolUse", "additionalContext": "LOOM_HOOK_WARN: ..."}}

set -euo pipefail

# Source shared utilities for strip_embedded_content(), loom_tokenize_command(),
# and loom_tokens_invoke()
source "$(dirname "$0")/_common.sh"
loom_warn_no_jq "prefer-modern-tools.sh"

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

# Tokenize once. On a clean parse this populates the global LOOM_TOKENS array
# so uses_grep/uses_find below can scan argv VALUES instead of regex-matching
# the raw string. TOKENIZED=0 means the string had an unterminated quote (not
# valid bash anyway), so loom_tokenize_command's LOOM_TOKENS can't be trusted -
# uses_grep/uses_find fall back to the pre-tokenizing regex scan in that case,
# same as check_dangerous_patterns does in git-add-guard.sh.
if loom_tokenize_command "$STRIPPED_COMMAND"; then
	TOKENIZED=1
	# Guard ${LOOM_TOKENS[*]} behind a count check: a whitespace-only command
	# tokenizes to ZERO tokens (and still returns 0 from loom_tokenize_command),
	# and under `set -u` bash 3.2 (macOS) errors expanding `[*]` on an empty
	# array - the expansion happens at this call site even when debug is off,
	# same class of bug already fixed in commit-filter.sh.
	if ((${#LOOM_TOKENS[@]} > 0)); then
		debug "Tokenized into ${#LOOM_TOKENS[@]} token(s): ${LOOM_TOKENS[*]}"
	else
		debug "Tokenized into 0 token(s)"
	fi
else
	TOKENIZED=0
	debug "Tokenizer reported an unterminated quote - falling back to the legacy regex scan"
fi

# Skip loom knowledge/memory commands — their text payloads often contain
# words like "find" or "grep" that are not actual command invocations. Token
# scanning above already ignores text payloads on its own, so this is now
# largely redundant on the tokenized path - it stays as a cheap
# belt-and-braces guard for the TOKENIZED=0 fallback below, which has no
# other protection against a loom memory/knowledge body quoting those words.
if echo "$COMMAND" | grep -qE '(^|[;&|[:space:]])loom[[:space:]]+(knowledge|memory)[[:space:]]'; then
	debug "Skipping: loom knowledge/memory command"
	exit 0
fi

# Check if command invokes grep (but not rg). loom_tokens_invoke matches on
# the effective command word's BASENAME, so "/usr/bin/grep" and "grep" both
# match while "rg" never does (it is a different basename entirely, not a
# substring match).
uses_grep() {
	if [[ $TOKENIZED -eq 1 ]]; then
		loom_tokens_invoke 'grep'
		return
	fi
	# Fallback (unterminated quote): preserve the pre-tokenizing regex scan
	# verbatim so protection is never weaker than it was before tokenizing.
	local cmd="$1"
	echo "$cmd" | grep -qE '(^|[|;&[:space:]])(\/usr\/bin\/|\/bin\/)?grep[[:space:]]'
}

# Check if command invokes find (but not fd). Same basename-matching as
# uses_grep. Note this correctly does NOT flag a JavaScript `.find(` call -
# e.g. `ARR.find((c) => c.k === key)` inside a quoted argument - since that
# is a method call, not a command word at a command position.
uses_find() {
	if [[ $TOKENIZED -eq 1 ]]; then
		loom_tokens_invoke 'find'
		return
	fi
	# Fallback (unterminated quote): preserve the pre-tokenizing regex scan
	# verbatim so protection is never weaker than it was before tokenizing.
	local cmd="$1"
	echo "$cmd" | grep -qE '(^|[|;&[:space:]])(\/usr\/bin\/|\/bin\/)?find[[:space:]]'
}

# Check for grep usage - warn and guide to rg (ripgrep), unless rg itself is
# not installed on this machine (then proceeding with grep is the only option).
if uses_grep "$STRIPPED_COMMAND"; then
	if ! command -v rg &>/dev/null; then
		debug "WARNED: grep detected, but rg is not installed"
		jq -nc --arg ctx "LOOM_HOOK_WARN: CLAUDE.md rule 8 prefers 'rg' (ripgrep) over 'grep', but ripgrep is not installed on this machine. Proceed with grep for this search and tell the user to install ripgrep (apt install ripgrep / brew install ripgrep)." \
			'{hookSpecificOutput: {hookEventName: "PreToolUse", additionalContext: $ctx}}'
		exit 0
	fi
	debug "WARNED: grep detected"
	jq -nc --arg ctx "LOOM_HOOK_WARN: STOP — do NOT run this 'grep' command. CLAUDE.md rule 8 bans 'grep' in this project. Cancel it and redo the search NOW with 'rg' (ripgrep). Translate before retrying: grep -rn \"pat\" path → rg -n \"pat\" path" \
		'{hookSpecificOutput: {hookEventName: "PreToolUse", additionalContext: $ctx}}'
	exit 0
fi

# Check for find usage - warn and guide to fd, unless fd itself is not
# installed on this machine (then proceeding with find is the only option).
if uses_find "$STRIPPED_COMMAND"; then
	if ! command -v fd &>/dev/null; then
		debug "WARNED: find detected, but fd is not installed"
		jq -nc --arg ctx "LOOM_HOOK_WARN: CLAUDE.md rule 8 prefers 'fd' over 'find', but fd is not installed on this machine. Proceed with find for this search and tell the user to install fd (apt install fd-find, then symlink fdfind to fd / brew install fd)." \
			'{hookSpecificOutput: {hookEventName: "PreToolUse", additionalContext: $ctx}}'
		exit 0
	fi
	debug "WARNED: find detected"
	jq -nc --arg ctx "LOOM_HOOK_WARN: STOP — do NOT run this 'find' command. CLAUDE.md rule 8 bans 'find' in this project. Cancel it and redo the search NOW with 'fd'. Translate before retrying: find . -name \"*.txt\" → fd -e txt" \
		'{hookSpecificOutput: {hookEventName: "PreToolUse", additionalContext: $ctx}}'
	exit 0
fi

# Command is allowed as-is
debug "Allowing command as-is"
exit 0
