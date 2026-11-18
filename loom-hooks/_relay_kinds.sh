#!/usr/bin/env bash
# Shared relay-kind library, sourced by loom-relay.sh (PostToolUse), the relay
# guard (PreToolUse) and the relay Stop hook. Functions only: nothing runs at
# source time, and `relay_kind_is_control` needs no other library, so a test can
# source this file alone. The parsing helpers read the tokenizer's globals
# (LOOM_TOKENS, LOOM_TOKEN_SEPS, LOOM_TOKEN_META) and loom_tokens_command_word_index from
# _common.sh. Bash 3.2 compatible.
if [[ "${_LOOM_RELAY_KINDS_LOADED:-}" == 1 ]]; then return 0; fi
_LOOM_RELAY_KINDS_LOADED=1

# segment_has_arg <index> <arg> - True when a later word of the command segment
# whose command word sits at LOOM_TOKENS[<index>] is exactly <arg>.
segment_has_arg() {
	local k=$(($1 + 1)) n=${#LOOM_TOKENS[@]}
	while ((k < n)) && [[ "${LOOM_TOKENS[$k]}" != "%%SEP%%" ]]; do
		[[ "${LOOM_TOKENS[$k]}" == "$2" ]] && return 0
		k=$((k + 1))
	done
	return 1
}

# segment_has_option <index> <option> <value> - True when the command segment
# whose command word sits at LOOM_TOKENS[<index>] passes <option> <value>, as
# two words or as <option>=<value>.
segment_has_option() {
	local k=$(($1 + 1)) n=${#LOOM_TOKENS[@]}
	while ((k < n)) && [[ "${LOOM_TOKENS[$k]}" != "%%SEP%%" ]]; do
		[[ "${LOOM_TOKENS[$k]}" == "$2=$3" ]] && return 0
		[[ "${LOOM_TOKENS[$k]}" == "$2" && "${LOOM_TOKENS[$((k + 1))]:-}" == "$3" ]] && return 0
		k=$((k + 1))
	done
	return 1
}

# relay_kind_at <index> - Echo the request kind written by the loom invocation
# whose command word sits at LOOM_TOKENS[<index>], or nothing (section 5).
# A new relay request kind needs one case here, plus relay_kind_is_control when
# it is a control kind.
relay_kind_at() {
	local j=$1 n=${#LOOM_TOKENS[@]} sub="" verb=""
	((j + 1 < n)) && sub=${LOOM_TOKENS[$((j + 1))]}
	((j + 2 < n)) && verb=${LOOM_TOKENS[$((j + 2))]}
	[[ "$verb" == "%%SEP%%" ]] && verb=""
	case "$sub:$verb" in
	memory:note | memory:decision | memory:change | memory:question | memory:resolve) echo memory ;;
	stage:block) echo block ;;
	stage:commit) echo commit ;;
	stage:complete) echo complete ;;
	stage:dispute-criteria)
		# --field impact-tests files a plan v2 dispute kind, not a criterion.
		if segment_has_option "$j" --field impact-tests; then echo file-dispute; else echo dispute; fi
		;;
	stage:dispute-findings | stage:dispute-contract | stage:dispute-integrity) echo file-dispute ;;
	stage:merge) segment_has_arg "$j" --resolved && echo merge-resolved ;;
	stage:adjudicate) echo verdict ;;
	stage:contracts) [[ "${LOOM_TOKENS[$((j + 3))]:-}" == freeze ]] && echo freeze-contracts ;;
	knowledge:context) echo telemetry ;;
	subagents:start | subagents:message | subagents:cancel) echo subagents:control ;;
	check:*) echo check ;;
	handoff:*) echo handoff ;;
	esac
	return 0
}

# relay_kind_is_control <kind> - Exit 0 for exactly the ten control kinds: the
# ones only a session's lead may relay, never swept from a leftover ticket
# (relay/kind.rs RequestKind::is_control). A parity test pins this list.
relay_kind_is_control() {
	case "$1" in
	block | commit | complete | dispute | handoff | merge-resolved | verdict | freeze-contracts | file-dispute | subagents:control) return 0 ;;
	esac
	return 1
}

# relay_each_invocation - Print one "<token index> <kind>" line per loom
# invocation in LOOM_TOKENS that writes a relay request. Walks command
# positions the way loom_tokens_invoke does, so VAR= prefixes, wrappers and
# `sh -c` payloads are seen through while quoted prose stays one argument. The
# index is the invocation's command word.
relay_each_invocation() {
	local n=${#LOOM_TOKENS[@]} i=0 at_cmd_pos=1 j kind
	while ((i < n)); do
		if [[ "${LOOM_TOKENS[$i]}" == "%%SEP%%" ]]; then
			at_cmd_pos=1
			i=$((i + 1))
			continue
		fi
		if [[ $at_cmd_pos -eq 1 ]] && j=$(loom_tokens_command_word_index "$i") &&
			[[ "${LOOM_TOKENS[$j]##*/}" == loom ]]; then
			kind=$(relay_kind_at "$j")
			if [[ -n "$kind" ]]; then printf '%s %s\n' "$j" "$kind"; fi
		fi
		at_cmd_pos=0
		i=$((i + 1))
	done
}

# relay_allowed_kinds - Echo, comma-separated and de-duplicated, the kinds
# written by every loom invocation of LOOM_TOKENS.
relay_allowed_kinds() {
	local idx kind kinds=""
	while read -r idx kind; do
		if [[ -n "$kind" && ",$kinds," != *",$kind,"* ]]; then
			kinds=${kinds:+$kinds,}$kind
		fi
	done < <(relay_each_invocation)
	printf '%s' "$kinds"
}

# _relay_sep_core <index> <var> - Set <var> to the separator run at
# LOOM_TOKENS[<index>] without its grouping characters: `(`, `)`, `<` (which
# also covers a `<<` heredoc opener), backticks and newlines. What is left is
# the operator that decides where stdout goes. A run of newlines alone ends a
# command like `;`, so it reads as `;`; a newline next to `)` (the heredoc
# idiom's closing line) was already dropped by loom_token_sep.
_relay_sep_core() {
	local core
	loom_token_sep "$1" core
	if [[ -n "$core" && -z "${core//$'\n'/}" ]]; then core=';'; fi
	core=${core//')'/}
	core=${core//'('/}
	core=${core//'<'/}
	core=${core//'`'/}
	core=${core//$'\n'/}
	printf -v "$2" '%s' "$core"
}

# _relay_segment_group <start> <var> - Set <var> to `open` when the command
# segment starting at LOOM_TOKENS[<start>] begins a group (`{`, `if`, `for`,
# `while`, `until`, `case`, `select`), to `close` when it ends one (`}`, `fi`,
# `done`, `esac`), else to nothing. `then`, `do`, `else`, `elif` and `!` before
# the word are skipped: `do while ...` opens a group.
_relay_segment_group() {
	local k=$1 n=${#LOOM_TOKENS[@]} kind=""
	while ((k < n)); do
		case "${LOOM_TOKENS[$k]}" in
		then | do | else | elif | '!')
			k=$((k + 1))
			continue
			;;
		'{' | if | for | while | until | case | select) kind=open ;;
		'}' | fi | done | esac) kind=close ;;
		esac
		break
	done
	printf -v "$2" '%s' "$kind"
}

# _relay_group_exit <sep_index> <var> - LOOM_TOKENS[<sep_index>] is the `;`,
# `&&`, `||` or newline run that ends the command of a loom invocation. Scan the
# following segments at the same nesting depth, counting group openers against
# closers (_relay_segment_group). The first closer with no opener from the scan
# closes the group that holds the invocation: set <var> to the index of the
# separator run after that closer, where its redirect or pipe begins, and
# return 0. Return 1 when the scan reaches the end of the command, or the depth
# drops, without such a closer: the invocation ends its own pipeline.
_relay_group_exit() {
	local k=$(($1 + 1)) n=${#LOOM_TOKENS[@]} base d gk level=0 closed=0
	loom_token_depth "$1" base
	while ((k < n)); do
		loom_token_depth "$k" d
		((d >= base)) || return 1
		_relay_segment_group "$k" gk
		case "$gk" in
		open) level=$((level + 1)) ;;
		close) if ((level == 0)); then closed=1; else level=$((level - 1)); fi ;;
		esac
		while ((k < n)); do
			if [[ "${LOOM_TOKENS[$k]}" == "%%SEP%%" ]]; then
				loom_token_depth "$k" d
				((d <= base)) && break
			fi
			k=$((k + 1))
		done
		if ((closed == 1)); then
			((k < n)) || return 1
			printf -v "$2" '%s' "$k"
			return 0
		fi
		((k < n)) || return 1
		((d == base)) || return 1
		k=$((k + 1))
	done
	return 1
}

# _relay_tail_operand <index> <var> - Return 0 when LOOM_TOKENS[<index>] is
# exactly `tail -N` or `tail -n N` (N >= 1, no file operand, nothing after
# it in its segment). Sets <var> to the index of the N word of `-n N`, else -1.
_relay_tail_operand() {
	local c=$1 n=${#LOOM_TOKENS[@]} a1 a2 a3 found=-1
	[[ "${LOOM_TOKENS[$c]}" == tail ]] || return 1
	a1=${LOOM_TOKENS[$((c + 1))]:-}
	a2=${LOOM_TOKENS[$((c + 2))]:-}
	a3=${LOOM_TOKENS[$((c + 3))]:-}
	if [[ "$a1" =~ ^-[0-9]{1,9}$ ]] && ((10#${a1#-} >= 1)) &&
		{ ((c + 2 >= n)) || [[ "$a2" == "%%SEP%%" ]]; }; then
		:
	elif [[ "$a1" == -n && "$a2" =~ ^[0-9]{1,9}$ ]] && ((10#$a2 >= 1)) &&
		{ ((c + 3 >= n)) || [[ "$a3" == "%%SEP%%" ]]; }; then
		found=$((c + 2))
	else
		return 1
	fi
	printf -v "$2" '%s' "$found"
}

# relay_stdout_fate <index> - Print where the stdout of the loom invocation whose
# command word sits at LOOM_TOKENS[<index>] ends up, as one word: `free` (the
# call's output keeps its last line), `captured` (redirected to a file, piped to
# anything but a closing `tail -N`) or `detached` (a lone `&`). The relay hook
# reads the LOOM_RELAY_V1 line from that stdout, so only `free` relays.
#
# Walks the separator runs after the command word (modeled on poll-guard.sh's
# _loom_read_stdout_captured, which lives in an executed hook and cannot be
# sourced):
#   - empty runs (substitution and `sh -c` splices) and runs of only `)`, `<`,
#     `<<` or a backtick continue the walk (a bare newline does not, below): the commit idiom
#     `-m "$(cat <<'EOF' ... EOF\n)" 2>&1 | rg x` yields the runs empty, `<<`,
#     `\n)`, `>&`, `|`, and a walk that stopped at the first empty run would
#     never see the pipe;
#   - `;`, `&&`, `||` and a bare newline end the command: free (a `;` inside
#     `$(...)` is an accepted fail-open), unless the invocation sits in a group
#     (`{ }`, `if`, a loop, `case`): _relay_group_exit finds the closer, even
#     past other commands, and the walk goes on to what follows it;
#   - `2>&1`, `|&`, `>&N` and a redirect on a descriptor other than 1
#     (`2>/dev/null`) leave stdout alone;
#   - `>`, `>>`, `1>`, `&>` capture it and a lone `&` detaches it;
#   - `|` is allowed only into a closing `tail -N` / `tail -n N`; any other
#     consumer, or a second pipe stage, captures.
relay_stdout_fate() {
	local i=$(($1 + 1)) n=${#LOOM_TOKENS[@]} allow_pipe=1 operand=-1 run prev next
	while ((i < n)); do
		if [[ "${LOOM_TOKENS[$i]}" != "%%SEP%%" ]]; then
			i=$((i + 1))
			continue
		fi
		_relay_sep_core "$i" run
		prev=${LOOM_TOKENS[$((i - 1))]:-}
		next=${LOOM_TOKENS[$((i + 1))]:-}
		case "$run" in
		';'* | '&&'* | '||'*)
			# The command ends. When the invocation sits in a group
			# (`{ loom ...; echo; } | rg x`, `do loom ...; done > f`), the
			# group's redirect or pipe applies to it: resume after the closer.
			_relay_group_exit "$i" i || break
			continue
			;;
		'|'*)
			if ((allow_pipe == 0)) || ! _relay_tail_operand $((i + 1)) operand; then
				echo captured
				return 0
			fi
			allow_pipe=0
			;;
		'&>'*)
			echo captured
			return 0
			;;
		'&'*)
			echo detached
			return 0
			;;
		'>&'*)
			if ! [[ "$next" =~ ^[0-9]+$ ]]; then
				echo captured
				return 0
			fi
			;;
		'>'*)
			# `2>/dev/null` leaves stdout alone, but only when the digit is glued
			# to the `>`: `--criterion 2 > log` redirects stdout. The N of
			# `tail -n N` is not a descriptor either.
			if ! [[ "$prev" =~ ^[0-9]+$ && "$prev" != 1 ]] || ! loom_token_glued "$i" || ((i - 1 == operand)); then
				echo captured
				return 0
			fi
			;;
		esac
		i=$((i + 1))
	done
	echo free
}
