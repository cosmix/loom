#!/usr/bin/env bash
# read-guard.sh - PreToolUse hook (matcher: Read) enforcing loom's read
# discipline: CLAUDE.md rule 14 ("query before you read... read ranges, not
# files") and rule 17's 400-line file-size ceiling.
#
# All three rules (unbounded read of a large file -> outline instead, repeat
# reads of the same path, and a tier-1 knowledge read in a stage session)
# live in the shared core hooks/_read_discipline.sh, so this hook and
# poll-guard.sh's Bash-side file reads can never drift apart - see that
# file's header for the full rule description.
#
# Input: JSON from stdin - {"tool_name": "Read", "tool_input": {"file_path":
# ..., "offset": ..., "limit": ..., "pages": ...}, "agent_id": ...,
# "session_id": ...}
# Exit codes: 0 = allow (optionally with a LOOM_HOOK_WARN additionalContext),
# 1 = jq not installed (non-blocking error),
# 2 = deny with guidance on stderr (only when the deny switch is enabled AND
# a live loom session is running above this process - see
# loom_hook_deny_or_warn in _read_discipline.sh; absent either, every
# decision below is a warning).

set -euo pipefail

source "$(dirname "$0")/_common.sh"
source "$(dirname "$0")/_read_discipline.sh"
loom_warn_no_jq "read-guard.sh"

# Read stdin under gtimeout (macOS+coreutils), timeout (Linux), or bare cat.
if command -v gtimeout &>/dev/null; then
	INPUT_JSON=$(gtimeout 1 cat 2>/dev/null || true)
elif command -v timeout &>/dev/null; then
	INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)
else
	INPUT_JSON=$(cat 2>/dev/null || true)
fi

TOOL_NAME=$(echo "$INPUT_JSON" | jq -r '.tool_name // empty' 2>/dev/null || true)
if [[ "$TOOL_NAME" != "Read" ]]; then
	exit 0
fi

FILE_PATH=$(echo "$INPUT_JSON" | jq -r '.tool_input.file_path // empty' 2>/dev/null || true)
if [[ -z "$FILE_PATH" ]]; then
	exit 0
fi

OFFSET=$(echo "$INPUT_JSON" | jq -r '.tool_input.offset // empty' 2>/dev/null || true)
LIMIT=$(echo "$INPUT_JSON" | jq -r '.tool_input.limit // empty' 2>/dev/null || true)
PAGES=$(echo "$INPUT_JSON" | jq -r '.tool_input.pages // empty' 2>/dev/null || true)
RAW_AGENT_ID=$(echo "$INPUT_JSON" | jq -r '.agent_id // empty' 2>/dev/null || true)
PAYLOAD_SID=$(echo "$INPUT_JSON" | jq -r '.session_id // empty' 2>/dev/null || true)
AGENT_ID=$(_loom_sanitize_agent_id "$RAW_AGENT_ID")

# PAGES is not an arithmetic-injection vector ($lines is only ever
# string-compared downstream), but it flows unmodified into LINES and from
# there into the TSV read ledger (_loom_ledger_append), so a value carrying a
# tab or newline would still write a malformed ledger row. A real page range
# is "N" or "N-M" (per the Read tool's own documentation); anything else is
# treated as absent, matching how a malformed offset/limit is handled above.
if [[ -n "$PAGES" ]] && [[ ! "$PAGES" =~ ^[0-9]+(-[0-9]+)?$ ]]; then
	loom_debug "read-guard: ignoring malformed pages value: $PAGES"
	PAGES=""
fi

# Determine kind + lines value for this read. `pages` (a PDF page range,
# e.g. "1-5") and offset/limit both make this a bounded "range" read -
# offset/limit do not apply to a PDF per the Read tool's own documentation,
# so `pages` is the range selector for that case instead. Anything else is
# "full", whose `lines` column is the file's own line count, computed once
# via _loom_read_full_lines (which also skips the `wc -l` for a binary/image
# extension) since rule 1 reuses this same value rather than recomputing it.
KIND="full"
LINES=""
if [[ -n "$PAGES" ]]; then
	KIND="range"
	LINES="$PAGES"
elif [[ -n "$OFFSET" || -n "$LIMIT" ]]; then
	# OFFSET/LIMIT come straight from tool_input with no validation upstream,
	# and bash re-evaluates a variable's VALUE as an expression in the
	# arithmetic context below - so a non-numeric value (e.g. a
	# "$(...)" command substitution payload) MUST be treated as absent, never
	# interpolated into $(( )). A value that fails validation degrades to the
	# same "0"/"no limit" behavior as a genuinely absent offset/limit.
	KIND="range"
	OFF=0
	if [[ "$OFFSET" =~ ^[0-9]+$ ]]; then
		OFF="$OFFSET"
	elif [[ -n "$OFFSET" ]]; then
		loom_debug "read-guard: ignoring non-numeric offset: $OFFSET"
	fi
	if [[ "$LIMIT" =~ ^[0-9]+$ ]]; then
		LINES="${OFF}-$((OFF + LIMIT))"
	else
		if [[ -n "$LIMIT" ]]; then
			loom_debug "read-guard: ignoring non-numeric limit: $LIMIT"
		fi
		LINES="${OFF}-"
	fi
else
	LINES=$(_loom_read_full_lines "$FILE_PATH")
fi

loom_debug "FILE_PATH=$FILE_PATH KIND=$KIND LINES=$LINES AGENT_ID=$AGENT_ID"

loom_read_discipline_check "$FILE_PATH" "$KIND" "$LINES" "$AGENT_ID" "${PAYLOAD_SID:-unknown}"

loom_hook_emit_warns
exit 0
