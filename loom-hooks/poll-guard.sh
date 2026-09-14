#!/usr/bin/env bash
# PreToolUse:Bash hook for long sleeps, repeated polls, Bash-side reads, and pathless `git show`/`git diff`.
# Warnings collect; only a configured live stage can deny via loom_hook_deny_or_warn in _read_discipline.sh.

set -euo pipefail

source "$(dirname "$0")/_common.sh"
source "$(dirname "$0")/_progress-classification.sh"
source "$(dirname "$0")/_read_discipline.sh"
loom_warn_no_jq "poll-guard.sh"

_loom_sleep_argument() {
	local j base arg
	while IFS= read -r j; do
		base="${LOOM_TOKENS[$j]##*/}"
		if [[ "$base" == "sleep" ]]; then
			arg="${LOOM_TOKENS[$((j + 1))]:-}"
			[[ -n "$arg" && "$arg" != "%%SEP%%" ]] && printf '%s' "$arg" && return 0
		fi
	done < <(_loom_poll_command_indices)
	return 1
}

_loom_poll_rule_sleep() {
	local arg num unit="" mult=1
	arg=$(_loom_sleep_argument) || return 0
	num="$arg"
	case "$arg" in
	*[smhd])
		unit="${arg: -1}"
		num="${arg%?}"
		;;
	esac
	[[ "$num" =~ ^[0-9]+(\.[0-9]+)?$ ]] || return 0
	case "$unit" in
	m) mult=60 ;;
	h) mult=3600 ;;
	d) mult=86400 ;;
	esac
	awk -v n="$num" -v m="$mult" 'BEGIN { exit !(n * m >= 30) }' || return 0
	loom_hook_note_warn "\`sleep ${arg}\` burns a turn doing nothing. Wait on the real signal instead: one background \`loom subagents watch --worker <kind>:<id> ... --timeout 3600\`, never re-armed."
	return 0
}

# Echo one active valid receipt ID; missing or malformed ledger data falls open.
_loom_poll_active_receipt() {
	local file body rows id
	local -a active=()
	[[ -n "${LOOM_WORK_DIR:-}" && -n "${LOOM_STAGE_ID:-}" && -n "${LOOM_SESSION_ID:-}" ]] || return 1
	file="${LOOM_WORK_DIR}/subagents/${LOOM_STAGE_ID}/forward-receipts.jsonl"
	[[ -f "$file" ]] || return 1
	body=$(tail -c 262144 "$file" 2>/dev/null) || return 1
	[[ -n "$body" ]] || return 1
	[[ "$body" == \{* ]] || body="${body#*$'\n'}"
	[[ -n "$body" ]] || return 1
	rows=$(printf '%s\n' "$body" | jq -rs --arg session "$LOOM_SESSION_ID" '
		map(select(.loom_session_id == $session) |
			if ((.receipt_id | type) == "string" and (.state == "queued" or .state == "running" or .state == "succeeded" or .state == "failed" or .state == "canceled"))
			then {id: .receipt_id, state: .state} else error("malformed forward receipt") end) as $receipts |
		[$receipts[].id] | unique | .[] as $id |
		select([$receipts[] | select(.id == $id and (.state == "succeeded" or .state == "failed" or .state == "canceled"))] | length == 0) | $id
	' 2>/dev/null) || return 1
	while IFS= read -r id; do [[ -n "$id" ]] && active+=("$id"); done <<<"$rows"
	[[ ${#active[@]} -eq 1 && "${active[0]}" =~ ^[0-9a-f]{64}$ ]] || return 1
	printf '%s' "${active[0]}"
}

# Diagnostics warn on the third/fourth repeat and deny-or-warn from the fifth;
# an owned watch deny-or-warns as soon as the same normalized wait is repeated.
_loom_poll_rule_repeat() {
	local stripped="$1" agent_id="$2" fallback_sid="$3" op="" key
	if op=$(_loom_progress_subagents_pipeline_op "$stripped"); then
		if [[ "$op" == watch ]] && ! _loom_progress_subagents_watch_is_owned; then return 0; fi
		key=$(_loom_progress_subagents_key "$stripped") || return 0
	else
		_loom_poll_is_countable "$stripped" || return 0
		key=$(printf '%s' "$stripped" | tr -s '[:space:]' ' ')
		key="${key# }"
		key="${key% }"
	fi
	local ledger occ
	ledger=$(_loom_ledger_file "polls" "$agent_id" "$fallback_sid")
	occ=$(($(_loom_polls_count "$ledger" "$key") + 1))
	if [[ "$op" == watch ]]; then
		if ((occ >= 2)); then
			loom_hook_deny_or_warn "the first watch already owns this wait; a second one only returns AlreadyWaiting (exit 4) without another monitor; wait for the first watch notification, then harvest each terminal report once."
		fi
		_loom_ledger_append "$ledger" "$key"
		return 0
	fi
	if ((occ >= 3)); then
		local guidance="" receipt
		if [[ "$op" == list ]]; then
			if receipt=$(_loom_poll_active_receipt); then
				guidance="act on what you know: use \`loom subagents wait --receipt ${receipt} --timeout 3600\`."
			else
				guidance="forward state is unresolved; repeated list polling will not resolve it. Run one background call: \`loom subagents watch --worker <kind>:<id> ... --timeout 3600\`; never re-arm it. The unowned \`loom subagents watch --timeout 3600\` lacks the required worker identity; use explicit exact-id recovery if identities are unavailable, and never retry or cancel."
			fi
		fi
		if ((occ >= 5)); then
			loom_hook_deny_or_warn "\`${key}\` has run ${occ} times this session - stop polling and act on what you already know instead of checking again${guidance:+: ${guidance}}"
		else
			loom_hook_note_warn "\`${key}\` has run ${occ} times this session - act on what you already know instead of checking again${guidance:+: ${guidance}}"
		fi
	fi
	_loom_ledger_append "$ledger" "$key"
	return 0
}

# Byte counts and follow flags skip line discipline: they are bounded or endless.
_loom_read_bound_for_head_tail() {
	local base="$1" tok k=""
	shift
	while [[ $# -gt 0 ]]; do
		tok="$1"
		shift
		case "$tok" in
		-c | -c[0-9]* | --bytes | --bytes=* | -f | --follow | --follow=*)
			printf 'skip '
			return 0
			;;
		-n)
			k="${1:-}"
			shift || true
			;;
		-n[0-9]*) k="${tok#-n}" ;;
		-[0-9]*) k="${tok#-}" ;;
		esac
	done
	if [[ -n "$k" ]]; then
		[[ "$base" == "head" ]] && printf 'range 1-%s' "$k" || printf 'range -%s' "$k"
	else
		printf 'full '
	fi
}

# A sed range ending in `$` is full; other non-range forms can rewrite, so skip.
_loom_read_bound_for_sed() {
	local tok range=""
	for tok in "$@"; do
		case "$tok" in
		[0-9]*,\$p)
			printf 'full '
			return 0
			;;
		[0-9]*,[0-9]*p)
			range="${tok%p}"
			range="${range/,/-}"
			;;
		esac
	done
	[[ -n "$range" ]] && printf 'range %s' "$range" || printf 'skip '
}

_loom_bash_read_check() {
	local cmd_idx="$1" base="$2" agent_id="$3" fallback_sid="$4"
	local n=${#LOOM_TOKENS[@]} i=$((cmd_idx + 1))
	local -a args=()
	while ((i < n)) && [[ "${LOOM_TOKENS[$i]}" != "%%SEP%%" ]]; do
		args+=("${LOOM_TOKENS[$i]}")
		i=$((i + 1))
	done

	local kind="full" lines="" bound
	if ((${#args[@]} > 0)); then
		case "$base" in
		head | tail)
			bound=$(_loom_read_bound_for_head_tail "$base" "${args[@]}")
			kind="${bound%% *}"
			lines="${bound#* }"
			[[ "$kind" == "skip" ]] && return 0
			;;
		sed)
			bound=$(_loom_read_bound_for_sed "${args[@]}")
			kind="${bound%% *}"
			lines="${bound#* }"
			[[ "$kind" == "skip" ]] && return 0
			;;
		esac
	elif [[ "$base" == "sed" ]]; then
		return 0 # `sed` with no arguments reads nothing
	fi

	# Full-read line counts are path-specific (`cat a b` names two files).
	local tok read_lines
	if ((${#args[@]} > 0)); then
		for tok in "${args[@]}"; do
			case "$tok" in -*) continue ;; esac
			[[ -f "$tok" ]] || continue
			read_lines="$lines"
			[[ "$kind" == "full" ]] && read_lines=$(_loom_read_full_lines "$tok")
			loom_read_discipline_check "$tok" "$kind" "$read_lines" "$agent_id" "$fallback_sid"
		done
	fi
	return 0
}

# Check every cat/head/tail/sed segment.
loom_bash_reads_scan() {
	local agent_id="$1" fallback_sid="$2"
	local j base
	while IFS= read -r j; do
		base="${LOOM_TOKENS[$j]##*/}"
		case "$base" in
		cat | head | tail | sed) _loom_bash_read_check "$j" "$base" "$agent_id" "$fallback_sid" ;;
		esac
	done < <(_loom_poll_command_indices)
	return 0
}

# A path exists, contains `/` or `:`, but a revision range never counts as one.
_loom_git_arg_names_path() {
	local tok="$1"
	case "$tok" in
	-*) return 1 ;;
	*..*) return 1 ;;
	esac
	[[ -e "$tok" ]] && return 0
	case "$tok" in
	*/* | *:*) return 0 ;;
	esac
	return 1
}

_loom_git_segment_is_pathless_show_diff() {
	local j="$1" n=${#LOOM_TOKENS[@]}
	local sub="${LOOM_TOKENS[$((j + 1))]:-}"
	case "$sub" in
	show | diff) ;;
	*) return 1 ;;
	esac
	local i=$((j + 2)) tok
	while ((i < n)) && [[ "${LOOM_TOKENS[$i]}" != "%%SEP%%" ]]; do
		tok="${LOOM_TOKENS[$i]}"
		case "$tok" in
		-- | --stat | --name-only | --name-status) return 1 ;;
		esac
		_loom_git_arg_names_path "$tok" && return 1
		i=$((i + 1))
	done
	return 0
}

_loom_git_segment_text() {
	local j="$1" n=${#LOOM_TOKENS[@]} i="$1" out=""
	while ((i < n)) && [[ "${LOOM_TOKENS[$i]}" != "%%SEP%%" ]]; do
		[[ -z "$out" ]] && out="${LOOM_TOKENS[$i]}" || out="${out} ${LOOM_TOKENS[$i]}"
		i=$((i + 1))
	done
	printf '%s' "$out"
}

# Warn once for the first pathless show/diff, avoiding duplicate chain warnings.
_loom_poll_rule_git_pathless() {
	local j base
	while IFS= read -r j; do
		base="${LOOM_TOKENS[$j]##*/}"
		if [[ "$base" == "git" ]] && _loom_git_segment_is_pathless_show_diff "$j"; then
			loom_hook_note_warn "\`$(_loom_git_segment_text "$j")\` ran with no path - run --stat first, then per-file -- <path>"
			return 0
		fi
	done < <(_loom_poll_command_indices)
	return 0
}

# --- Main ----------------------------------------------------------------

if command -v gtimeout &>/dev/null; then
	INPUT_JSON=$(gtimeout 1 cat 2>/dev/null || true)
elif command -v timeout &>/dev/null; then
	INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)
else
	INPUT_JSON=$(cat 2>/dev/null || true)
fi

TOOL_NAME=$(echo "$INPUT_JSON" | jq -r '.tool_name // empty' 2>/dev/null || true)
if [[ "$TOOL_NAME" != "Bash" ]]; then
	exit 0
fi

COMMAND=$(echo "$INPUT_JSON" | jq -r '.tool_input.command // empty' 2>/dev/null || true)
if [[ -z "$COMMAND" ]]; then
	exit 0
fi

RAW_AGENT_ID=$(echo "$INPUT_JSON" | jq -r '.agent_id // empty' 2>/dev/null || true)
PAYLOAD_SID=$(echo "$INPUT_JSON" | jq -r '.session_id // empty' 2>/dev/null || true)
AGENT_ID=$(_loom_sanitize_agent_id "$RAW_AGENT_ID")

STRIPPED=$(strip_embedded_content "$COMMAND")
if [[ -z "$STRIPPED" ]]; then
	exit 0
fi

# Unterminated quotes leave LOOM_TOKENS untrustworthy, so evaluate no rule.
if loom_tokenize_command "$STRIPPED"; then
	_loom_poll_rule_sleep
	_loom_poll_rule_repeat "$STRIPPED" "$AGENT_ID" "${PAYLOAD_SID:-unknown}"
	loom_bash_reads_scan "$AGENT_ID" "${PAYLOAD_SID:-unknown}"
	_loom_poll_rule_git_pathless
fi

loom_hook_emit_warns
exit 0
