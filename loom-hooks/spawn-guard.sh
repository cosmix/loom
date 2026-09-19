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
#   2. DENIES an untyped spawn outright (live loom stage session only; warns
#      everywhere else - see the ENFORCEMENT GATE below).
#   3. FILLS IN the model from the agent's own definition (or a built-in
#      table) when a typed spawn omits `model`, so every spawn ends up with an
#      explicit, auditable model.
#   4. WARNS (never denies) when an explicit `model` escalates above the
#      agent's defined tier.
#   5. PREPENDS _subagent-preamble.txt to a typed spawn's prompt that lacks
#      it, in every session, and APPENDS an optional scoped worker brief.
#   6. RECORDS every typed spawn to $LOOM_WORK_DIR/subagents/<stage-id>/spawns.jsonl
#      (the state directory - .loom/work, or the legacy .work) so
#      `loom subagents` can report on model usage across a stage.
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
source "$(dirname "$0")/_read_discipline.sh"
loom_warn_no_jq "spawn-guard.sh"

INPUT_JSON=$(loom_run_bounded 1 cat 2>/dev/null || true)

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
#
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

# --- Model resolution helpers ------------------------------------------------

# read_frontmatter_model <path> - Echo the `model:` value from <path>'s YAML
# frontmatter (between the first `---` line and the next `---` line). Returns
# 1 when the file is missing/unreadable, has no frontmatter, or no `model:`
# key appears inside it.
read_frontmatter_model() {
	local file="$1"
	[[ -n "$file" && -f "$file" && -r "$file" ]] || return 1

	# Capped: this bash loop is on every spawn's critical path, and frontmatter
	# not closed within the cap counts as no frontmatter at all - unresolvable.
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
# definition file (<cwd>/.claude/agents/<type>.md, then ~/.claude/agents/),
# else a built-in table. On success sets RESOLVED_TIER (the model) and
# RESOLVE_SOURCE ("definition" or "table") and returns 0; on failure both are
# "" and it returns 1, and callers must not warn: the tier cannot be known.
# Globals, not an echo: callers need both values, and a `$(...)` call runs in
# a subshell whose globals never reach the caller - so never wrap it in one.
resolve_defined_tier() {
	local agent_type="$1" val
	RESOLVED_TIER=""
	RESOLVE_SOURCE=""

	# agent_type is caller-controlled and becomes a path component below, so
	# an unsafe segment fails resolution BEFORE interpolation (the guard
	# LOOM_STAGE_ID gets in record_spawn) - never a deny, never a substitute.
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

# tier_rank <model> - Echo the rank on haiku < sonnet < opus < fable, or -1 for
# an unrecognized string (a raw model ID): "cannot compare", not lowest tier.
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
# Follows the ENFORCEMENT GATE only (deny when it passes, else warn-and-allow).
# With an explicit `model` nothing is inherited; the message says so.
TYPED_AGENTS='loom-software-engineer (sonnet, default) / loom-senior-software-engineer (opus) / loom-code-reviewer / loom-advisor (fable, read-only) / loom-codex-forwarder / Explore'
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
	jq -nc --arg ctx "$(hook_context "$UNTYPED_MSG")" \
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
if [[ -n "$WARN_PREAMBLE" ]]; then WARN_TEXT="${WARN_TEXT:+${WARN_TEXT} | }${WARN_PREAMBLE}"; fi

# A scoped brief is optional and must never change the existing decision on a
# missing command, timeout, malformed envelope, or empty selection. Read the
# prompt from TOOL_INPUT during the merge so trailing newlines remain intact.
WORKER_BRIEF_JSON=""
if [[ $GATE_PASSED -eq 1 ]] && command -v "${LOOM_BIN:-loom}" &>/dev/null &&
	printf '%s' "$TOOL_INPUT" | jq -e 'type == "object" and (.prompt | type == "string")' >/dev/null 2>&1; then
	if ! WORKER_BRIEF_ENVELOPE=$(printf '%s' "$INPUT_JSON" | LOOM_HOOK_CONTEXT=1 loom_run_bounded 3 "${LOOM_BIN:-loom}" hook worker-brief 2>/dev/null); then
		loom_debug "spawn-guard: worker-brief timeout or nonzero exit"
	elif [[ -z "$WORKER_BRIEF_ENVELOPE" ]]; then
		loom_debug "spawn-guard: worker-brief empty output"
	else
		if [[ "$WORKER_BRIEF_ENVELOPE" != *$'\n'* ]]; then
			WORKER_BRIEF_JSON=$(printf '%s' "$WORKER_BRIEF_ENVELOPE" | jq -ec '
				select(type == "object" and (keys | sort) == ["brief", "nonce"])
				| .nonce as $nonce
				| select(($nonce | type) == "string" and ($nonce | test("^[0-9a-f]{32}$")))
				| select((.brief | type) == "string" and (.brief | length) > 0)
				| select(([.brief | split("\n")[] | select(. == ("<!-- loom-worker-brief nonce=" + $nonce + " -->"))] | length) == 1)
				| .brief' 2>/dev/null || true)
		fi
		[[ -n "$WORKER_BRIEF_JSON" ]] || loom_debug "spawn-guard: worker-brief invalid envelope"
	fi
fi

if [[ -n "$WORKER_BRIEF_JSON" ]]; then
	UPDATED_INPUT=$(printf '%s' "$UPDATED_INPUT" | jq -c --argjson brief "$WORKER_BRIEF_JSON" \
		'. + {prompt: (.prompt + "\n\n" + $brief)}')
	PROMPT_REWRITTEN=1
fi
if [[ $NEEDS_REWRITE -eq 1 ]]; then
	UPDATED_INPUT=$(printf '%s' "$UPDATED_INPUT" | jq -c --arg model "$MODEL" '. + {model: $model}')
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
