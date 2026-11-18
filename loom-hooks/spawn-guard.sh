#!/usr/bin/env bash
# spawn-guard.sh - PreToolUse hook (matchers: Task, Agent) that makes subagent
# model selection visible and explicit and hands every subagent the Rule 5
# preamble.
#
# An untyped Task/Agent spawn (no subagent_type, or a generic placeholder type)
# inherits the SPAWNING session's model. On an opus stage session that silently
# makes every worker opus, defeating CLAUDE.md Rule 7/hard-stop-6's cheapest-
# capable-tier delegation. This hook:
#   1. ADVISES, on the first spawn of a session, loading the
#      loom-orchestration skill.
#   2. DENIES an untyped spawn outright (live loom stage session only; else it
#      warns and, with no model requested, rewrites the input to model sonnet),
#      and a loom-codex-forwarder or codex:* spawn on the native Codex lane.
#   3. FILLS IN the model from the agent's own definition (or a built-in
#      table) when a typed spawn omits `model`, so every spawn ends up with an
#      explicit, auditable model.
#   4. WARNS (never denies) when an explicit `model` escalates above the
#      agent's defined tier.
#   5. PREPENDS _subagent-preamble.txt to a typed spawn's prompt that lacks
#      it, in every session; inside a stage (the GATE_PASSED gate) also APPENDS
#      an optional worker brief (neither for loom-codex-forwarder or codex:*);
#      a loom-code-reviewer spawn also gets the reviewer brief appended last.
#   6. RECORDS every typed spawn to $LOOM_WORK_DIR/subagents/<stage-id>/spawns.jsonl
#      (.loom/work, or the legacy .work) for `loom subagents` model reports.
#
# Input: JSON from stdin - {"tool_name": "Task"|"Agent", "tool_input": {...},
#        "agent_id": ..., "agent_type": ..., "session_id": ...}
# Exit codes: 0 = allow (optionally with a warning/rewrite), 1 = jq not
# installed (non-blocking error), 2 = block
#
# Output (allow, no issue): nothing on stdout.
# Output (allow, prompt/model updated and/or a warning): one JSON object -
#   {"hookSpecificOutput": {"hookEventName": "PreToolUse",
#     "permissionDecision": "allow", "updatedInput": {...},
#     "additionalContext": "LOOM_HOOK_WARN: ..."}}
#   (updatedInput and additionalContext each appear only when applicable;
#   Claude Code discards an updatedInput that carries no permissionDecision,
#   so every rewrite is an explicit allow.)
# Output (block): human-readable reason on stderr.

# Resolve commands through loom's pinned hook PATH when set (LOOM_HOOK_PATH):
# inherited PATH directories can be writable from a sandboxed session.
PATH="${LOOM_HOOK_PATH:-$PATH}"

set -euo pipefail

source "$(dirname "$0")/_common.sh"
source "$(dirname "$0")/_spawn_policy.sh"
source "$(dirname "$0")/_read_discipline.sh"
loom_warn_no_jq "spawn-guard.sh"

INPUT_JSON=$(loom_read_input 1 2>/dev/null || true)

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

RAW_AGENT_ID=$(printf '%s' "$INPUT_JSON" | jq -r '.agent_id // empty' 2>/dev/null || true)
PAYLOAD_SID=$(printf '%s' "$INPUT_JSON" | jq -r '.session_id // empty' 2>/dev/null || true)
CALLER="$RAW_AGENT_ID"
if [[ -z "$CALLER" ]]; then
	CALLER=$(printf '%s' "$INPUT_JSON" | jq -r '.agent_type // empty' 2>/dev/null || true)
fi
[[ -n "$CALLER" ]] || CALLER="main"

PREAMBLE_LINE='CLAUDE.md is already in your context; the rules below are the ones that bind you as a subagent. The knowledge you need for this task is quoted in this brief - do not open doc/loom/knowledge/ unless the brief says a pull came back empty.'

# --- 1. ADVISE ONCE PER SESSION (per agent, ledger kind `spawns`) -----------
# Shown only once its ledger row is written: an unwritable ledger stays silent.
ADVISORY=""
note_first_spawn() {
	local ledger
	ledger=$(_loom_ledger_file "spawns" "$(_loom_sanitize_agent_id "$RAW_AGENT_ID")" "${PAYLOAD_SID:-unknown}")
	if [[ -s "$ledger" ]]; then return 0; fi
	_loom_ledger_append "$ledger" "advised"
	if [[ -s "$ledger" ]]; then
		ADVISORY='Load the `loom-orchestration` skill before delegating if it is not loaded yet.'
	fi
	return 0
}
note_first_spawn

# hook_context <warn-text> - Echo the additionalContext body: the warning as a
# LOOM_HOOK_WARN line, then the advisory line; either may be absent.
hook_context() {
	local out="" nl=$'\n'
	if [[ -n "$1" ]]; then out="LOOM_HOOK_WARN: $1"; fi
	if [[ -n "$ADVISORY" ]]; then out="${out:+${out}${nl}}${ADVISORY}"; fi
	printf '%s' "$out"
}

# --- THE ENFORCEMENT GATE ---------------------------------------------------
# This hook runs in every Claude Code session, loom or not, and LOOM_STAGE_ID
# leaks into plain sessions from the shell a prior loom run exported it into
# (doc/loom/knowledge/mistakes/session-identity-env.md). Gated on it alone, an
# untyped spawn would be hard-blocked with no orchestrator anywhere to fix it.
# LOOM_MAIN_AGENT_PID must ALSO be a live ancestor of this process
# (is_ancestor, _common.sh): only then does anything below deny; everywhere
# else each would-be denial degrades to a LOOM_HOOK_WARN and the call proceeds.
GATE_PASSED=0
if [[ -n "${LOOM_STAGE_ID:-}" && -n "${LOOM_MAIN_AGENT_PID:-}" ]] && is_ancestor "$LOOM_MAIN_AGENT_PID"; then
	GATE_PASSED=1
fi

if loom_native_lane_spawn "$AGENT_TYPE_REQ" "$GATE_PASSED"; then exit 2; fi # header item 2

# --- 2. DENY: UNTYPED SPAWN --------------------------------------------------
# ENFORCEMENT GATE passes: deny; else warn, and with no model requested rewrite to sonnet.
# With an explicit `model` nothing is inherited; the message says so.
TYPED_AGENTS='loom-software-engineer (sonnet, default) / loom-senior-software-engineer (opus) / loom-software-developer (haiku) / loom-code-reviewer / loom-advisor (fable, read-only) / loom-codex-forwarder / Explore'
if [[ -z "$MODEL_REQ" ]]; then
	UNTYPED_MSG="Untyped spawn inherits the model of the spawning session. Use ${TYPED_AGENTS}. Pass \`model\` only to escalate, and record why."
else
	UNTYPED_MSG="Generic agent type '${AGENT_TYPE_REQ:-none}' spawned with explicit model '${MODEL_REQ}'. Spawn by agent type instead: ${TYPED_AGENTS}; pass \`model\` to a typed agent only to escalate, and record why."
fi

case "$AGENT_TYPE_REQ" in
"" | general-purpose | claude | Plan)
	if [[ $GATE_PASSED -eq 1 ]]; then
		loom_debug "DEBUG: BLOCKED untyped spawn agent_type='${AGENT_TYPE_REQ}'"
		{
			printf '⛔ BLOCKED: untyped subagent spawn.\n\n'
			printf '%s\n' "$UNTYPED_MSG"
			if [[ -n "$ADVISORY" ]]; then printf '%s\n' "$ADVISORY"; fi
		} >&2
		exit 2
	fi
	[[ -n "$MODEL_REQ" ]] || UNTYPED_MSG="Untyped spawn outside a loom stage: loom set model sonnet so it does not inherit the spawning session's model. Pass \`model\` to choose another tier, or spawn by agent type: ${TYPED_AGENTS}."
	printf '%s' "$TOOL_INPUT" | jq -c --arg ctx "$(hook_context "$UNTYPED_MSG")" --arg model "$MODEL_REQ" \
		'{hookSpecificOutput: ({hookEventName: "PreToolUse", additionalContext: $ctx} + (if $model == "" then {permissionDecision: "allow", updatedInput: ((. // {}) + {model: "sonnet"})} else {} end))}'
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
WARN_REVIEWER=""

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

# --- 4b. REVIEWER MODEL FROM CONFIG ------------------------------------------
# loom-code-reviewer runs the operator-configured model and effort over any
# caller `model` (loom_reviewer_model_override, _spawn_policy.sh); a failure keeps the
# frontmatter behavior above and sets WARN_REVIEWER.
EFFORT=""
if [[ "$AGENT_TYPE_REQ" == loom-code-reviewer ]]; then
	LOOM_SPAWN_MODEL="$MODEL" LOOM_SPAWN_EFFORT="$EFFORT"
	LOOM_SPAWN_MODEL_SOURCE="$MODEL_SOURCE" LOOM_SPAWN_NEEDS_REWRITE="$NEEDS_REWRITE"
	LOOM_SPAWN_WARN_ESCALATION="$WARN_ESCALATION" LOOM_SPAWN_WARN_REVIEWER="$WARN_REVIEWER"
	loom_reviewer_model_override "$MODEL_REQ"
	MODEL="$LOOM_SPAWN_MODEL" EFFORT="$LOOM_SPAWN_EFFORT"
	MODEL_SOURCE="$LOOM_SPAWN_MODEL_SOURCE" NEEDS_REWRITE="$LOOM_SPAWN_NEEDS_REWRITE"
	WARN_ESCALATION="$LOOM_SPAWN_WARN_ESCALATION" WARN_REVIEWER="$LOOM_SPAWN_WARN_REVIEWER"
fi

# --- 5. PREPEND THE SUBAGENT PREAMBLE ---------------------------------------
# Every session, gated or not: a typed spawn whose prompt lacks PREAMBLE_LINE
# gets _subagent-preamble.txt, a blank line, then its prompt byte for byte; an
# unreadable file changes nothing and warns instead. Codex-bound types are
# EXCLUDED: codex reads AGENTS.md, never CLAUDE.md, and this preamble sends it
# paging the whole knowledge corpus instead of working.
UPDATED_INPUT="$TOOL_INPUT"
PROMPT_REWRITTEN=0
WARN_PREAMBLE=""
case "$AGENT_TYPE_REQ" in
loom-codex-forwarder | codex:*) ;;
*)
	if [[ "$PROMPT" != *"$PREAMBLE_LINE"* ]] &&
		printf '%s' "$TOOL_INPUT" | jq -e '.prompt | type == "string"' >/dev/null 2>&1; then
		PREAMBLE_TEXT=$(cat -- "$(dirname "$0")/_subagent-preamble.txt" 2>/dev/null) || PREAMBLE_TEXT=""
		if [[ -n "$PREAMBLE_TEXT" ]]; then
			UPDATED_INPUT=$(printf '%s' "$UPDATED_INPUT" | jq -c --arg pre "$PREAMBLE_TEXT" '.prompt = $pre + "\n\n" + .prompt')
			PROMPT_REWRITTEN=1
		else
			WARN_PREAMBLE="subagent_type ${AGENT_TYPE_REQ} prompt is missing the Rule 5 preamble - its first line must be exactly '${PREAMBLE_LINE}'"
		fi
	fi
	;;
esac

WARN_TEXT="$WARN_ESCALATION"
if [[ -n "$WARN_REVIEWER" ]]; then WARN_TEXT="${WARN_TEXT:+${WARN_TEXT} | }${WARN_REVIEWER}"; fi
if [[ -n "$WARN_PREAMBLE" ]]; then WARN_TEXT="${WARN_TEXT:+${WARN_TEXT} | }${WARN_PREAMBLE}"; fi

# Scoped briefs are optional (_spawn_policy.sh): any failure changes nothing.
# The worker brief follows the prompt; the reviewer brief (loom-code-reviewer only) comes last.
loom_worker_brief "$INPUT_JSON" "$TOOL_INPUT" "$GATE_PASSED" "$AGENT_TYPE_REQ"
loom_append_brief "$LOOM_SPAWN_WORKER_BRIEF_JSON"
loom_reviewer_brief "$TOOL_INPUT" "$GATE_PASSED" "$AGENT_TYPE_REQ"
loom_append_brief "$LOOM_SPAWN_REVIEWER_BRIEF_JSON"
if [[ $NEEDS_REWRITE -eq 1 ]]; then
	UPDATED_INPUT=$(printf '%s' "$UPDATED_INPUT" | jq -c --arg model "$MODEL" '. + {model: $model}')
	if [[ -n "$EFFORT" ]]; then
		UPDATED_INPUT=$(printf '%s' "$UPDATED_INPUT" | jq -c --arg effort "$EFFORT" '. + {effort: $effort}')
	fi
fi

# emit_result - Print ONE hookSpecificOutput object with the whole rewrite and
# context, or nothing. The input goes on stdin: a large prompt is no argv item.
emit_result() {
	local ctx
	ctx=$(hook_context "$WARN_TEXT")
	if [[ $NEEDS_REWRITE -eq 1 || $PROMPT_REWRITTEN -eq 1 ]]; then
		printf '%s' "$UPDATED_INPUT" | jq -c --arg ctx "$ctx" '
			{hookSpecificOutput: (
				{hookEventName: "PreToolUse", permissionDecision: "allow", updatedInput: .}
				+ (if $ctx != "" then {additionalContext: $ctx} else {} end)
			)}'
	elif [[ -n "$ctx" ]]; then
		jq -nc --arg ctx "$ctx" \
			'{hookSpecificOutput: {hookEventName: "PreToolUse", additionalContext: $ctx}}'
	fi
	return 0
}
emit_result

# --- 6. RECORD THE SPAWN -----------------------------------------------------
# Contract C1: `loom subagents` reads this file - key order and names below
# must not change. Writes mirror loom_lifecycle_append (_lifecycle.sh): plain
# mkdir/redirection (the state directory is a SYMLINK in a worktree, and loom's
# safe-write opens roots O_NOFOLLOW), a symlinked target is refused, and every
# step is best-effort so recording never changes the decision made above.
record_spawn() {
	local work_dir="${LOOM_WORK_DIR:-}" stage_id="${LOOM_STAGE_ID:-}"
	[[ -n "$work_dir" && -n "$stage_id" ]] || return 0

	case "$stage_id" in
	*[!A-Za-z0-9._-]* | "" | "." | "..")
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

	# The effort the subagent runs at: the reviewer's configured effort (which
	# replaces the caller's in the rewritten input), else the caller's, else the
	# agent definition's default; empty when none is known.
	local effort="${EFFORT:-}"
	[[ -n "$effort" ]] || effort=$(printf '%s' "$TOOL_INPUT" | jq -r '.effort | strings' 2>/dev/null || true)
	[[ -n "$effort" ]] || effort=$(resolve_defined_effort "$AGENT_TYPE_REQ" || true)

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
		--arg effort "$effort" \
		--arg description "${DESCRIPTION:-}" \
		'{ts: $ts, stage_id: $stage_id, session_id: $session_id, caller: $caller, agent_type: $agent_type, model: $model, model_source: $model_source, effort: $effort, description: $description}' \
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
