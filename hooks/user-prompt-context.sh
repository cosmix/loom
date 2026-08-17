#!/usr/bin/env bash
# user-prompt-context.sh - UserPromptSubmit hook for retrieval-backed context
#
# Delegates ALL retrieval logic to the Rust binary via `loom hook user-prompt`.
# This script contains NO retrieval logic of its own, and it must never make a
# model call or a network call — retrieval is pure filesystem + string work,
# entirely on the Rust side.
#
# Input: JSON from stdin (Claude Code passes prompt info via stdin)
#   {"session_id": "...", "prompt": "...", ...}
#
# Output: the delegate's stdout, verbatim, when it produced output within the
# 8 KiB payload ceiling; nothing otherwise. Every failure path exits 0 with no
# output, so a broken or slow retrieval path never disturbs the session:
#   {"hookSpecificOutput": {"hookEventName": "UserPromptSubmit", "additionalContext": "..."}}
#
# Note: hooks/skill-trigger.sh is a SEPARATE UserPromptSubmit hook (Python,
# keyword-based skill suggestions). Two hooks on one event run as separate
# processes, each printing at most one JSON object — this script does not
# merge with or depend on it.

set -euo pipefail
umask 077

# Read JSON input from stdin (Claude Code passes prompt info via stdin)
# Cross-platform timeout: gtimeout (macOS+coreutils), timeout (Linux), or plain cat
if command -v gtimeout &>/dev/null; then
	INPUT_JSON=$(gtimeout 1 cat 2>/dev/null || true)
elif command -v timeout &>/dev/null; then
	INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)
else
	INPUT_JSON=$(cat 2>/dev/null || true)
fi

# Fail open (no output) when there is no loom session to retrieve against.
if [[ -z "${LOOM_WORK_DIR:-}" ]] || [[ -z "${LOOM_STAGE_ID:-}" ]]; then
	exit 0
fi

if ! command -v loom &>/dev/null; then
	exit 0
fi

# Delegate to the Rust binary under a hard command timeout - a hung or slow
# retrieval must never hang the prompt submit path.
OUTPUT=""
if command -v gtimeout &>/dev/null; then
	OUTPUT=$(printf '%s' "$INPUT_JSON" | gtimeout 5 loom hook user-prompt 2>/dev/null || true)
elif command -v timeout &>/dev/null; then
	OUTPUT=$(printf '%s' "$INPUT_JSON" | timeout 5 loom hook user-prompt 2>/dev/null || true)
else
	OUTPUT=$(printf '%s' "$INPUT_JSON" | loom hook user-prompt 2>/dev/null || true)
fi

# A failed, timed-out, or empty delegate call means nothing to inject.
if [[ -z "$OUTPUT" ]]; then
	exit 0
fi

# Re-check the 8 KiB payload ceiling on this side too. The Rust delegate caps
# itself, but this script runs against whatever `loom` happens to be on PATH,
# so the bound is enforced where the bytes actually enter the session rather
# than trusted to the binary that produced them. `wc -c` counts BYTES
# regardless of locale, which `${#OUTPUT}` (characters) would not.
MAX_OUTPUT_BYTES=8192
OUTPUT_BYTES=$(LC_ALL=C printf '%s' "$OUTPUT" | wc -c)
if [[ "$OUTPUT_BYTES" -gt "$MAX_OUTPUT_BYTES" ]]; then
	exit 0
fi

# The delegate already emits exactly one JSON object - print it verbatim.
printf '%s\n' "$OUTPUT"
exit 0
