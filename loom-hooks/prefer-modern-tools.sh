#!/usr/bin/env bash
# prefer-modern-tools.sh - PreToolUse hook to guide CLI tool selection
#
# This hook intercepts Bash commands and provides guidance:
#
# For grep: redo the search with 'rg' (ripgrep) instead of 'grep'.
# For find: redo the search with 'fd' instead of 'find'.
# For a single-file `cat`, `sed -i`, `ls <path>`, and `head`/`tail` with a
# file operand: point at the tool CLAUDE.md rule 8 names for that job (Read,
# Edit, fd, Read respectively).
#
# Claude Code's native Grep/Glob tools were removed, so 'rg' and 'fd' are
# now the canonical replacements — not just shell-pipeline fallbacks.
#
# Per CLAUDE.md rule 8:
#   "Search with `rg` (text) and `fd` (files) — never `grep` or `find`."
#   `cat`/`head`/`tail` -> Read tool; `sed`/`awk` -> Edit tool; `ls`/`find` -> fd.
#
# Every tool family below warns AT MOST ONCE per session (ledger kind
# "tools", loom-hooks/_read_discipline.sh's _loom_ledger_file) so a long
# session doing the same thing repeatedly is not spammed once the agent has
# already been told.
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
# and loom_tokens_invoke(). _read_discipline.sh brings in _loom_ledger_file,
# _loom_sanitize_agent_id (both defined there) and _loom_polls_count /
# _loom_ledger_append (from _read_ledger.sh, which it sources) for the
# once-per-session ledger below.
source "$(dirname "$0")/_common.sh"
source "$(dirname "$0")/_read_discipline.sh"
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

# --- Once-per-session ledger (kind "tools") --------------------------------
# _pmt_ledger echoes the ledger path for THIS session, resolved once and
# cached in PMT_LEDGER. _pmt_family_warned/_pmt_mark_family let each family
# check below fire at most once per session without re-deriving the path.
PMT_LEDGER=""
_pmt_ledger() {
	if [[ -z "$PMT_LEDGER" ]]; then
		local raw_agent_id payload_sid agent_id
		raw_agent_id=$(echo "$INPUT_JSON" | jq -r '.agent_id // empty' 2>/dev/null || true)
		payload_sid=$(echo "$INPUT_JSON" | jq -r '.session_id // empty' 2>/dev/null || true)
		agent_id=$(_loom_sanitize_agent_id "$raw_agent_id")
		PMT_LEDGER=$(_loom_ledger_file "tools" "$agent_id" "${payload_sid:-unknown}")
	fi
	printf '%s' "$PMT_LEDGER"
}

_pmt_family_warned() {
	[[ "$(_loom_polls_count "$(_pmt_ledger)" "$1")" -gt 0 ]]
}

_pmt_mark_family() {
	_loom_ledger_append "$(_pmt_ledger)" "$1"
}

# --- Segment introspection shared by the new family checks ------------------

# _pmt_find_segment <basename-ere> - echo the command-word index of the FIRST
# segment invoking <basename-ere> (same basename-matching rule as
# loom_tokens_invoke), or return 1 when no segment matches.
_pmt_find_segment() {
	local pattern="$1"
	local re="^(${pattern})$"
	local n=${#LOOM_TOKENS[@]} i=0 at_cmd_pos=1 j base
	while ((i < n)); do
		if [[ "${LOOM_TOKENS[$i]}" == "%%SEP%%" ]]; then
			at_cmd_pos=1
			i=$((i + 1))
			continue
		fi
		if [[ $at_cmd_pos -eq 1 ]] && j=$(loom_tokens_command_word_index "$i"); then
			base="${LOOM_TOKENS[$j]##*/}"
			if [[ "$base" =~ $re ]]; then
				printf '%s' "$j"
				return 0
			fi
		fi
		at_cmd_pos=0
		i=$((i + 1))
	done
	return 1
}

# _pmt_segment_operand_count <cmd-word-index> - count non-flag argv tokens
# after the command word, before the segment's next %%SEP%% or the end.
_pmt_segment_operand_count() {
	local j="$1" n=${#LOOM_TOKENS[@]} i=$((j + 1)) tok count=0
	while ((i < n)) && [[ "${LOOM_TOKENS[$i]}" != "%%SEP%%" ]]; do
		tok="${LOOM_TOKENS[$i]}"
		case "$tok" in
		-*) ;;
		*) count=$((count + 1)) ;;
		esac
		i=$((i + 1))
	done
	printf '%s' "$count"
}

_pmt_segment_has_operand() {
	[[ "$(_pmt_segment_operand_count "$1")" -gt 0 ]]
}

# _pmt_cat_operand_has_redirection - true when the FIRST standalone `cat`
# word in $STRIPPED_COMMAND is itself followed, before the next "hard"
# segment separator (`;`, `&`, `|`, backtick, `(`, `)`, or a newline) or the
# end of the command, by a `<` or `>` character. Used only to keep the
# `cat <file>` rule to the plain, unredirected shape it is meant for -
# `cat file.txt && make > out.log` must still warn, since the `>` there
# belongs to `make`'s segment, past the `&&` hard separator, not to cat's.
#
# The tokenizer folds `<`/`>` into the same "%%SEP%%" sentinel as the hard
# separators (_common.sh's _loom_tokenize_walk), so LOOM_TOKENS keeps no
# trace of which character ended a segment - scoping by token index alone
# (the way _pmt_segment_operand_count scopes operands) cannot tell `cat a
# > b` apart from `cat a && b`. This raw-text scan is the only way left to
# tell them apart, the same trick _pmt_grep_is_pipeline_filter uses to
# recover the pipe character the tokenizer already discarded.
_pmt_cat_operand_has_redirection() {
	local text="$STRIPPED_COMMAND" pattern segment
	pattern=$'(^|[[:space:];&|()`\n])cat([[:space:]]([^;&|()`\n]*))?'
	if [[ "$text" =~ $pattern ]]; then
		segment="${BASH_REMATCH[3]:-}"
		case "$segment" in
		*'<'* | *'>'*) return 0 ;;
		esac
	fi
	return 1
}

# _pmt_head_tail_has_file <cmd-word-index> - true when a head/tail segment
# names an actual file operand rather than reading from a pipe: skips known
# bound flags (and their argument for -n/-c/--lines/--bytes) before deciding
# whether a remaining bare word is a file. `some_command | tail -50` has none.
_pmt_head_tail_has_file() {
	local j="$1" n=${#LOOM_TOKENS[@]} i=$((j + 1)) tok
	while ((i < n)) && [[ "${LOOM_TOKENS[$i]}" != "%%SEP%%" ]]; do
		tok="${LOOM_TOKENS[$i]}"
		case "$tok" in
		-n | -c | --lines | --bytes)
			i=$((i + 2))
			continue
			;;
		-*)
			i=$((i + 1))
			continue
			;;
		*)
			return 0
			;;
		esac
	done
	return 1
}

# _pmt_grep_is_pipeline_filter - true when a real grep invocation is (a)
# preceded by a single pipe character (never `||`) and (b) has at most one
# non-flag argument in its own segment (just the search pattern, no file
# operand) - i.e. it is filtering another command's stdout, the pattern Rule
# 14 asks agents to pipe verbose output through, not the anti-pattern Rule 8
# targets. Raw-text pipe detection: the tokenizer collapses `;`, `&&`, `|`,
# and redirections to the same "%%SEP%%" sentinel, so the separator's actual
# character survives only in the original text.
_pmt_grep_is_pipeline_filter() {
	[[ $TOKENIZED -eq 1 ]] || return 1
	echo "$STRIPPED_COMMAND" | grep -qE '(^|[^|])\|[[:space:]]*(/usr/bin/|/bin/)?grep([[:space:]]|$)' || return 1
	local j
	j=$(_pmt_find_segment 'grep') || return 1
	[[ "$(_pmt_segment_operand_count "$j")" -le 1 ]]
}

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
# not installed on this machine (then proceeding with grep is the only
# option), or grep is only filtering another command's piped stdout with no
# file operand of its own (_pmt_grep_is_pipeline_filter).
if uses_grep "$STRIPPED_COMMAND" && ! _pmt_grep_is_pipeline_filter; then
	if _pmt_family_warned "grep"; then
		debug "grep family already warned this session"
		exit 0
	fi
	_pmt_mark_family "grep"
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
	if _pmt_family_warned "find"; then
		debug "find family already warned this session"
		exit 0
	fi
	_pmt_mark_family "find"
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

# The families below are new tokenized-path-only checks (no unterminated-quote
# fallback exists to preserve, unlike grep/find above): cat with a single
# unredirected file operand, an in-place sed, ls given a path, and head/tail
# given a real file operand.
if [[ $TOKENIZED -eq 1 ]]; then
	if j=$(_pmt_find_segment 'cat') && [[ "$(_pmt_segment_operand_count "$j")" -eq 1 ]] && ! _pmt_cat_operand_has_redirection; then
		if _pmt_family_warned "cat"; then
			debug "cat family already warned this session"
			exit 0
		fi
		_pmt_mark_family "cat"
		debug "WARNED: cat detected"
		jq -nc --arg ctx "LOOM_HOOK_WARN: CLAUDE.md rule 8 prefers the Read tool over 'cat' for reading a single file. Use the Read tool instead." \
			'{hookSpecificOutput: {hookEventName: "PreToolUse", additionalContext: $ctx}}'
		exit 0
	fi

	if loom_tokens_cmd_has_arg 'sed' '(-i.*|--in-place.*)'; then
		if _pmt_family_warned "sed"; then
			debug "sed family already warned this session"
			exit 0
		fi
		_pmt_mark_family "sed"
		debug "WARNED: sed -i detected"
		jq -nc --arg ctx "LOOM_HOOK_WARN: CLAUDE.md rule 8 prefers the Edit tool over 'sed -i' for in-place file edits. Use the Edit tool instead." \
			'{hookSpecificOutput: {hookEventName: "PreToolUse", additionalContext: $ctx}}'
		exit 0
	fi

	if j=$(_pmt_find_segment 'ls') && _pmt_segment_has_operand "$j"; then
		if _pmt_family_warned "ls"; then
			debug "ls family already warned this session"
			exit 0
		fi
		_pmt_mark_family "ls"
		debug "WARNED: ls with a path operand detected"
		jq -nc --arg ctx "LOOM_HOOK_WARN: CLAUDE.md rule 8 prefers 'fd' over 'ls' for listing files at a path. Use fd instead." \
			'{hookSpecificOutput: {hookEventName: "PreToolUse", additionalContext: $ctx}}'
		exit 0
	fi

	for base in head tail; do
		if j=$(_pmt_find_segment "$base") && _pmt_head_tail_has_file "$j"; then
			if _pmt_family_warned "$base"; then
				debug "$base family already warned this session"
				exit 0
			fi
			_pmt_mark_family "$base"
			debug "WARNED: $base with a file operand detected"
			jq -nc --arg ctx "LOOM_HOOK_WARN: CLAUDE.md rule 8 prefers the Read tool over '$base' with a file argument. Reserve '$base' for filtering another command's piped stdout, and use the Read tool to read a file." \
				'{hookSpecificOutput: {hookEventName: "PreToolUse", additionalContext: $ctx}}'
			exit 0
		fi
	done
fi

# Command is allowed as-is
debug "Allowing command as-is"
exit 0
