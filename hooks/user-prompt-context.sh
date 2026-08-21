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
# 16 KiB payload ceiling; nothing otherwise. Every failure path exits 0 with no
# output, so a broken or slow retrieval path never disturbs the session:
#   {"hookSpecificOutput": {"hookEventName": "UserPromptSubmit", "additionalContext": "..."}}
#
# A loom session is NOT required. This runs on every prompt in every repository
# on the machine, so it gates on whether this checkout has anything to retrieve
# FROM — a loom work directory, a knowledge tree, or a built context cache —
# rather than on whether a loom stage spawned the session. Inside a stage the
# delegate keys its brief to that stage; outside one it keys it to the
# checkout's working-tree overlay.
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

if ! command -v loom &>/dev/null; then
	exit 0
fi

# Fail open (no output) unless this checkout holds something to retrieve from.
# The walk upward mirrors `WorkDir::new`, which looks for `.work` the way git
# looks for `.git`; `doc/loom/knowledge/` and `.loom/cache/context-v1/` are the
# other two roots the delegate can answer out of (a mapped source graph needs no
# knowledge tree). Pure bash directory tests, no subprocesses - this runs on
# every prompt in every repository, so it must stay close to free.
has_retrievable_context() {
	if [[ -d "${LOOM_WORK_DIR:-}" ]]; then
		return 0
	fi
	local dir="$PWD"
	while [[ -n "$dir" ]]; do
		if [[ -d "$dir/.work" ]] ||
			[[ -d "$dir/doc/loom/knowledge" ]] ||
			[[ -d "$dir/.loom/cache/context-v1" ]]; then
			return 0
		fi
		dir="${dir%/*}"
	done
	return 1
}

if ! has_retrievable_context; then
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

# Re-check the 16 KiB payload ceiling on this side too. The Rust delegate caps
# itself at `config.max_payload_bytes` (default 16384), but this script runs
# against whatever `loom` happens to be on PATH — possibly an older binary
# than the hook script itself, which is why the bound is duplicated here
# rather than trusted entirely to the binary that produced the output: the
# bytes are enforced where they actually enter the session. `wc -c` counts
# BYTES regardless of locale, which `${#OUTPUT}` (characters) would not.
MAX_OUTPUT_BYTES=16384
OUTPUT_BYTES=$(LC_ALL=C printf '%s' "$OUTPUT" | wc -c)
if [[ "$OUTPUT_BYTES" -gt "$MAX_OUTPUT_BYTES" ]]; then
	exit 0
fi

# The delegate already emits exactly one JSON object - print it verbatim.
printf '%s\n' "$OUTPUT"
exit 0
