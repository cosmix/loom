#!/usr/bin/env bash
# PreToolUse:Bash hook for long sleeps, repeated polls, Bash-side reads, and pathless `git show`/`git diff`.
# Warnings collect; only a configured live stage can deny via loom_hook_deny_or_warn in _read_discipline.sh.

set -euo pipefail

source "$(dirname "$0")/_common.sh"
source "$(dirname "$0")/_read_discipline.sh"
loom_warn_no_jq "poll-guard.sh"

# _loom_poll_command_indices - echo each command segment's effective command
# word index. All rule scans use this so wrapper-unwrapping stays identical.
_loom_poll_command_indices() {
	local n=${#LOOM_TOKENS[@]} i=0 j
	while ((i < n)); do
		j=$(loom_tokens_command_word_index "$i") && printf '%s\n' "$j"
		while ((i < n)) && [[ "${LOOM_TOKENS[$i]}" != "%%SEP%%" ]]; do i=$((i + 1)); done
		i=$((i + 1))
	done
}

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
	loom_hook_note_warn "\`sleep ${arg}\` burns a turn doing nothing. Wait on the real signal instead: one backgrounded \`loom subagents watch --timeout 3600\`."
	return 0
}

# `git status` always counts; `git log` only without a path argument.
_loom_poll_git_is_countable() {
	local j="$1"
	local sub="${LOOM_TOKENS[$((j + 1))]:-}"
	case "$sub" in
	status) return 0 ;;
	log)
		local i=$((j + 2)) tok
		while ((i < ${#LOOM_TOKENS[@]})) && [[ "${LOOM_TOKENS[$i]}" != "%%SEP%%" ]]; do
			tok="${LOOM_TOKENS[$i]}"
			case "$tok" in -*) : ;; *) return 1 ;; esac
			i=$((i + 1))
		done
		return 0
		;;
	esac
	return 1
}

# `cat` counts only for legacy `.work` or current `.loom/work` paths.
_loom_poll_cat_is_countable() {
	local j="$1"
	local arg="${LOOM_TOKENS[$((j + 1))]:-}"
	[[ -z "$arg" || "$arg" == "%%SEP%%" ]] && return 1
	case "$arg" in
	*.work/* | */.work | *.loom/work/* | */.loom/work) return 0 ;;
	esac
	return 1
}

_loom_poll_pipeline_only() {
	local raw="$1" i=0 n=${#1} quote="" ch next
	[[ "$raw" == *$'\n'* ]] && return 1
	while ((i < n)); do
		ch="${raw:$i:1}"
		if [[ "$quote" == single ]]; then [[ "$ch" == "'" ]] && quote=""
		elif [[ "$quote" == double ]]; then
			[[ "$ch" == $'\\' ]] && { i=$((i + 1)); } || { [[ "$ch" == '"' ]] && quote=""; }
		else
			[[ "$ch" == $'\\' ]] && { i=$((i + 1)); i=$((i + 1)); continue; }
			[[ "$ch" == "'" ]] && { quote=single; i=$((i + 1)); continue; }
			[[ "$ch" == '"' ]] && { quote=double; i=$((i + 1)); continue; }
			next="${raw:$((i + 1)):1}"
			case "$ch" in '&' | ';' | '<' | '>') return 1 ;; '|') [[ "$next" == '|' ]] && return 1 ;; esac
		fi
		i=$((i + 1))
	done
	[[ -z "$quote" ]]
}

# Only `loom subagents list` with declared flags and one display pipe counts.
_loom_poll_list_is_countable() {
	local raw="$1" j i n=${#LOOM_TOKENS[@]} tok base k
	_loom_poll_pipeline_only "$raw" || return 1
	j=$(loom_tokens_command_word_index 0) || return 1
	[[ "${LOOM_TOKENS[$j]##*/}" == loom && "${LOOM_TOKENS[$((j + 1))]:-}" == subagents && "${LOOM_TOKENS[$((j + 2))]:-}" == list ]] || return 1
	i=$((j + 3))
	while ((i < n)) && [[ "${LOOM_TOKENS[$i]}" != "%%SEP%%" ]]; do
		tok="${LOOM_TOKENS[$i]}"
		case "$tok" in
		--json | -h | --help | --session=* | --dir=* | --debounce=*) ;;
		--session | --dir | --debounce) i=$((i + 1)); [[ $i -lt $n && "${LOOM_TOKENS[$i]}" != "%%SEP%%" ]] || return 1 ;;
		*) return 1 ;;
		esac
		i=$((i + 1))
	done
	((i == n)) && return 0
	i=$((i + 1)); j=$(loom_tokens_command_word_index "$i") || return 1
	((j == i)) || return 1
	base="${LOOM_TOKENS[$j]##*/}"; k=$((j + 1))
	case "$base" in
	head | tail)
		[[ "${LOOM_TOKENS[$k]:-}" == -n ]] || return 1
		k=$((k + 1))
		[[ "${LOOM_TOKENS[$k]:-}" =~ ^[0-9]+$ && $((k + 1)) -eq $n ]] ;;
	rg) [[ $((k + 1)) -eq $n && "${LOOM_TOKENS[$k]:-}" != "%%SEP%%" ]] ;;
	*) return 1 ;;
	esac
}

_loom_poll_is_countable() {
	local raw="$1" tok j base
	_loom_poll_pipeline_only "$raw" || return 1
	for tok in "${LOOM_TOKENS[@]}"; do [[ "$tok" == "%%SEP%%" ]] && return 1; done
	_loom_is_verify_runner_command && return 1
	j=$(loom_tokens_command_word_index 0) || return 1
	base="${LOOM_TOKENS[$j]##*/}"
	case "$base" in
	ls | wc | test | '[' | stat | sleep | date | pwd) return 0 ;;
	git) _loom_poll_git_is_countable "$j" ;;
	cat) _loom_poll_cat_is_countable "$j" ;;
	*) return 1 ;;
	esac
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

# Warn at the third/fourth identical poll and deny-or-warn from the fifth.
_loom_poll_rule_repeat() {
	local stripped="$1" agent_id="$2" fallback_sid="$3" is_list=0
	if _loom_poll_list_is_countable "$stripped"; then is_list=1; else _loom_poll_is_countable "$stripped" || return 0; fi
	local key
	key=$(printf '%s' "$stripped" | tr -s '[:space:]' ' ')
	key="${key# }"
	key="${key% }"
	local ledger occ
	ledger=$(_loom_ledger_file "polls" "$agent_id" "$fallback_sid")
	occ=$(($(_loom_polls_count "$ledger" "$key") + 1))
	if ((occ >= 3)); then
		local guidance="" receipt
		if [[ $is_list -eq 1 ]]; then
			if receipt=$(_loom_poll_active_receipt); then
				guidance="act on what you know: use \`loom subagents wait --receipt ${receipt} --timeout 3600\`."
			else
				guidance="forward state is unresolved; repeated list polling will not resolve it. Use one background \`loom subagents watch --timeout 3600\` or explicit exact-id recovery; never retry or cancel."
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
