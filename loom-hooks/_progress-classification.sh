#!/usr/bin/env bash
# Shared tokenized recognition of status-only Bash commands.
# Source _common.sh first. Defining these functions has no source-time effects.

# Echo each command segment's effective command-word index.
_loom_poll_command_indices() {
	local n=${#LOOM_TOKENS[@]} i=0 j
	while ((i < n)); do
		j=$(loom_tokens_command_word_index "$i") && printf '%s\n' "$j"
		while ((i < n)) && [[ "${LOOM_TOKENS[$i]}" != "%%SEP%%" ]]; do i=$((i + 1)); done
		i=$((i + 1))
	done
}

# `git status` always counts; `git log` only without a path argument.
_loom_poll_git_is_countable() {
	local j="$1" sub="${LOOM_TOKENS[$((j + 1))]:-}"
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
	local j="$1" arg="${LOOM_TOKENS[$((j + 1))]:-}"
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
			[[ "$ch" == $'\\' ]] && { i=$((i + 2)); continue; }
			[[ "$ch" == "'" ]] && { quote=single; i=$((i + 1)); continue; }
			[[ "$ch" == '"' ]] && { quote=double; i=$((i + 1)); continue; }
			next="${raw:$((i + 1)):1}"
			case "$ch" in '&' | ';' | '<' | '>') return 1 ;; '|') [[ "$next" == '|' ]] && return 1 ;; esac
		fi
		i=$((i + 1))
	done
	[[ -z "$quote" ]]
}

_loom_progress_subagents_flags_valid() {
	local op="$1" i="$2" n=${#LOOM_TOKENS[@]} tok
	while ((i < n)) && [[ "${LOOM_TOKENS[$i]}" != "%%SEP%%" ]]; do
		tok="${LOOM_TOKENS[$i]}"
		case "$op:$tok" in
		list:--json | list:-h | list:--help | harvest:-h | harvest:--help | \
		watch:--json | watch:-h | watch:--help) ;;
		list:--session=* | list:--dir=* | list:--debounce=* | \
		harvest:--id=* | harvest:--session=* | harvest:--dir=* | harvest:--debounce=* | \
		watch:--timeout=* | watch:--session=* | watch:--dir=* | watch:--worker=*) ;;
		list:--session | list:--dir | list:--debounce | \
		harvest:--id | harvest:--session | harvest:--dir | harvest:--debounce | \
		watch:--timeout | watch:--session | watch:--dir | watch:--worker)
			i=$((i + 1))
			((i < n)) && [[ "${LOOM_TOKENS[$i]}" != "%%SEP%%" ]] || return 1
			;;
		*) return 1 ;;
		esac
		i=$((i + 1))
	done
	return 0
}

# loom_progress_subagents_op <segment-start> - Print the recognized operation.
# LOOM_TOKENS must already contain a successfully tokenized command.
loom_progress_subagents_op() {
	local start="${1:-0}" j op
	[[ "$start" =~ ^[0-9]+$ ]] || return 1
	j=$(loom_tokens_command_word_index "$start") || return 1
	[[ "${LOOM_TOKENS[$j]##*/}" == loom && "${LOOM_TOKENS[$((j + 1))]:-}" == subagents ]] || return 1
	op="${LOOM_TOKENS[$((j + 2))]:-}"
	case "$op" in list | harvest | watch) ;; *) return 1 ;; esac
	_loom_progress_subagents_flags_valid "$op" "$((j + 3))" || return 1
	printf '%s' "$op"
}

_loom_progress_display_segment_is_countable() {
	local start="$1" n=${#LOOM_TOKENS[@]} j base k
	j=$(loom_tokens_command_word_index "$start") || return 1
	((j == start)) || return 1
	base="${LOOM_TOKENS[$j]##*/}"
	k=$((j + 1))
	case "$base" in
	head | tail)
		[[ "${LOOM_TOKENS[$k]:-}" == -n ]] || return 1
		k=$((k + 1))
		[[ "${LOOM_TOKENS[$k]:-}" =~ ^[0-9]+$ && $((k + 1)) -eq $n ]]
		;;
	rg) [[ $((k + 1)) -eq $n && "${LOOM_TOKENS[$k]:-}" != "%%SEP%%" ]] ;;
	*) return 1 ;;
	esac
}

# Print the subagents operation when the whole command is that observation,
# optionally followed by the one display pipe accepted by the poll guard.
_loom_progress_subagents_pipeline_op() {
	local raw="$1" op i n=${#LOOM_TOKENS[@]}
	_loom_poll_pipeline_only "$raw" || return 1
	op=$(loom_progress_subagents_op 0) || return 1
	i=0
	while ((i < n)) && [[ "${LOOM_TOKENS[$i]}" != "%%SEP%%" ]]; do i=$((i + 1)); done
	if ((i < n)); then
		i=$((i + 1))
		_loom_progress_display_segment_is_countable "$i" || return 1
	fi
	printf '%s' "$op"
}

# Print a wrapper-independent key for the recognized subagents invocation.
# The optional display pipeline is deliberately absent: it does not change
# which one-shot diagnostic or owned wait was invoked.
_loom_progress_subagents_key() {
	local raw="$1" op j i n=${#LOOM_TOKENS[@]} key="loom" tok word
	local -a workers=()
	op=$(_loom_progress_subagents_pipeline_op "$raw") || return 1
	j=$(loom_tokens_command_word_index 0) || return 1
	i=$((j + 1))
	if [[ "$op" == watch ]]; then
		key="loom subagents watch"
		i=$((j + 3))
		while ((i < n)) && [[ "${LOOM_TOKENS[$i]}" != "%%SEP%%" ]]; do
			tok="${LOOM_TOKENS[$i]}"
			case "$tok" in
			--worker=*) workers+=("${tok#--worker=}") ;;
			--worker)
				i=$((i + 1))
				workers+=("${LOOM_TOKENS[$i]}")
				;;
			esac
			i=$((i + 1))
		done
		while IFS= read -r word; do
			[[ -n "$word" ]] || continue
			printf -v tok '%q' "$word"
			key+=" --worker ${tok}"
		done < <(printf '%s\n' "${workers[@]}" | LC_ALL=C sort -u)
		printf '%s' "$key"
		return 0
	fi
	while ((i < n)) && [[ "${LOOM_TOKENS[$i]}" != "%%SEP%%" ]]; do
		printf -v word '%q' "${LOOM_TOKENS[$i]}"
		key="${key} ${word}"
		i=$((i + 1))
	done
	printf '%s' "$key"
}

# An owned watch names at least one concrete worker. The CLI rejects the
# legacy unowned form, so it must not acquire a poll-guard wait identity.
_loom_progress_subagents_watch_is_owned() {
	local j i n=${#LOOM_TOKENS[@]} tok
	j=$(loom_tokens_command_word_index 0) || return 1
	[[ "${LOOM_TOKENS[$((j + 2))]:-}" == watch ]] || return 1
	i=$((j + 3))
	while ((i < n)) && [[ "${LOOM_TOKENS[$i]}" != "%%SEP%%" ]]; do
		tok="${LOOM_TOKENS[$i]}"
		case "$tok" in
		--worker=claude:?* | --worker=codex:?*) return 0 ;;
		--worker=*) ;;
		--worker)
			i=$((i + 1))
			((i < n)) && [[ "${LOOM_TOKENS[$i]}" != "%%SEP%%" ]] || return 1
			case "${LOOM_TOKENS[$i]}" in claude:?* | codex:?*) return 0 ;; esac
			;;
		esac
		i=$((i + 1))
	done
	return 1
}

_loom_progress_is_verify_runner_command() {
	loom_tokens_invoke 'cargo|pytest|tsc|eslint|go|npm|bun|pnpm|yarn|make'
}

_loom_poll_is_countable() {
	local raw="$1" tok j base
	_loom_poll_pipeline_only "$raw" || return 1
	for tok in "${LOOM_TOKENS[@]}"; do [[ "$tok" == "%%SEP%%" ]] && return 1; done
	_loom_progress_is_verify_runner_command && return 1
	j=$(loom_tokens_command_word_index 0) || return 1
	base="${LOOM_TOKENS[$j]##*/}"
	case "$base" in
	ls | wc | test | '[' | stat | sleep | date | pwd) return 0 ;;
	git) _loom_poll_git_is_countable "$j" ;;
	cat) _loom_poll_cat_is_countable "$j" ;;
	*) return 1 ;;
	esac
}

_loom_progress_shell_connectors_safe() {
	local raw="$1" i=0 n=${#1} quote="" ch next
	while ((i < n)); do
		ch="${raw:$i:1}"
		if [[ "$quote" == single ]]; then [[ "$ch" == "'" ]] && quote=""
		elif [[ "$quote" == double ]]; then
			[[ "$ch" == $'\\' ]] && i=$((i + 1)) || { [[ "$ch" == '"' ]] && quote=""; }
		else
			[[ "$ch" == $'\\' ]] && { i=$((i + 2)); continue; }
			[[ "$ch" == "'" ]] && { quote=single; i=$((i + 1)); continue; }
			[[ "$ch" == '"' ]] && { quote=double; i=$((i + 1)); continue; }
			next="${raw:$((i + 1)):1}"
			case "$ch" in '<' | '>' | '(' | ')' | '`') return 1 ;; '&') [[ "$next" == '&' ]] || return 1; i=$((i + 1)) ;; '|') [[ "$next" == '|' ]] && i=$((i + 1)) ;; esac
		fi
		i=$((i + 1))
	done
	[[ -z "$quote" ]]
}

_loom_progress_segment_is_observation() {
	local start="$1" j base
	loom_progress_subagents_op "$start" >/dev/null && return 0
	j=$(loom_tokens_command_word_index "$start") || return 1
	base="${LOOM_TOKENS[$j]##*/}"
	case "$base" in
	ls | wc | test | '[' | stat | sleep | date | pwd) return 0 ;;
	git) _loom_poll_git_is_countable "$j" ;;
	cat) _loom_poll_cat_is_countable "$j" ;;
	*) return 1 ;;
	esac
}

_loom_progress_tokens_are_observation() {
	local raw="$1" n=${#LOOM_TOKENS[@]} i=0 seen=0
	((n > 0)) || return 1
	[[ "${LOOM_TOKENS[0]}" != "%%SEP%%" && "${LOOM_TOKENS[$((n - 1))]}" != "%%SEP%%" ]] || return 1
	_loom_progress_shell_connectors_safe "$raw" || return 1
	_loom_progress_subagents_pipeline_op "$raw" >/dev/null && return 0
	while ((i < n)); do
		_loom_progress_segment_is_observation "$i" || return 1
		seen=1
		while ((i < n)) && [[ "${LOOM_TOKENS[$i]}" != "%%SEP%%" ]]; do i=$((i + 1)); done
		i=$((i + 1))
	done
	((seen == 1))
}

# loom_progress_classify_bash <command> - Print exactly observation or progress.
# Classification is deliberately fail-closed toward useful progress and always
# returns success so heartbeat writing cannot be suppressed by parse failures.
loom_progress_classify_bash() {
	local raw="${1-}" stripped
	if [[ $# -ne 1 || -z "$raw" ]]; then printf 'progress'; return 0; fi
	stripped=$(strip_embedded_content "$raw") || { printf 'progress'; return 0; }
	if [[ -z "$stripped" ]] || ! loom_tokenize_command "$stripped"; then printf 'progress'; return 0; fi
	if _loom_progress_tokens_are_observation "$stripped"; then printf 'observation'; else printf 'progress'; fi
	return 0
}
