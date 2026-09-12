#!/usr/bin/env bash
# post-tool-use.sh - Claude Code PostToolUse hook for loom
#
# Called after each tool use to update the heartbeat and check the running
# session's resident context usage against its ceiling.
#
# SCOPE: a SESSION hook, registered per loom stage session by
# loom/src/hooks/config.rs::to_settings_hooks - NOT a global hook. It exits
# below whenever LOOM_STAGE_ID is unset, so it only ever runs for loom stage
# sessions (the main agent AND its Task-tool subagents), never an
# interactive, non-loom Claude Code session.
#
# Input: JSON from stdin (Claude Code passes tool info via stdin)
#   {"tool_name": "Bash", "tool_input": {...}, "transcript_path": "...", ...}
#
# Environment variables (set by loom worktree settings):
#   LOOM_STAGE_ID    - The stage being executed
#   LOOM_SESSION_ID  - The session ID
#   LOOM_WORK_DIR    - Path to the state directory (.loom/work, or the
#                      legacy .work for a workspace that already resolved
#                      to it)
#
# Actions:
#   1. Updates heartbeat in $LOOM_WORK_DIR/heartbeat/<stage-id>.json with the resident
#      token count read from the tail of the live transcript
#   2. After git commits in loom stages, reminds Claude to update knowledge/memory
#   3. Forwards Write/Edit/MultiEdit/NotebookEdit paths to `loom context record-edit`
#   4. Compares resident tokens against the context ceiling and, at 80%/100%
#      of it, tells the agent via exit 2 (see the CONTEXT CEILING section)

set -euo pipefail
umask 077

source "$(dirname "$0")/_common.sh"

# ---------------------------------------------------------------------------
# Context-ceiling helpers. Kept in this file rather than _common.sh, which
# this stage's contract leaves untouched.
# ---------------------------------------------------------------------------

# Ceiling defaults, used only when the canonical Rust resolver is unavailable
# or returns malformed output. A shell script cannot read a Rust constant, so
# these are hand-kept copies and each one carries the name of its counterpart:
#
#   LOOM_DEFAULT_CONTEXT_CEILING_TOKENS  mirrors DEFAULT_CONTEXT_CEILING_TOKENS
#   LOOM_DEFAULT_SUBAGENT_CEILING_TOKENS mirrors DEFAULT_SUBAGENT_CEILING_TOKENS
#
# both in loom/src/models/constants.rs. Change one side and grep the constant
# name to find the other; a drift here means the hook governs the agent against
# a number no other layer uses.
readonly LOOM_DEFAULT_CONTEXT_CEILING_TOKENS=800000
readonly LOOM_DEFAULT_SUBAGENT_CEILING_TOKENS=800000

# How much of a transcript's tail is read to find the last usage record. Cheap
# even for a huge transcript, and large enough to hold many records.
readonly LOOM_TRANSCRIPT_WINDOW_BYTES=131072

# _loom_ctx_usage_from_stream
# Reads JSONL on stdin; echoes the resident token count (input +
# cache_creation + cache_read) of the LAST assistant usage record in it, or
# nothing. Never fails: the caller must get no reading rather than a wrong one.
_loom_ctx_usage_from_stream() {
	jq -c 'select(.type == "assistant" and .message.usage != null) |
		((.message.usage.input_tokens // 0) +
		 (.message.usage.cache_creation_input_tokens // 0) +
		 (.message.usage.cache_read_input_tokens // 0))' 2>/dev/null |
		tail -n 1 || true
}

# _loom_ctx_last_usage_tokens <transcript-path>
# Echoes the resident token count from the LAST assistant usage record of
# <transcript-path>, or nothing if it cannot be determined. Fail-open under
# set -euo pipefail.
#
# Only the last LOOM_TRANSCRIPT_WINDOW_BYTES are read, and the first line of
# that chunk is dropped ONLY when the file is bigger than the window: a
# byte-offset tail routinely slices a JSONL record in half, and jq aborts the
# WHOLE stream on one malformed leading value (verified: unlike a runtime type
# error on a later value, which jq skips and continues past, a parse error
# discards everything). When the whole file fits in the window nothing is torn,
# and dropping a line there would throw away a complete record - the only usage
# record the transcript has, if it holds just one.
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

# _loom_ctx_pair_is_valid <main:subagent>
# Accept exactly the hidden Rust command's two-u32 wire format. Cache files are
# local mutable state, so validate them just as strictly as fresh command output.
_loom_ctx_pair_is_valid() {
	local pair="$1" main_value subagent_value
	[[ "$pair" =~ ^(0|[1-9][0-9]*):(0|[1-9][0-9]*)$ ]] || return 1
	main_value="${pair%%:*}"
	subagent_value="${pair#*:}"
	[[ "${#main_value}" -le 10 && "${#subagent_value}" -le 10 ]] || return 1
	[[ "$main_value" -le 4294967295 && "$subagent_value" -le 4294967295 ]]
}

# _loom_ctx_cache_pair <cache-file> <main:subagent>
# Best-effort same-directory replacement: concurrent main/subagent hook calls
# may both resolve, but neither can expose a partially-written cache document.
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

# _loom_ctx_resolve_ceiling <cache-file> <main|subagent> <fallback>
# The Rust command owns TOML/YAML parsing and returns BOTH ceilings in one call.
# Cache that pair per Loom session, then select the caller's branch locally so
# the main agent and its subagents cannot drift onto independently-read values.
_loom_ctx_resolve_ceiling() {
	local cache_file="$1" branch="$2" fallback="$3" pair=""

	if [[ -r "$cache_file" && ! -L "$cache_file" ]]; then
		pair=$(<"$cache_file")
		_loom_ctx_pair_is_valid "$pair" || pair=""
	fi

	if [[ -z "$pair" ]] && command -v loom &>/dev/null; then
		if command -v gtimeout &>/dev/null; then
			pair=$(gtimeout 3 loom hook context-ceilings 2>/dev/null || true)
		elif command -v timeout &>/dev/null; then
			pair=$(timeout 3 loom hook context-ceilings 2>/dev/null || true)
		else
			pair=$(loom hook context-ceilings 2>/dev/null || true)
		fi
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

# _loom_ctx_check_main_ceiling <resident-tokens>
# MAIN branch: warns once (marker-guarded) at >=80% of the ceiling, then
# hard-blocks every subsequent tool call at >=100%. Exits the whole script
# via `exit 2` when a threshold fires; falls through otherwise.
#
# Both files it writes are keyed on the SESSION, not the stage. Nothing ever
# deletes them - `remove_heartbeat` (monitor/heartbeat.rs) removes only
# <stage>.json - so a stage-keyed pair leaks into the stage's successor
# sessions: the successor would inherit a "already warned" marker it never
# triggered and go from silence straight to the hard block, and it would keep
# resolving a ceiling cached before the operator edited config.toml.
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

# _loom_ctx_check_subagent_ceiling <resident-tokens>
# SUBAGENT branch: warns once at >=80%, then hard-blocks every subsequent tool
# call at >=100%. The 80% marker is keyed on the session AND on the subagent's
# OWN transcript file: several subagents share one stage and one session, so a
# marker keyed any less finely would let the first one to cross 80% silence all
# the others (and, keyed on the stage alone, silence the next session's too).
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

# Read JSON input from stdin (Claude Code passes tool info via stdin)
# Cross-platform timeout: gtimeout (macOS+coreutils), timeout (Linux), or plain cat
if command -v gtimeout &>/dev/null; then
	INPUT_JSON=$(gtimeout 1 cat 2>/dev/null || true)
elif command -v timeout &>/dev/null; then
	INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)
else
	INPUT_JSON=$(cat 2>/dev/null || true)
fi

# Parse tool_name and tool_input from JSON using jq
TOOL_NAME=$(echo "$INPUT_JSON" | jq -r '.tool_name // empty' 2>/dev/null || true)
TOOL_NAME="${TOOL_NAME:-unknown}"
TOOL_INPUT=$(echo "$INPUT_JSON" | jq -r '.tool_input // empty' 2>/dev/null || true)

# For Bash tool, extract the command
COMMAND=""
if [[ "$TOOL_NAME" == "Bash" ]]; then
	COMMAND=$(echo "$TOOL_INPUT" | jq -r '.command // empty' 2>/dev/null || echo "$TOOL_INPUT")
fi

# Validate required environment variables
if [[ -z "${LOOM_STAGE_ID:-}" ]] || [[ -z "${LOOM_SESSION_ID:-}" ]] || [[ -z "${LOOM_WORK_DIR:-}" ]]; then
	# Silently exit if not in loom context
	exit 0
fi

case "$LOOM_STAGE_ID" in
*[!A-Za-z0-9._-]* | "") exit 0 ;;
esac

# The session id goes into filenames too (the ceiling cache and the 80% warn
# marker are keyed per session, see _loom_ctx_check_main_ceiling), so it gets
# the same guard.
case "$LOOM_SESSION_ID" in
*[!A-Za-z0-9._-]* | "") exit 0 ;;
esac

# Validate work directory exists and is accessible
if [[ ! -d "${LOOM_WORK_DIR}" ]]; then
	# Silently exit - work dir may have been cleaned up
	exit 0
fi

# Ensure heartbeat directory exists
HEARTBEAT_DIR="${LOOM_WORK_DIR}/heartbeat"
mkdir -p -m 700 "$HEARTBEAT_DIR" 2>/dev/null || exit 0
chmod 700 "$HEARTBEAT_DIR" 2>/dev/null || exit 0

# Resident context usage for THIS invocation's own transcript, and whether
# this invocation is running under a subagent - both are needed twice below
# (the heartbeat write and the ceiling check at the end), so compute once.
TRANSCRIPT_PATH=$(echo "$INPUT_JSON" | jq -r '.transcript_path // empty' 2>/dev/null || true)
RESIDENT_TOKENS=$(_loom_ctx_last_usage_tokens "$TRANSCRIPT_PATH")

IS_SUBAGENT=0
# This is a per-session hook, already scoped by the three validated LOOM_*
# values above. Unlike globally-installed enforcement hooks it may therefore
# trust a positive harness payload before process ancestry. Agent-team
# teammates inherit the session metadata but are not descendants of the main
# Claude process; ancestry-first classification mistakes them for the parent,
# overwrites the parent's heartbeat, and gives them the parent ceiling.
PAYLOAD_AGENT_VERDICT=$(loom_payload_agent_verdict "$INPUT_JSON")
if [[ "$PAYLOAD_AGENT_VERDICT" == "subagent" ]]; then
	IS_SUBAGENT=1
elif [[ "$PAYLOAD_AGENT_VERDICT" == "unknown" ]] && loom_is_subagent "$INPUT_JSON"; then
	# Payload-less/back-compat callers retain the existing process-tree fallback.
	IS_SUBAGENT=1
fi

# Update heartbeat file in JSON format.
# Build via `jq -n --arg` so a value containing a quote/backslash (e.g. an exotic
# TOOL_NAME) can never produce malformed JSON. Fall back to the heredoc only when
# jq is unavailable — the heartbeat must never be broken by a missing dependency,
# and these values are loom-controlled.
#
# A symlinked heartbeat path is refused - the target must never be written
# through - but that refusal must only skip the heartbeat write itself. It is
# NOT a whole-script exit: the matcher blocks below (post-commit reminder,
# edit recording) are unrelated to the heartbeat and must still run.
HEARTBEAT_FILE="${HEARTBEAT_DIR}/${LOOM_STAGE_ID}.json"
# A judge (LOOM_SESSION_TYPE=adjudication) writes its own heartbeat file,
# separate from the stage session's, since a stage's session: frontmatter
# never names the judge.
if [[ "${LOOM_SESSION_TYPE:-}" == "adjudication" ]]; then
	HEARTBEAT_FILE="${HEARTBEAT_DIR}/${LOOM_STAGE_ID}.adjudication.json"
fi
HEARTBEAT_LOCK_DIR="${HEARTBEAT_FILE}.lock"
if loom_heartbeat_lock_acquire "$HEARTBEAT_LOCK_DIR"; then
	trap 'loom_heartbeat_lock_release "$HEARTBEAT_LOCK_DIR"' EXIT
	# Re-check after acquiring: another writer may have replaced the path while
	# this hook waited.
	if [[ -L "$HEARTBEAT_FILE" ]]; then
		loom_debug "post-tool-use: skipping heartbeat refresh - $HEARTBEAT_FILE is a symlink"
		loom_heartbeat_lock_release "$HEARTBEAT_LOCK_DIR"
		trap - EXIT
	# A judge's ownership is already implied by LOOM_SESSION_TYPE alone: the
	# daemon enforces one live judge per stage, so the ownership gate below
	# applies only to non-judge (stage) sessions.
	elif [[ "${LOOM_SESSION_TYPE:-}" != "adjudication" ]] && ! loom_heartbeat_owner_is_current "$LOOM_WORK_DIR" "$LOOM_STAGE_ID" "$LOOM_SESSION_ID" "$HEARTBEAT_FILE"; then
		loom_debug "post-tool-use: skipping stale heartbeat refresh for session $LOOM_SESSION_ID"
	else
	# The heartbeat's context_tokens/transcript_path belong to the MAIN
	# session's own resident usage exclusively. A subagent's own numbers must
	# never overwrite them - carry the file's existing values forward instead
	# (empty/empty, rendered as null below, if it does not exist yet or
	# cannot be read).
	if [[ "$IS_SUBAGENT" == "1" ]]; then
		HB_CONTEXT_TOKENS_RAW=""
		HB_TRANSCRIPT_PATH_RAW=""
		if [[ -r "$HEARTBEAT_FILE" ]] && command -v jq &>/dev/null; then
			HB_CONTEXT_TOKENS_RAW=$(jq -r '.context_tokens // empty' "$HEARTBEAT_FILE" 2>/dev/null || true)
			HB_TRANSCRIPT_PATH_RAW=$(jq -r '.transcript_path // empty' "$HEARTBEAT_FILE" 2>/dev/null || true)
		fi
	else
		HB_CONTEXT_TOKENS_RAW="$RESIDENT_TOKENS"
		HB_TRANSCRIPT_PATH_RAW="$TRANSCRIPT_PATH"
	fi

	HEARTBEAT_TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")
	HEARTBEAT_JSON=""
	if command -v jq &>/dev/null; then
		HEARTBEAT_JSON=$(jq -n \
			--arg stage_id "$LOOM_STAGE_ID" \
			--arg session_id "$LOOM_SESSION_ID" \
			--arg timestamp "$HEARTBEAT_TIMESTAMP" \
			--arg last_tool "$TOOL_NAME" \
			--arg context_tokens_raw "$HB_CONTEXT_TOKENS_RAW" \
			--arg transcript_path_raw "$HB_TRANSCRIPT_PATH_RAW" \
			'{stage_id: $stage_id, session_id: $session_id, timestamp: $timestamp,
			  context_tokens: (if ($context_tokens_raw | test("^[0-9]+$")) then ($context_tokens_raw | tonumber) else null end),
			  transcript_path: (if $transcript_path_raw == "" then null else $transcript_path_raw end),
			  last_tool: $last_tool, activity: ("Tool executed: " + $last_tool)}' \
			2>/dev/null || true)
	fi

	if [[ -n "$HEARTBEAT_JSON" ]]; then
		loom_heartbeat_atomic_write "$HEARTBEAT_FILE" "$HEARTBEAT_JSON" || \
			loom_debug "post-tool-use: skipping heartbeat refresh - atomic replacement failed"
	else
		HB_CONTEXT_TOKENS_JSON="null"
		[[ "$HB_CONTEXT_TOKENS_RAW" =~ ^[0-9]+$ ]] && HB_CONTEXT_TOKENS_JSON="$HB_CONTEXT_TOKENS_RAW"
		HB_TRANSCRIPT_PATH_JSON="null"
		[[ -n "$HB_TRANSCRIPT_PATH_RAW" ]] && HB_TRANSCRIPT_PATH_JSON="\"${HB_TRANSCRIPT_PATH_RAW}\""
		HEARTBEAT_JSON=$(cat <<EOF
{
  "stage_id": "${LOOM_STAGE_ID}",
  "session_id": "${LOOM_SESSION_ID}",
  "timestamp": "${HEARTBEAT_TIMESTAMP}",
  "context_tokens": ${HB_CONTEXT_TOKENS_JSON},
  "transcript_path": ${HB_TRANSCRIPT_PATH_JSON},
  "last_tool": "${TOOL_NAME}",
  "activity": "Tool executed: ${TOOL_NAME}"
}
EOF
		)
		loom_heartbeat_atomic_write "$HEARTBEAT_FILE" "$HEARTBEAT_JSON" || \
			loom_debug "post-tool-use: skipping heartbeat refresh - atomic replacement failed"
	fi
	fi
	loom_heartbeat_lock_release "$HEARTBEAT_LOCK_DIR"
	trap - EXIT
fi

# Tool results are intentionally not persisted here. A shell hook cannot append
# to a shared path with a race-free no-follow guarantee, and even redacted
# previews risk retaining credentials or private source. The heartbeat above is
# the complete post-tool observability record.

# === POST-COMMIT KNOWLEDGE/MEMORY REMINDER ===
# After a git commit in a loom stage, remind Claude to update knowledge/memory
# This is non-blocking - just a prompt to help capture lessons learned

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

# Check if this was a git commit command
if [[ "$TOOL_NAME" == "Bash" ]] && [[ -n "$COMMAND" ]]; then
	# Detect a real `git commit` invocation (including `git -C <dir> commit`)
	# via the shared tokenizer over the STRIPPED command, exactly as
	# commit-filter.sh:74-83 does - heredoc bodies are stripped BEFORE
	# tokenizing, so a heredoc body whose text happens to start a line with
	# "git commit" (docs, a test fixture, a plan file) is never mistaken for
	# a real invocation. This also means a "commit" appearing only inside a
	# quoted argument's text (e.g. a `loom memory note` body) never fires
	# this, and the check carries no GNU-only \s/\S dependency. A command
	# the tokenizer cannot parse (unterminated quote - not valid bash
	# anyway) simply does not fire the reminder.
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
	&& command -v loom &>/dev/null && command -v jq &>/dev/null; then
	if [[ "$TOOL_NAME" == "NotebookEdit" ]]; then
		EDIT_PATH=$(echo "$TOOL_INPUT" | jq -r '.notebook_path // .file_path // empty' 2>/dev/null || true)
	else
		EDIT_PATH=$(echo "$TOOL_INPUT" | jq -r '.file_path // empty' 2>/dev/null || true)
	fi
	if [[ -n "$EDIT_PATH" ]]; then
		if command -v gtimeout &>/dev/null; then
			gtimeout 3 loom context record-edit --stage "$LOOM_STAGE_ID" --path "$EDIT_PATH" >/dev/null 2>&1 || true
		elif command -v timeout &>/dev/null; then
			timeout 3 loom context record-edit --stage "$LOOM_STAGE_ID" --path "$EDIT_PATH" >/dev/null 2>&1 || true
		else
			loom context record-edit --stage "$LOOM_STAGE_ID" --path "$EDIT_PATH" >/dev/null 2>&1 || true
		fi
	fi
fi

# === CONTEXT CEILING DETECTION ===
# Runs LAST, after the heartbeat write and the two blocks above (unrelated
# to context, and must always run). The tool has already executed by the
# time PostToolUse fires, so exit 2 here blocks nothing - it is purely a
# message to the agent, the documented channel that reaches the model in
# Claude Code 2.1.251 (`additionalContext` may or may not, so unused here).
if [[ "$IS_SUBAGENT" == "1" ]]; then
	_loom_ctx_check_subagent_ceiling "$RESIDENT_TOKENS"
else
	_loom_ctx_check_main_ceiling "$RESIDENT_TOKENS"
fi

exit 0
