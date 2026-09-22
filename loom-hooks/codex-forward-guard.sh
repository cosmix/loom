#!/usr/bin/env bash
# codex-forward-guard.sh - pin codex forwarding shims to one companion call
#
# A recognized forwarder may make only a direct invocation of Loom's installed
# argv-aware forwarding wrapper. The command is accepted only when its parsed
# argument shape is exact and it contains no unquoted shell operators.
# Missing classification metadata is rejected rather than silently disabling
# the policy. Authorized calls are rewritten with guard-minted job identity.
# The whole policy applies only inside a loom stage: with no stage evidence
# (loom_stage_evidence in _codex_forward.sh) every payload is allowed.
#
# Input: JSON from stdin - {"tool_name": ..., "tool_input": ...,
#        "agent_type": ..., "transcript_path": ...}
# Exit codes: 0 = allow, 2 = block (inside a stage, jq not installed also
#        blocks - fail closed)

set -euo pipefail

source "$(dirname "${BASH_SOURCE[0]}")/_codex_forward.sh"
source "$(dirname "${BASH_SOURCE[0]}")/_lifecycle.sh"

# Without the stage probe from _codex_forward.sh this guard cannot decide
# anything, so an unloaded library must refuse rather than allow.
if ! declare -F loom_stage_evidence >/dev/null; then
	printf '%s\n' 'LOOM_HOOK_ERROR: codex-forward-guard.sh could not load its stage-evidence probe, so it cannot authorize this tool call.' >&2
	exit 2
fi

# Outside a loom stage this guard has no policy to enforce: forwarding shims
# exist only inside a stage, and the stock Codex plugin has to stay usable in
# ordinary sessions. Every path that blocks or rewrites a call passes through
# here first, so an ordinary tool call in an ordinary session never pays for
# the probe's process spawns. Out of scope: a nested `claude` started with a
# different config directory loads no hooks at all, so no hook can police it -
# that is the sandbox policy's job.
require_stage_evidence() {
	loom_stage_evidence || exit 0
}

# jq is what classifies the payload, so a missing jq still fails closed - but
# only where there is a policy to fail closed about. The probe runs solely in
# that already-blocking case; with jq present this costs one builtin lookup.
command -v jq &>/dev/null || require_stage_evidence
loom_require_jq "codex-forward-guard.sh"

if command -v gtimeout &>/dev/null; then
	INPUT_JSON=$(gtimeout 1 cat 2>/dev/null || true)
elif command -v timeout &>/dev/null; then
	INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)
else
	INPUT_JSON=$(cat 2>/dev/null || true)
fi

TOOL_NAME=$(printf '%s' "$INPUT_JSON" | jq -r '.tool_name // empty' 2>/dev/null || true)
AGENT_TYPE=$(printf '%s' "$INPUT_JSON" | jq -r '.agent_type // empty' 2>/dev/null || true)
TRANSCRIPT_PATH=$(printf '%s' "$INPUT_JSON" | jq -r '.transcript_path // empty' 2>/dev/null || true)
TOOL_USE_ID=$(printf '%s' "$INPUT_JSON" | jq -r '.tool_use_id? | strings' 2>/dev/null || true)
AGENT_ID=$(printf '%s' "$INPUT_JSON" | jq -r '.agent_id? | strings' 2>/dev/null || true)
PARENT_SESSION_ID=$(printf '%s' "$INPUT_JSON" | jq -r '.session_id? | strings' 2>/dev/null || true)
PAYLOAD_CWD=$(printf '%s' "$INPUT_JSON" | jq -r '.cwd? | strings' 2>/dev/null || true)
TOOL_INPUT=$(printf '%s' "$INPUT_JSON" | jq -c '.tool_input | select(type == "object")' 2>/dev/null || true)

block_forwarder() {
	local reason="$1" evidence_note=""
	require_stage_evidence
	# Name the signal when it is not the hook's own environment, so the
	# operator of a false positive can see what classified the session.
	case "$LOOM_STAGE_EVIDENCE" in
	"env "*) ;;
	*) evidence_note="
This session was classified as a loom stage by: $LOOM_STAGE_EVIDENCE" ;;
	esac
	loom_debug "DEBUG: BLOCKED codex forwarder tool=$TOOL_NAME reason=$reason evidence=$LOOM_STAGE_EVIDENCE"
	cat >&2 <<EOF
⛔ BLOCKED: codex forwarding policy could not authorize this tool call.

Reason: $reason$evidence_note

The forwarding shim may make one direct Bash call of this form:
  ~/.claude/hooks/loom/codex-forward.sh task '<prompt>' --model gpt-5.6-terra --effort xhigh --write [--unit-id <unit>]
The wrapper path may instead be written out in full as
  $HOME/.claude/hooks/loom/codex-forward.sh
Write that path expanded, exactly as shown - a literal \$HOME is rejected,
because an unquoted \$ is a forbidden shell metacharacter.

Shell operators, pipelines, redirections, command substitution, and any other
tool are forbidden. If forwarding fails, report the error and stop.
EOF
	exit 2
}

[[ -n "$TOOL_NAME" ]] || block_forwarder "tool_name metadata is missing"

valid_unit_id() {
	local value="$1"
	[[ ${#value} -ge 1 && ${#value} -le 64 && "$value" =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$ ]]
}

valid_identity_id() {
	local value="$1"
	[[ ${#value} -ge 1 && ${#value} -le 128 && "$value" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]]
}

is_exact_forward_command() {
	local allow_invocation="${2:-0}" count
	parse_shell_words "$1" || return 1
	count=${#PARSED_WORDS[@]}
	if [[ $count -ne 8 && $count -ne 10 ]]; then
		[[ "$allow_invocation" == 1 && $count -eq 12 ]] || return 1
	fi
	if [[ -n "${HOME:-}" ]]; then
		[[ "${PARSED_WORDS[0]}" == "~/.claude/hooks/loom/codex-forward.sh" || "${PARSED_WORDS[0]}" == "${HOME}/.claude/hooks/loom/codex-forward.sh" ]] || return 1
	else
		[[ "${PARSED_WORDS[0]}" == "~/.claude/hooks/loom/codex-forward.sh" ]] || return 1
	fi
	[[ "${PARSED_WORDS[1]}" == task && -n "${PARSED_WORDS[2]}" ]] || return 1
	[[ "${PARSED_WORDS[3]}" == --model ]] || return 1
	case "${PARSED_WORDS[4]}" in gpt-6-astra | gpt-6-sol | gpt-5.6-terra | gpt-6-luna) ;; *) return 1 ;; esac
	[[ "${PARSED_WORDS[5]}" == --effort ]] || return 1
	case "${PARSED_WORDS[6]}" in low | medium | high | xhigh | max | ultra) ;; *) return 1 ;; esac
	[[ "${PARSED_WORDS[7]}" == --write ]] || return 1
	if [[ $count -ge 10 ]]; then
		[[ "${PARSED_WORDS[8]}" == --unit-id ]] || return 1
		valid_unit_id "${PARSED_WORDS[9]}" || return 1
	fi
	if [[ $count -eq 12 ]]; then
		[[ "${PARSED_WORDS[10]}" == --invocation-id ]] || return 1
		[[ "${PARSED_WORDS[11]}" =~ ^inv-[0-9a-f]{32}$ ]] || return 1
	fi
}

caller_supplied_invocation() {
	parse_shell_words "$1" || return 1
	local i
	for ((i = 8; i < ${#PARSED_WORDS[@]}; i++)); do
		[[ "${PARSED_WORDS[i]}" == --invocation-id ]] && return 0
	done
	return 1
}

# has_prior_forwarding_call - Find an earlier exact Bash forwarding tool use
# in this forwarder's transcript. A missing or unreadable transcript is not
# evidence, so the caller retains the existing authorization behavior.
has_prior_forwarding_call() {
	local candidate_id candidate_command
	[[ -n "$TRANSCRIPT_PATH" && -f "$TRANSCRIPT_PATH" && -r "$TRANSCRIPT_PATH" && ! -L "$TRANSCRIPT_PATH" ]] || return 1

	while IFS= read -r -d '' candidate_id && IFS= read -r -d '' candidate_command; do
		if [[ -n "$TOOL_USE_ID" ]] && { [[ -z "$candidate_id" ]] || [[ "$candidate_id" == "$TOOL_USE_ID" ]]; }; then
			continue
		fi
		is_exact_forward_command "$candidate_command" 1 && return 0
	done < <(
		LC_ALL=C head -c 4194304 "$TRANSCRIPT_PATH" 2>/dev/null |
			jq -jR '
				fromjson? |
				select(.type? == "assistant" or .message.role? == "assistant") |
				(.message.content? // [])[]? |
				select(.type? == "tool_use" and .name? == "Bash" and (.input.command? | type == "string")) |
				(((.id? // "") | if type == "string" then . else "" end) + "\u0000"),
				(.input.command + "\u0000")
			' 2>/dev/null || true
	)
	return 1
}

canonical_dir_target() {
	local target="$1" suffix="" leaf parent physical
	while [[ ! -d "$target" ]]; do
		[[ ! -e "$target" && ! -L "$target" ]] || return 1
		leaf=${target##*/}
		parent=${target%/*}
		[[ -n "$leaf" && -n "$parent" && "$parent" != "$target" ]] || return 1
		suffix="/$leaf$suffix"
		target=$parent
	done
	physical=$(cd "$target" 2>/dev/null && pwd -P) || return 1
	printf '%s%s\n' "$physical" "$suffix"
}

resolve_active_stage() {
	local work_dir="${LOOM_WORK_DIR:-}"
	valid_identity_id "${LOOM_STAGE_ID:-}" || return 1
	valid_identity_id "${LOOM_SESSION_ID:-}" || return 1
	[[ "$work_dir" == /* && "$work_dir" != *$'\n'* && -d "$work_dir" ]] || return 1
	ACTIVE_WORK_DIR=$(cd "$work_dir" 2>/dev/null && pwd -P) || return 1
	[[ -n "$ACTIVE_WORK_DIR" && "$ACTIVE_WORK_DIR" != / ]]
}

select_companion() {
	[[ -n "${HOME:-}" && -d "$HOME" ]] || return 1
	local selected="${HOME}/.claude/plugins/cache/openai-codex/codex/1.0.6/scripts/codex-companion.mjs"
	[[ -f "$selected" && ! -L "$selected" ]] || return 1
	local directory
	directory=$(cd "$(dirname "$selected")" 2>/dev/null && pwd -P) || return 1
	COMPANION_PATH="$directory/codex-companion.mjs"
	COMPANION_VERSION=1.0.6
	STATE_ROOT=$(canonical_dir_target "${HOME}/.codex/plugin-data/state") || return 1
}

resolve_workspace_root() {
	local cwd="${PAYLOAD_CWD:-$PWD}" candidate=""
	[[ -d "$cwd" ]] || return 1
	if command -v git >/dev/null 2>&1; then
		candidate=$(git -C "$cwd" rev-parse --show-toplevel 2>/dev/null || true)
	fi
	[[ -n "$candidate" && -d "$candidate" ]] || candidate=$cwd
	WORKSPACE_ROOT=$(cd "$candidate" 2>/dev/null && pwd -P) || return 1
}

resolve_forwarder_agent() {
	FORWARDER_AGENT_ID=$AGENT_ID
	if [[ -z "$FORWARDER_AGENT_ID" ]]; then
		case "$TRANSCRIPT_PATH" in
		*/subagents/agent-*.jsonl)
			FORWARDER_AGENT_ID=${TRANSCRIPT_PATH##*/agent-}
			FORWARDER_AGENT_ID=${FORWARDER_AGENT_ID%.jsonl}
			;;
		esac
	fi
	[[ -n "$FORWARDER_AGENT_ID" ]]
}

forward_observed_at() {
	local observed transcript_observed observed_epoch transcript_epoch
	observed=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z" 2>/dev/null) || return 1
	observed_epoch=$(loom_lifecycle_epoch "$observed") || return 1
	if [[ -n "$TRANSCRIPT_PATH" && -f "$TRANSCRIPT_PATH" && -r "$TRANSCRIPT_PATH" &&
		! -L "$TRANSCRIPT_PATH" ]]; then
		transcript_observed=$(LC_ALL=C head -c 4194304 "$TRANSCRIPT_PATH" 2>/dev/null |
			jq -sr '[.[] | .timestamp? | select(type == "string")] | last // empty' \
			2>/dev/null || true)
		transcript_epoch=$(loom_lifecycle_epoch "$transcript_observed" 2>/dev/null || true)
		if [[ "$transcript_epoch" =~ ^[0-9]+$ ]] && ((transcript_epoch > observed_epoch)); then
			observed=$transcript_observed
		fi
	fi
	printf '%s\n' "$observed"
}

resolve_forwarder_start() {
	local expected_type=loom-codex-forwarder observed ledger row candidate status
	local matches=0
	[[ "$AGENT_TYPE" != codex:codex-rescue ]] || expected_type=codex:codex-rescue
	observed=$(forward_observed_at) || return 1
	loom_lifecycle_resolve_start "$ACTIVE_WORK_DIR" "${LOOM_STAGE_ID:-}" \
		"$PARENT_SESSION_ID" "${LOOM_SESSION_ID:-}" "$FORWARDER_AGENT_ID" \
		"$expected_type" "$observed" "codex-forward-guard.sh" || return
	ledger="$ACTIVE_WORK_DIR/subagents/${LOOM_STAGE_ID:-}/starts.jsonl"
	loom_lifecycle_plain_path "$ledger" file || return 1
	while IFS= read -r row || [[ -n "$row" ]]; do
		[[ -n "${row//[[:space:]]/}" ]] || continue
		candidate=$(loom_lifecycle_start_candidate "$row" "${LOOM_STAGE_ID:-}" \
			"$PARENT_SESSION_ID" "${LOOM_SESSION_ID:-}" "$FORWARDER_AGENT_ID" \
			"codex-forward-guard.sh"); status=$?
		((status == 0)) || return "$status"
		[[ -z "$candidate" ]] || matches=$((matches + 1))
	done <"$ledger"
	[[ $matches -eq 1 ]]
}

mint_invocation() {
	local nonce
	nonce=$(LC_ALL=C od -An -N16 -tx1 /dev/urandom 2>/dev/null | tr -d ' \n') || return 1
	[[ "$nonce" =~ ^[0-9a-f]{32}$ ]] || return 1
	INVOCATION_ID="inv-$nonce"
}

# Append the authorization before allowing execution. Unlike the legacy model
# ledger, failure is an authorization failure: the job must never launch.
record_codex_task() {
	local model="$1" effort="$2" unit="$3" invocation="$4"
	local work_dir="${ACTIVE_WORK_DIR:-}" stage_id="${LOOM_STAGE_ID:-}"
	local loom_session_id="${LOOM_SESSION_ID:-}" dir file ts line
	[[ -n "$work_dir" && -n "$stage_id" && -n "$loom_session_id" ]] || return 1
	valid_identity_id "$stage_id" || return 1
	valid_identity_id "$loom_session_id" || return 1
	valid_identity_id "$PARENT_SESSION_ID" || return 1
	valid_identity_id "$FORWARDER_AGENT_ID" || return 1
	valid_identity_id "$TOOL_USE_ID" || return 1

	dir="${work_dir}/subagents/${stage_id}"
	mkdir -p -m 700 "$dir" 2>/dev/null || return 1
	chmod 700 "$dir" 2>/dev/null || return 1
	file="${dir}/codex.jsonl"
	[[ ! -L "$file" ]] || return 1
	[[ ! -e "$file" || -f "$file" ]] || return 1
	ts=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z") || return 1
	line=$(jq -nc --arg ts "$ts" --arg stage_id "$stage_id" \
		--arg session_id "$loom_session_id" --arg parent_session_id "$PARENT_SESSION_ID" \
		--arg forwarder_agent_id "$FORWARDER_AGENT_ID" --arg tool_use_id "$TOOL_USE_ID" \
		--arg unit_id "$unit" --arg invocation_id "$invocation" --arg model "$model" \
		--arg effort "$effort" --arg workspace_root "$WORKSPACE_ROOT" \
		--arg companion_version "$COMPANION_VERSION" --arg companion_path "$COMPANION_PATH" \
		--arg state_root "$STATE_ROOT" \
		'{v:2,ts:$ts,stage_id:$stage_id,session_id:$session_id,parent_session_id:$parent_session_id,forwarder_agent_id:$forwarder_agent_id,tool_use_id:$tool_use_id,unit_id:$unit_id,invocation_id:$invocation_id,model:$model,effort:$effort,workspace_root:$workspace_root,companion_version:$companion_version,companion_path:$companion_path,state_root:$state_root}' \
		2>/dev/null) || return 1
	[[ -n "$line" ]] || return 1
	{ printf '%s\n' "$line" >>"$file"; } 2>/dev/null || return 1
	chmod 600 "$file" 2>/dev/null || true
}

emit_authorized_input() {
	local command="$1" unit="$2" invocation="$3" has_unit="$4" updated
	updated=$command
	[[ "$has_unit" == 1 ]] || updated+=" --unit-id $unit"
	updated+=" --invocation-id $invocation"
	jq -nc --argjson input "$TOOL_INPUT" --arg command "$updated" \
		'{hookSpecificOutput:{hookEventName:"PreToolUse",permissionDecision:"allow",updatedInput:($input + {command:$command})}}'
}

enforce_forwarder() {
	require_stage_evidence
	[[ "$TOOL_NAME" == "Bash" ]] || block_forwarder "forwarders may use Bash only"
	local command
	command=$(printf '%s' "$INPUT_JSON" | jq -r '.tool_input.command // empty' 2>/dev/null || true)
	[[ -n "$command" && -n "$TOOL_INPUT" ]] || block_forwarder "Bash command metadata is missing"
	caller_supplied_invocation "$command" && block_forwarder "caller-supplied --invocation-id is forbidden"
	is_exact_forward_command "$command" || block_forwarder "command is not an exact forwarding-wrapper invocation"
	resolve_active_stage || block_forwarder "codex forwarding is allowed only inside an active loom stage (safe LOOM_STAGE_ID, LOOM_SESSION_ID, and LOOM_WORK_DIR are required)"
	local model="${PARSED_WORDS[4]}" effort="${PARSED_WORDS[6]}" unit="" has_unit=0
	if [[ ${#PARSED_WORDS[@]} -eq 10 ]]; then
		unit=${PARSED_WORDS[9]}
		has_unit=1
	fi
	resolve_forwarder_agent || block_forwarder "forwarder agent identity is missing"
	resolve_forwarder_start || block_forwarder "forwarder identity does not match exactly one SubagentStart row"
	[[ -n "$unit" ]] || unit="fwd-$FORWARDER_AGENT_ID"
	valid_unit_id "$unit" || block_forwarder "unit id is invalid"
	has_prior_forwarding_call && block_forwarder "one forward per forwarder: the first forward is already running or finished. Return its output as the final message and stop; never retry or re-forward."
	select_companion || block_forwarder "supported codex companion 1.0.6 is missing or unsafe"
	resolve_workspace_root || block_forwarder "payload cwd cannot be resolved to a canonical workspace"
	mint_invocation || block_forwarder "could not mint an invocation id"
	record_codex_task "$model" "$effort" "$unit" "$INVOCATION_ID" || block_forwarder "authorization row could not be written"
	emit_authorized_input "$command" "$unit" "$INVOCATION_ID" "$has_unit"
	exit 0
}

# A hook payload without either authoritative agent type or transcript metadata
# cannot establish that the caller is not a forwarder. This fail-closed check
# stays local rather than folding into loom_codex_forwarder_verdict below:
# that shared verdict (_common.sh) treats an empty agent_type together with an
# empty transcript_path as "not a forwarder" (rc=1), which would silently
# ALLOW the call here instead of blocking it.
if [[ -z "$AGENT_TYPE" && -z "$TRANSCRIPT_PATH" ]]; then
	block_forwarder "agent_type and transcript_path metadata are both missing"
fi

# loom_codex_forwarder_verdict (_common.sh) applies codex-forward-guard.sh's
# own classification rule: an authoritative agent_type match wins outright
# (rc=0, regardless of transcript_path); otherwise a non-empty non-matching
# agent_type is a known non-forwarder (rc=1); otherwise only a SUBAGENT
# transcript path carries the LOOM-CODEX-FORWARD-ONLY sentinel fallback
# (rc=0 found, rc=1 not found or shape mismatch, rc=2 unreadable/symlinked).
verdict_rc=0
loom_codex_forwarder_verdict "$AGENT_TYPE" "$TRANSCRIPT_PATH" || verdict_rc=$?
case "$verdict_rc" in
0) enforce_forwarder ;;
2) block_forwarder "subagent transcript metadata is unreadable or unsafe" ;;
esac

exit 0
