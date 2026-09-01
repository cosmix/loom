#!/usr/bin/env bash
# spawn-guard.sh - PreToolUse hook (matchers: Task, Agent) that makes subagent
# model selection visible and explicit.
#
# An untyped Task/Agent spawn (no subagent_type, or a generic placeholder type)
# inherits the SPAWNING session's model. On an opus stage session that silently
# makes every worker opus, defeating CLAUDE.md Rule 7/hard-stop-6's cheapest-
# capable-tier delegation. This hook:
#   1. DENIES an untyped spawn outright (live loom stage session only; warns
#      everywhere else - see the ENFORCEMENT GATE below).
#   2. FILLS IN the model from the agent's own definition (or a built-in
#      table) when a typed spawn omits `model`, so every spawn ends up with an
#      explicit, auditable model.
#   3. WARNS (never denies) when an explicit `model` escalates above the
#      agent's defined tier, or when a loom-* subagent's prompt is missing the
#      Rule 5 preamble.
#   4. RECORDS every typed spawn to $LOOM_WORK_DIR/subagents/<stage-id>/spawns.jsonl
#      (the state directory - .loom/work, or the legacy .work) so
#      `loom subagents` can report on model usage across a stage.
#
# Input: JSON from stdin - {"tool_name": "Task"|"Agent", "tool_input": {...},
#        "agent_id": ..., "agent_type": ...}
# Exit codes: 0 = allow (optionally with a warning/rewrite), 2 = block
#
# Output (allow, no issue): nothing on stdout.
# Output (allow, model filled in and/or a warning): one JSON object -
#   {"hookSpecificOutput": {"hookEventName": "PreToolUse",
#     "permissionDecision": "allow", "updatedInput": {...},
#     "additionalContext": "LOOM_HOOK_WARN: ..."}}
#   (permissionDecision/updatedInput and additionalContext each appear only
#   when applicable.)
# Output (block): human-readable reason on stderr.

set -euo pipefail

source "$(dirname "$0")/_common.sh"

if command -v gtimeout &>/dev/null; then
	INPUT_JSON=$(gtimeout 1 cat 2>/dev/null || true)
elif command -v timeout &>/dev/null; then
	INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)
else
	INPUT_JSON=$(cat 2>/dev/null || true)
fi

TOOL_NAME=$(printf '%s' "$INPUT_JSON" | jq -r '.tool_name // empty' 2>/dev/null || true)
case "$TOOL_NAME" in
Task | Agent) ;;
*) exit 0 ;;
esac

AGENT_TYPE_REQ=$(printf '%s' "$INPUT_JSON" | jq -r '.tool_input.subagent_type // empty' 2>/dev/null || true)
MODEL_REQ=$(printf '%s' "$INPUT_JSON" | jq -r '.tool_input.model // empty' 2>/dev/null || true)
DESCRIPTION=$(printf '%s' "$INPUT_JSON" | jq -r '.tool_input.description // empty' 2>/dev/null || true)
PROMPT=$(printf '%s' "$INPUT_JSON" | jq -r '.tool_input.prompt // empty' 2>/dev/null || true)
TOOL_INPUT=$(printf '%s' "$INPUT_JSON" | jq -c '.tool_input // null' 2>/dev/null || true)
[[ -n "$TOOL_INPUT" ]] || TOOL_INPUT="null"

CALLER=$(printf '%s' "$INPUT_JSON" | jq -r '.agent_id // empty' 2>/dev/null || true)
if [[ -z "$CALLER" ]]; then
	CALLER=$(printf '%s' "$INPUT_JSON" | jq -r '.agent_type // empty' 2>/dev/null || true)
fi
[[ -n "$CALLER" ]] || CALLER="main"

PREAMBLE_LINE='CLAUDE.md is already in your context; the rules below are the ones that bind you as a subagent. The knowledge you need for this task is quoted in this brief - do not open doc/loom/knowledge/ unless the brief says a pull came back empty.'

# --- THE ENFORCEMENT GATE ---------------------------------------------------
#
# This hook installs globally at ~/.claude/hooks/loom/ and runs in every
# Claude Code session on the machine, loom or not. LOOM_STAGE_ID alone is NOT
# sufficient to scope it to a live stage: that variable leaks into ordinary,
# non-loom sessions (a prior loom run exported it into the shell it was
# started from, and the value survives into whatever runs next there) - see
# _common.sh's loom_is_subagent header and doc/loom/knowledge/mistakes/
# session-identity-env.md for the same class of leaked-env-var mistake. A
# hook gated on LOOM_STAGE_ID alone would hard-block an untyped spawn on a
# plain branch with no live orchestrator anywhere in the process tree - no
# escape hatch, no orchestrator to fix it. Requiring LOOM_MAIN_AGENT_PID to
# additionally be a LIVE ancestor of THIS process (is_ancestor, from
# _common.sh) is what proves a real loom stage session is actually running
# above us right now, not just that the variable is set. Only when BOTH hold
# does anything below ever deny; everywhere else, the same checks fire but
# every would-be denial degrades to a LOOM_HOOK_WARN and the call proceeds.
GATE_PASSED=0
if [[ -n "${LOOM_STAGE_ID:-}" && -n "${LOOM_MAIN_AGENT_PID:-}" ]] && is_ancestor "$LOOM_MAIN_AGENT_PID"; then
	GATE_PASSED=1
fi

# --- Model resolution helpers ------------------------------------------------

# read_frontmatter_model <path> - Echo the `model:` value from <path>'s YAML
# frontmatter (between the first `---` line and the next `---` line). Returns
# 1 when the file is missing/unreadable, has no frontmatter, or no `model:`
# key appears inside it.
read_frontmatter_model() {
	local file="$1"
	[[ -n "$file" && -f "$file" && -r "$file" ]] || return 1

	# Cap the walk at a small, generous line count: this runs line-by-line in
	# bash on the critical path of every spawn, and a large file whose first
	# line happens to be `---` but never closes the frontmatter would
	# otherwise be read to its end. "No closing `---` by then" is treated the
	# same as "no frontmatter at all" - unresolvable.
	local max_lines=100
	local line first=1 in_fm=0 model_val="" count=0
	while IFS= read -r line || [[ -n "$line" ]]; do
		count=$((count + 1))
		if [[ $first -eq 1 ]]; then
			first=0
			[[ "$line" == "---" ]] || return 1
			in_fm=1
			continue
		fi
		if [[ $in_fm -eq 1 ]]; then
			[[ "$line" == "---" ]] && break
			if [[ "$line" =~ ^model:[[:space:]]*(.*)$ ]]; then
				model_val="${BASH_REMATCH[1]}"
				model_val="${model_val%\"}"
				model_val="${model_val#\"}"
				model_val="${model_val%\'}"
				model_val="${model_val#\'}"
				model_val="${model_val%$'\r'}"
			fi
		fi
		((count >= max_lines)) && return 1
	done <"$file"

	[[ -n "$model_val" ]] || return 1
	printf '%s' "$model_val"
}

# resolve_defined_tier <agent-type> - Resolve the model tier from the agent's
# own definition file, checked at <cwd>/.claude/agents/<type>.md then
# ~/.claude/agents/<type>.md, falling back to a built-in table for types that
# ship with no definition file. Sets globals RESOLVED_TIER (the model string)
# and RESOLVE_SOURCE ("definition" or "table") and returns 0 on success; on
# failure both globals are set to "" and it returns 1 - callers must not warn
# in that case, since an unresolvable definition means the tier truly cannot
# be known.
#
# This sets globals instead of echoing because both callers need TWO pieces
# of information (the tier AND where it came from), and a function invoked
# via command substitution ($(...)) runs in a SUBSHELL - any global it
# assigns dies with that subshell and never reaches the caller. Callers MUST
# call this directly (never wrap it in `$(...)`) and read RESOLVED_TIER /
# RESOLVE_SOURCE from the parent shell afterward.
resolve_defined_tier() {
	local agent_type="$1" val
	RESOLVED_TIER=""
	RESOLVE_SOURCE=""

	# agent_type is caller-controlled (.tool_input.subagent_type) and becomes a
	# path component below - reject anything that is not a safe path segment
	# BEFORE it is ever interpolated, the same character-class guard
	# LOOM_STAGE_ID gets at record_spawn (below) and AGENT_ID gets in
	# subagent-start.sh. A type that fails this is not a valid agent type: it
	# cannot resolve a definition either way, so resolution just fails here -
	# the same "unresolvable definition" outcome as a type with no def file at
	# all. Do not deny on it and do not substitute a different type.
	case "$agent_type" in
	*[!A-Za-z0-9._-]* | "")
		loom_debug "spawn-guard: agent_type is not a safe path component, skipping definition lookup: $agent_type"
		return 1
		;;
	esac

	if val=$(read_frontmatter_model "$(pwd)/.claude/agents/${agent_type}.md" 2>/dev/null) && [[ -n "$val" ]]; then
		RESOLVED_TIER="$val"
		RESOLVE_SOURCE="definition"
		return 0
	fi
	if val=$(read_frontmatter_model "${HOME:-}/.claude/agents/${agent_type}.md" 2>/dev/null) && [[ -n "$val" ]]; then
		RESOLVED_TIER="$val"
		RESOLVE_SOURCE="definition"
		return 0
	fi

	case "$agent_type" in
	Explore | claude-code-guide)
		RESOLVED_TIER="sonnet"
		RESOLVE_SOURCE="table"
		return 0
		;;
	esac

	return 1
}

# tier_rank <model> - Echo the tier's rank on the haiku < sonnet < opus < fable
# order, or -1 for a model string this scale does not recognize (a raw model
# ID rather than a tier name). Callers must treat -1 as "cannot compare", not
# as the lowest tier.
tier_rank() {
	case "$1" in
	haiku) echo 0 ;;
	sonnet) echo 1 ;;
	opus) echo 2 ;;
	fable) echo 3 ;;
	*) echo -1 ;;
	esac
}

# --- 2. DENY: UNTYPED SPAWN --------------------------------------------------
#
# Not converted to a warning by any other switch: it fully follows the
# ENFORCEMENT GATE above (deny when the gate passes, warn-and-allow when it
# does not) and has no additional override.
UNTYPED_MSG=$(cat <<'EOF'
Untyped spawn inherits the model of the spawning session. Use loom-software-engineer (sonnet, default) / loom-senior-software-engineer (opus) / loom-code-reviewer / loom-advisor (fable, read-only) / loom-codex-forwarder / Explore. Pass `model` only to escalate, and record why.
EOF
)

case "$AGENT_TYPE_REQ" in
"" | general-purpose | claude | Plan)
	if [[ $GATE_PASSED -eq 1 ]]; then
		loom_debug "DEBUG: BLOCKED untyped spawn agent_type='${AGENT_TYPE_REQ}'"
		{
			printf '⛔ BLOCKED: untyped subagent spawn.\n\n'
			printf '%s\n' "$UNTYPED_MSG"
		} >&2
		exit 2
	fi
	jq -nc --arg ctx "LOOM_HOOK_WARN: ${UNTYPED_MSG}" \
		'{hookSpecificOutput: {hookEventName: "PreToolUse", additionalContext: $ctx}}'
	exit 0
	;;
esac

# --- 3. NO MODEL -> FILL IT IN, or 4. EXPLICIT MODEL -> CHECK ESCALATION ----
MODEL=""
MODEL_SOURCE=""
NEEDS_REWRITE=0
WARN_ESCALATION=""
RESOLVED_TIER=""
RESOLVE_SOURCE=""

if [[ -z "$MODEL_REQ" ]]; then
	if resolve_defined_tier "$AGENT_TYPE_REQ"; then
		MODEL="$RESOLVED_TIER"
		MODEL_SOURCE="$RESOLVE_SOURCE"
		NEEDS_REWRITE=1
	fi
else
	MODEL="$MODEL_REQ"
	MODEL_SOURCE="explicit"

	if resolve_defined_tier "$AGENT_TYPE_REQ"; then
		DEFINED_TIER="$RESOLVED_TIER"
		req_rank=$(tier_rank "$MODEL_REQ")
		def_rank=$(tier_rank "$DEFINED_TIER")
		# -1 means "cannot compare" (e.g. a raw model id, not a tier name) -
		# never warn on an unresolvable comparison, only on a proven escalation.
		if [[ "$req_rank" -ge 0 && "$def_rank" -ge 0 && "$req_rank" -gt "$def_rank" ]]; then
			WARN_ESCALATION="explicit model '${MODEL_REQ}' on ${AGENT_TYPE_REQ} (defined tier: ${DEFINED_TIER}) is an escalation above the agent's tier - Rule 7 point 4 requires evidence; record it with loom memory decision"
		fi
	fi
fi

# --- 5. WARN: MISSING SUBAGENT PREAMBLE -------------------------------------
#
# loom-codex-forwarder is EXCLUDED: codex reads AGENTS.md, never CLAUDE.md, so
# prepending the Rule 5 preamble to a codex prompt is a documented mistake
# that sends codex paging the whole knowledge corpus instead of working.
WARN_PREAMBLE=""
if [[ $GATE_PASSED -eq 1 && "$AGENT_TYPE_REQ" == loom-* && "$AGENT_TYPE_REQ" != "loom-codex-forwarder" ]]; then
	if [[ "$PROMPT" != *"$PREAMBLE_LINE"* ]]; then
		WARN_PREAMBLE="subagent_type ${AGENT_TYPE_REQ} prompt is missing the Rule 5 preamble - its first line must be exactly '${PREAMBLE_LINE}'"
	fi
fi

WARN_TEXT=""
if [[ -n "$WARN_ESCALATION" ]]; then WARN_TEXT="$WARN_ESCALATION"; fi
if [[ -n "$WARN_PREAMBLE" ]]; then
	if [[ -n "$WARN_TEXT" ]]; then WARN_TEXT="$WARN_TEXT | $WARN_PREAMBLE"; else WARN_TEXT="$WARN_PREAMBLE"; fi
fi

# emit_result - Print at most ONE hookSpecificOutput JSON object combining
# whatever applies: the rule-3 model rewrite, the rule-4/5 warning text, both,
# or neither (in which case nothing is printed - a silent allow).
emit_result() {
	if [[ $NEEDS_REWRITE -eq 1 && -n "$WARN_TEXT" ]]; then
		jq -nc --argjson ti "$TOOL_INPUT" --arg model "$MODEL" --arg ctx "LOOM_HOOK_WARN: ${WARN_TEXT}" \
			'{hookSpecificOutput: {hookEventName: "PreToolUse", permissionDecision: "allow", updatedInput: ($ti + {model: $model}), additionalContext: $ctx}}'
	elif [[ $NEEDS_REWRITE -eq 1 ]]; then
		jq -nc --argjson ti "$TOOL_INPUT" --arg model "$MODEL" \
			'{hookSpecificOutput: {hookEventName: "PreToolUse", permissionDecision: "allow", updatedInput: ($ti + {model: $model})}}'
	elif [[ -n "$WARN_TEXT" ]]; then
		jq -nc --arg ctx "LOOM_HOOK_WARN: ${WARN_TEXT}" \
			'{hookSpecificOutput: {hookEventName: "PreToolUse", additionalContext: $ctx}}'
	fi
	return 0
}
emit_result

# --- 6. RECORD THE SPAWN -----------------------------------------------------
#
# Contract C1: `loom subagents` reads this file - key order and names below
# must not change. Write discipline mirrors subagent-stop.sh:125-186 exactly:
# plain mkdir/redirection (never a Rust/loom CLI path - the state directory
# is a SYMLINK inside a worktree and loom's safe-write opens roots
# O_NOFOLLOW), a symlinked target is refused, and every step is best-effort
# so a recording failure can never change the decision already made above.
record_spawn() {
	local work_dir="${LOOM_WORK_DIR:-}" stage_id="${LOOM_STAGE_ID:-}"
	[[ -n "$work_dir" && -n "$stage_id" ]] || return 0

	case "$stage_id" in
	*[!A-Za-z0-9._-]* | "")
		loom_debug "spawn-guard: skipping record - LOOM_STAGE_ID has unsafe characters: $stage_id"
		return 0
		;;
	esac

	local dir="${work_dir}/subagents/${stage_id}"
	mkdir -p -m 700 "$dir" 2>/dev/null || return 0
	chmod 700 "$dir" 2>/dev/null || true

	local file="${dir}/spawns.jsonl"
	if [[ -L "$file" ]]; then
		loom_debug "spawn-guard: skipping record - $file is a symlink"
		return 0
	fi

	local ts line
	ts=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")
	line=$(jq -nc \
		--arg ts "$ts" \
		--arg stage_id "$stage_id" \
		--arg session_id "${LOOM_SESSION_ID:-}" \
		--arg caller "$CALLER" \
		--arg agent_type "$AGENT_TYPE_REQ" \
		--arg model "${MODEL:-}" \
		--arg model_source "${MODEL_SOURCE:-}" \
		--arg description "${DESCRIPTION:-}" \
		'{ts: $ts, stage_id: $stage_id, session_id: $session_id, caller: $caller, agent_type: $agent_type, model: $model, model_source: $model_source, description: $description}' \
		2>/dev/null || true)

	if [[ -n "$line" ]]; then
		printf '%s\n' "$line" >>"$file" 2>/dev/null ||
			loom_debug "spawn-guard: ledger append failed for $file"
		chmod 600 "$file" 2>/dev/null || true
	else
		loom_debug "spawn-guard: skipping record - jq -n failed for caller=$CALLER agent_type=$AGENT_TYPE_REQ"
	fi
	return 0
}
record_spawn

exit 0
