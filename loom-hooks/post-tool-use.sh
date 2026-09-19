#!/usr/bin/env bash
# post-tool-use.sh - Claude Code PostToolUse hook for loom
#
# Updates a loom stage session's heartbeat and resident-context ceiling.
# It exits when LOOM_STAGE_ID is unset, so it is never active outside a stage.
#
# Input: JSON from stdin (Claude Code passes tool info via stdin)
#   {"tool_name": "Bash", "tool_input": {...}, "transcript_path": "...", ...}
#
# Environment: LOOM_STAGE_ID, LOOM_SESSION_ID, and LOOM_WORK_DIR from loom
# worktree settings.

# Resolve commands through loom's pinned hook PATH when set (LOOM_HOOK_PATH):
# inherited PATH directories can be writable from a sandboxed session.
PATH="${LOOM_HOOK_PATH:-$PATH}"

set -euo pipefail
umask 077

source "$(dirname "$0")/_common.sh"
source "$(dirname "$0")/_progress-classification.sh"
source "$(dirname "$0")/_post-tool-heartbeat.sh"
source "$(dirname "$0")/_read_ledger.sh"
# _read_discipline.sh brings in _loom_ledger_file and _loom_sanitize_agent_id
# for the large-output notice's "bigout" ledger below.
source "$(dirname "$0")/_read_discipline.sh"

# Fallbacks if the canonical resolver fails; in loom/src/models/constants.rs:
# LOOM_DEFAULT_CONTEXT_CEILING_TOKENS mirrors DEFAULT_CONTEXT_CEILING_TOKENS.
# LOOM_DEFAULT_SUBAGENT_CEILING_TOKENS mirrors DEFAULT_SUBAGENT_CEILING_TOKENS.
readonly LOOM_DEFAULT_CONTEXT_CEILING_TOKENS=800000
readonly LOOM_DEFAULT_SUBAGENT_CEILING_TOKENS=800000

# Tail window used to find the last transcript usage record.
readonly LOOM_TRANSCRIPT_WINDOW_BYTES=131072

# Echo the last assistant resident-token count in JSONL, or nothing.
_loom_ctx_usage_from_stream() {
	jq -c 'select(.type == "assistant" and .message.usage != null) |
		((.message.usage.input_tokens // 0) +
		 (.message.usage.cache_creation_input_tokens // 0) +
		 (.message.usage.cache_read_input_tokens // 0))' 2>/dev/null |
		tail -n 1 || true
}

# Read the final assistant usage record without loading a whole transcript.
# A byte-tail can begin mid-record, so discard its first line only when the
# file exceeds the window; otherwise jq would reject the entire stream.
_loom_ctx_last_usage_tokens() {
	local transcript_path="$1"
	[[ -n "$transcript_path" && -r "$transcript_path" ]] || return 0
	command -v jq &>/dev/null || return 0

	local size
	size=$(wc -c <"$transcript_path" 2>/dev/null || echo 0)
	# `wc` pads its output on some platforms; keep the digits only.
	size="${size//[^0-9]/}"

	if [[ -n "$size" ]] && [[ "$size" -gt "$LOOM_TRANSCRIPT_WINDOW_BYTES" ]]; then
		tail -c "$LOOM_TRANSCRIPT_WINDOW_BYTES" "$transcript_path" 2>/dev/null |
			tail -n +2 |
			_loom_ctx_usage_from_stream || true
	else
		_loom_ctx_usage_from_stream <"$transcript_path" || true
	fi
}

# Accept the hidden Rust command's two-u32 wire format.
_loom_ctx_pair_is_valid() {
	local pair="$1" main_value subagent_value
	[[ "$pair" =~ ^(0|[1-9][0-9]*):(0|[1-9][0-9]*)$ ]] || return 1
	main_value="${pair%%:*}"
	subagent_value="${pair#*:}"
	[[ "${#main_value}" -le 10 && "${#subagent_value}" -le 10 ]] || return 1
	[[ "$main_value" -le 4294967295 && "$subagent_value" -le 4294967295 ]]
}

# Atomically cache a valid pair so concurrent readers never see a partial one.
_loom_ctx_cache_pair() {
	local cache_file="$1" pair="$2" temp_file=""
	[[ ! -L "$cache_file" ]] || return 0
	temp_file=$(mktemp "${cache_file}.tmp.XXXXXX" 2>/dev/null) || return 0
	if ! printf '%s\n' "$pair" >"$temp_file" 2>/dev/null; then
		rm -f "$temp_file" 2>/dev/null || true
		return 0
	fi
	chmod 600 "$temp_file" 2>/dev/null || true
	if [[ -L "$cache_file" ]] || ! mv -f "$temp_file" "$cache_file" 2>/dev/null; then
		rm -f "$temp_file" 2>/dev/null || true
	fi
}

# Rust resolves both ceilings once; cache the pair per session before selecting.
_loom_ctx_resolve_ceiling() {
	local cache_file="$1" branch="$2" fallback="$3" pair=""

	if [[ -r "$cache_file" && ! -L "$cache_file" ]]; then
		pair=$(<"$cache_file")
		_loom_ctx_pair_is_valid "$pair" || pair=""
	fi

	if [[ -z "$pair" ]] && command -v "${LOOM_BIN:-loom}" &>/dev/null; then
		pair=$(LOOM_HOOK_CONTEXT=1 loom_run_bounded 3 "${LOOM_BIN:-loom}" hook context-ceilings 2>/dev/null || true)
		if _loom_ctx_pair_is_valid "$pair"; then
			_loom_ctx_cache_pair "$cache_file" "$pair"
		else
			pair=""
		fi
	fi

	if [[ -z "$pair" ]]; then
		printf '%s\n' "$fallback"
	elif [[ "$branch" == "subagent" ]]; then
		printf '%s\n' "${pair#*:}"
	else
		printf '%s\n' "${pair%%:*}"
	fi
}

# Warn once at 80%, then exit 2 at 100%. These files are session-keyed so a
# successor does not inherit a stale cache or already-warned marker.
_loom_ctx_check_main_ceiling() {
	local resident="$1"
	[[ "$resident" =~ ^[0-9]+$ ]] || return 0

	local session_prefix="${HEARTBEAT_DIR}/${LOOM_STAGE_ID}.${LOOM_SESSION_ID}"

	local ceiling
	ceiling=$(_loom_ctx_resolve_ceiling "${session_prefix}.context-ceilings" "main" "$LOOM_DEFAULT_CONTEXT_CEILING_TOKENS")
	[[ "$ceiling" =~ ^[0-9]+$ ]] && [[ "$ceiling" -gt 0 ]] || return 0

	if [[ "$resident" -ge "$ceiling" ]]; then
		echo "CONTEXT CEILING REACHED: ${resident} >= ${ceiling}. Run \`loom handoff --stage ${LOOM_STAGE_ID} --session ${LOOM_SESSION_ID} --trigger ceiling\` now, then stop. Do not start new work." >&2
		exit 2
	fi

	local warn_marker="${session_prefix}.ceiling-warned"
	if [[ "$resident" -ge $((ceiling * 80 / 100)) && ! -e "$warn_marker" && ! -L "$warn_marker" ]]; then
		printf '%s' "$resident" >"$warn_marker" 2>/dev/null || true
		chmod 600 "$warn_marker" 2>/dev/null || true
		echo "Context usage is ${resident}/${ceiling} tokens (>= 80% of the ceiling). Finish the current unit of work and prepare to hand off." >&2
		exit 2
	fi
}

# Warn a subagent once at 80%, then exit 2 at 100%; transcript-specific
# markers keep one subagent from silencing another in the same session.
_loom_ctx_check_subagent_ceiling() {
	local resident="$1"
	[[ "$resident" =~ ^[0-9]+$ ]] || return 0

	local session_prefix="${HEARTBEAT_DIR}/${LOOM_STAGE_ID}.${LOOM_SESSION_ID}"

	local ceiling
	ceiling=$(_loom_ctx_resolve_ceiling "${session_prefix}.context-ceilings" "subagent" "$LOOM_DEFAULT_SUBAGENT_CEILING_TOKENS")
	[[ "$ceiling" =~ ^[0-9]+$ ]] && [[ "$ceiling" -gt 0 ]] || return 0

	if [[ "$resident" -ge "$ceiling" ]]; then
		echo "SUBAGENT CEILING REACHED: write your final report now - files changed, checks run, exactly what remains as numbered next steps - then end your turn. Do not start new work." >&2
		exit 2
	fi

	local marker_key="${TRANSCRIPT_PATH##*/}"
	marker_key="${marker_key//[^A-Za-z0-9._-]/_}"
	local warn_marker="${session_prefix}.subagent-warned-${marker_key:-unknown}"
	if [[ "$resident" -ge $((ceiling * 80 / 100)) && ! -e "$warn_marker" && ! -L "$warn_marker" ]]; then
		printf '%s' "$resident" >"$warn_marker" 2>/dev/null || true
		chmod 600 "$warn_marker" 2>/dev/null || true
		echo "context ${resident}/${ceiling}: finish the unit of work in progress; do not open another file or start another item" >&2
		exit 2
	fi
}

# === LARGE BASH OUTPUT NOTICE ===
# A Bash result whose stdout+stderr crosses this threshold is a long scroll
# the agent will re-read from context instead of a file; Rule 14 asks for
# verbose output to go through a file and a targeted tail/rg read instead.
# Advisory only (never blocks), capped per session (ledger kind "bigout") so
# it does not spam every big test run once the agent has already been told.
readonly LOOM_BIGOUT_THRESHOLD_CHARS=20000
readonly LOOM_BIGOUT_MAX_NOTICES=3

# Sum `.tool_result`/`.tool_response` stdout and stderr character counts in
# one jq call - `length` on a string, never on the raw payload through the
# shell.
_loom_bash_output_size() {
	printf '%s' "$INPUT_JSON" | jq -r '
	  ([.tool_result.stdout, .tool_response.stdout] | map(select(type == "string")) | join("") | length) +
	  ([.tool_result.stderr, .tool_response.stderr] | map(select(type == "string")) | join("") | length)
	' 2>/dev/null || true
}

_loom_bash_output_notice() {
	[[ "$TOOL_NAME" == "Bash" ]] || return 0
	command -v jq &>/dev/null || return 0

	local size
	size=$(_loom_bash_output_size)
	[[ "$size" =~ ^[0-9]+$ ]] || return 0
	((size > LOOM_BIGOUT_THRESHOLD_CHARS)) || return 0

	local raw_agent_id agent_id ledger count
	raw_agent_id=$(echo "$INPUT_JSON" | jq -r '.agent_id // empty' 2>/dev/null || true)
	agent_id=$(_loom_sanitize_agent_id "$raw_agent_id")
	ledger=$(_loom_ledger_file "bigout" "$agent_id" "${LOOM_SESSION_ID:-unknown}")
	count=$(_loom_polls_count "$ledger" "notice")
	((count < LOOM_BIGOUT_MAX_NOTICES)) || return 0

	echo "This Bash result was ${size} characters (stdout+stderr) - send verbose output to a file and read the failing part (\`tail\`, \`rg -B2 -A5 'FAIL|error'\`)." >&2
	_loom_ledger_append "$ledger" "notice"
	return 0
}

# Bound stdin in case the hook runner leaves it open.
INPUT_JSON=$(loom_run_bounded 1 cat 2>/dev/null || true)

TOOL_NAME=$(echo "$INPUT_JSON" | jq -r '.tool_name // empty' 2>/dev/null || true)
TOOL_NAME="${TOOL_NAME:-unknown}"
TOOL_INPUT=$(echo "$INPUT_JSON" | jq -r '.tool_input // empty' 2>/dev/null || true)
READ_FILE_PATH=""
if [[ "$TOOL_NAME" == "Read" ]]; then
	READ_FILE_PATH=$(echo "$TOOL_INPUT" | jq -r '.file_path // empty' 2>/dev/null || true)
fi

COMMAND=""
if [[ "$TOOL_NAME" == "Bash" ]]; then
	COMMAND=$(echo "$TOOL_INPUT" | jq -r '.command // empty' 2>/dev/null || echo "$TOOL_INPUT")
fi

if [[ -z "${LOOM_STAGE_ID:-}" ]] || [[ -z "${LOOM_SESSION_ID:-}" ]] || [[ -z "${LOOM_WORK_DIR:-}" ]]; then
	exit 0
fi

case "$LOOM_STAGE_ID" in
*[!A-Za-z0-9._-]* | "") exit 0 ;;
esac

# The session id also forms ceiling-cache and warning-marker filenames.
case "$LOOM_SESSION_ID" in
*[!A-Za-z0-9._-]* | "") exit 0 ;;
esac

if [[ ! -d "${LOOM_WORK_DIR}" ]]; then
	exit 0
fi

loom_post_tool_write_heartbeat

# Rust revalidates this bounded, best-effort forward lifecycle notification.
if [[ "$TOOL_NAME" == "Bash" && "$COMMAND" == *"codex-forward.sh task"* && -n "$TRANSCRIPT_PATH" ]] \
	&& command -v "${LOOM_BIN:-loom}" &>/dev/null; then
	LOOM_HOOK_CONTEXT=1 loom_run_bounded 5 "${LOOM_BIN:-loom}" hook forward-receipt --transcript "$TRANSCRIPT_PATH" >/dev/null 2>&1 || true
fi

# Result content is intentionally not parsed or stored in shell. The Rust
# adapter receives the original payload and correlates its Read tool-use id
# with the bounded transcript tail before it records any receipt.
if [[ "$TOOL_NAME" == "Read" && -n "$INPUT_JSON" && -n "$READ_FILE_PATH" ]] \
	&& _loom_read_receipt_eligible "$READ_FILE_PATH" && command -v "${LOOM_BIN:-loom}" &>/dev/null; then
	printf '%s' "$INPUT_JSON" | LOOM_HOOK_CONTEXT=1 loom_run_bounded 2 "${LOOM_BIN:-loom}" hook read-receipt --complete >/dev/null 2>&1 || true
fi

# Tool results are not persisted: a shell hook cannot append race-free without
# following paths, and even redacted previews risk retaining private source.

# === POST-COMMIT KNOWLEDGE/MEMORY REMINDER ===
# After a git commit, print a non-blocking knowledge/memory reminder.

remind_knowledge_update() {
	cat >&2 <<'REMINDER'

┌────────────────────────────────────────────────────────────────────┐
│  📝 POST-COMMIT REMINDER: Update Knowledge & Memory                │
├────────────────────────────────────────────────────────────────────┤
│                                                                    │
│  You just committed changes. Before completing this stage:         │
│                                                                    │
│  1. RECORD any mistakes made (MANDATORY if errors occurred):       │
│     loom knowledge update mistakes "## [description]               │
│                                                                    │
│     **What happened:** [describe the mistake]                      │
│     **Why:** [root cause]                                          │
│     **How to avoid:** [prevention strategy]"                       │
│                                                                    │
│  2. CAPTURE session insights:                                      │
│     loom memory note "discovered X about Y"                        │
│     loom memory decision "chose X because Y" --context "details"   │
│                                                                    │
│  3. Memory becomes knowledge via the knowledge-distill stage:      │
│     loom memory show --all              # Review entries           │
│     (the knowledge-distill stage reads this output and curates     │
│     what belongs into doc/loom/knowledge/ - see commands/distill.md)│
│                                                                    │
│  Knowledge persists across sessions - future agents will thank you!│
└────────────────────────────────────────────────────────────────────┘

REMINDER
}

if [[ "$TOOL_NAME" == "Bash" ]] && [[ -n "$COMMAND" ]]; then
	# Shared tokenization strips heredocs and quoted prose, so only a real
	# `git commit` fires; an unparseable Bash command safely does not.
	STRIPPED_COMMAND=$(strip_embedded_content "$COMMAND")
	if loom_tokenize_command "$STRIPPED_COMMAND" && loom_tokens_cmd_has_arg 'git' 'commit'; then
		remind_knowledge_update
	fi
fi

# === EDIT RECORDING (Write/Edit/MultiEdit/NotebookEdit tool calls) ===
# Delegate to the Rust binary, which owns the shared state-directory write under a lock.
# This script only extracts the edited path and forwards it - it must never
# write shared state itself, and a failed/slow record must never fail the
# edit (every call below is suffixed with `|| true`).
#
# MultiEdit carries its target at `.file_path`, the same position Write/Edit
# use - confirmed against Claude Code's published PostToolUse examples, which
# match "Write|Edit|MultiEdit" and read `.tool_input.file_path` for all three.
# NotebookEdit does NOT: its field is `.notebook_path`, confirmed against
# `worktree-file-guard.sh`'s `extract_path()`, which already special-cases the
# same tool for the same reason. Falling back to `.file_path` only guards
# against a future field rename, matching that guard's fallback.
if [[ "$TOOL_NAME" == "Write" || "$TOOL_NAME" == "Edit" || "$TOOL_NAME" == "MultiEdit" || "$TOOL_NAME" == "NotebookEdit" ]] \
	&& command -v "${LOOM_BIN:-loom}" &>/dev/null && command -v jq &>/dev/null; then
	if [[ "$TOOL_NAME" == "NotebookEdit" ]]; then
		EDIT_PATH=$(echo "$TOOL_INPUT" | jq -r '.notebook_path // .file_path // empty' 2>/dev/null || true)
	else
		EDIT_PATH=$(echo "$TOOL_INPUT" | jq -r '.file_path // empty' 2>/dev/null || true)
	fi
	if [[ -n "$EDIT_PATH" ]]; then
		LOOM_HOOK_CONTEXT=1 loom_run_bounded 3 "${LOOM_BIN:-loom}" context record-edit --stage "$LOOM_STAGE_ID" --path "$EDIT_PATH" >/dev/null 2>&1 || true
	fi
fi

# The large-output notice runs after the heartbeat and before the ceiling
# checks, and never exits - it must not disturb either.
_loom_bash_output_notice

# Context ceilings run last; PostToolUse exit 2 is agent guidance after the
# completed tool call, not a block on that call.
if [[ "$IS_SUBAGENT" == "1" ]]; then
	_loom_ctx_check_subagent_ceiling "$RESIDENT_TOKENS"
else
	_loom_ctx_check_main_ceiling "$RESIDENT_TOKENS"
fi

exit 0
