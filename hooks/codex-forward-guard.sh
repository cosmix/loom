#!/usr/bin/env bash
# codex-forward-guard.sh - pin codex forwarding shims to one companion call
#
# A recognized forwarder may make only a direct invocation of Loom's installed
# argv-aware forwarding wrapper. The command is accepted only when its parsed
# argument shape is exact and it contains no unquoted shell operators.
# Missing classification metadata is rejected rather than silently disabling
# the policy.
#
# Input: JSON from stdin - {"tool_name": ..., "tool_input": ...,
#        "agent_type": ..., "transcript_path": ...}
# Exit codes: 0 = allow, 2 = block

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
AGENT_TYPE=$(printf '%s' "$INPUT_JSON" | jq -r '.agent_type // empty' 2>/dev/null || true)
TRANSCRIPT_PATH=$(printf '%s' "$INPUT_JSON" | jq -r '.transcript_path // empty' 2>/dev/null || true)

block_forwarder() {
	local reason="$1"
	loom_debug "DEBUG: BLOCKED codex forwarder tool=$TOOL_NAME reason=$reason"
	cat >&2 <<EOF
⛔ BLOCKED: codex forwarding policy could not authorize this tool call.

Reason: $reason

The forwarding shim may make one direct Bash call of this form:
  ~/.claude/hooks/loom/codex-forward.sh task '<prompt>' --model gpt-5.6-terra --effort xhigh --write

Shell operators, pipelines, redirections, command substitution, and any other
tool are forbidden. If forwarding fails, report the error and stop.
EOF
	exit 2
}

[[ -n "$TOOL_NAME" ]] || block_forwarder "tool_name metadata is missing"

parse_shell_words() {
	local input="$1" state=plain word="" char="" started=0 i
	PARSED_WORDS=()

	for ((i = 0; i < ${#input}; i++)); do
		char=${input:i:1}
		case "$state" in
		plain)
			case "$char" in
			' ')
				if [[ $started -eq 1 ]]; then
					PARSED_WORDS+=("$word")
					word=""
					started=0
				fi
				;;
			"'") state=single; started=1 ;;
			'"') state=double; started=1 ;;
			'\\') state=escape; started=1 ;;
			$'\n' | $'\r' | $'\t' | $'\v' | $'\f' | ';' | '|' | '&' | '<' | '>' | '`' | '$' | '(' | ')' | '#' | '*' | '?' | '[' | ']' | '{' | '}') return 1 ;;
			*) word+="$char"; started=1 ;;
			esac
			;;
		single)
			if [[ "$char" == "'" ]]; then state=plain; else word+="$char"; fi
			;;
		double)
			case "$char" in
			'"') state=plain ;;
			'\\') state=double_escape ;;
			$'\n' | $'\r' | $'\t' | $'\v' | $'\f' | '$' | '`') return 1 ;;
			*) word+="$char" ;;
			esac
			;;
		escape)
			case "$char" in $'\n' | $'\r' | $'\t' | $'\v' | $'\f') return 1 ;; esac
			word+="$char"
			state=plain
			;;
		double_escape)
			case "$char" in
			'"' | '\\') word+="$char"; state=double ;;
			*) return 1 ;;
			esac
			;;
		esac
	done

	[[ "$state" == plain ]] || return 1
	if [[ $started -eq 1 ]]; then PARSED_WORDS+=("$word"); fi
}

is_exact_forward_command() {
	parse_shell_words "$1" || return 1
	[[ ${#PARSED_WORDS[@]} -eq 8 ]] || return 1
	[[ "${PARSED_WORDS[0]}" == "~/.claude/hooks/loom/codex-forward.sh" ]] || return 1
	[[ "${PARSED_WORDS[1]}" == task && -n "${PARSED_WORDS[2]}" ]] || return 1
	[[ "${PARSED_WORDS[3]}" == --model ]] || return 1
	case "${PARSED_WORDS[4]}" in gpt-5.6-terra | gpt-5.6-luna) ;; *) return 1 ;; esac
	[[ "${PARSED_WORDS[5]}" == --effort ]] || return 1
	case "${PARSED_WORDS[6]}" in low | medium | high | xhigh | max | ultra) ;; *) return 1 ;; esac
	[[ "${PARSED_WORDS[7]}" == --write ]]
}

enforce_forwarder() {
	[[ "$TOOL_NAME" == "Bash" ]] || block_forwarder "forwarders may use Bash only"
	local command
	command=$(printf '%s' "$INPUT_JSON" | jq -r '.tool_input.command // empty' 2>/dev/null || true)
	[[ -n "$command" ]] || block_forwarder "Bash command metadata is missing"
	is_exact_forward_command "$command" || block_forwarder "command is not an exact forwarding-wrapper invocation"
	exit 0
}

case "$AGENT_TYPE" in
loom-codex-forwarder | codex:codex-rescue) enforce_forwarder ;;
esac

# A hook payload without either authoritative agent type or transcript metadata
# cannot establish that the caller is not a forwarder.
if [[ -z "$AGENT_TYPE" && -z "$TRANSCRIPT_PATH" ]]; then
	block_forwarder "agent_type and transcript_path metadata are both missing"
fi

# A known non-forwarder type is authoritative and needs no transcript fallback.
if [[ -n "$AGENT_TYPE" ]]; then
	exit 0
fi

# Only subagent transcripts carry the sentinel fallback. Main-session paths do
# not qualify because they also contain the sentinel in Agent tool payloads.
case "$TRANSCRIPT_PATH" in
*/subagents/agent-*.jsonl) ;;
*) exit 0 ;;
esac

[[ -f "$TRANSCRIPT_PATH" && -r "$TRANSCRIPT_PATH" && ! -L "$TRANSCRIPT_PATH" ]] || block_forwarder "subagent transcript metadata is unreadable or unsafe"

if LC_ALL=C dd if="$TRANSCRIPT_PATH" bs=200000 count=1 2>/dev/null | grep -qF 'LOOM-CODEX-FORWARD-ONLY'; then
	enforce_forwarder
fi

exit 0
