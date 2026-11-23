#!/usr/bin/env bash
# loom-relay-guard.sh - PreToolUse:Bash guard that refuses a Bash call which
# would hide the LOOM_RELAY_V1 line of a loom command from the relay hook.
#
# A confined session files block, commit, dispute, handoff, verdict, merge,
# freeze and check requests by printing one LOOM_RELAY_V1 line on stdout;
# loom-relay.sh (PostToolUse) reads that line from the call's output. A control
# ticket is never swept from disk later, so a call that backgrounds the command,
# redirects or captures its stdout, or pipes it through a filter loses the
# request while the agent believes it was filed. This guard refuses those
# spellings before the command runs.
#
# Rule relay-stdout: a guarded invocation (block, commit, dispute, file-dispute,
#   merge-resolved, verdict, freeze-contracts, handoff, check) may not run in
#   the background, inside a capture ($(...), backticks, a subshell or a
#   `sh -c` payload), nor with stdout redirected, detached or piped anywhere but
#   into a closing `tail -N` / `tail -n N`.
# Rule relay-filter: no pipe consumer of a pipeline holding any loom relay
#   command may filter on LOOM_R / RELAY_V1 / ^LOOM (grep -v '^LOOM_RELAY').
# Not guarded: complete and subagents:control (pinned by loom-control-complete.sh
#   and loom-control-subagents.sh), memory and telemetry (swept from disk).
#
# Call marker: a call that passes and runs any control kind (the guarded kinds
#   plus complete and subagents:control) also gets an empty file
#   $LOOM_SCRATCH_DIR/.pre-<tool_use_id>. The relay hook relays a control ticket
#   for the lead only when the ticket was written at or after that marker, so a
#   ticket a subagent planted in the shared scratch directory earlier cannot be
#   relayed as the lead's by a later call that happens to print its line. The
#   marker is written last, after every refusal, so a blocked call leaves none;
#   the relay deletes it, and markers of calls that never reached PostToolUse
#   are removed here once an hour old.
#
# Input:  the PreToolUse JSON payload on stdin.
# Output: guidance on stderr when blocking.
# Exit:   2 to block (fails closed without jq); 0 otherwise. Outside a loom
#         session (LOOM_SESSION_ID or LOOM_SCRATCH_DIR unset) always 0.

PATH="${LOOM_HOOK_PATH:-$PATH}"

set -euo pipefail

[[ -n ${LOOM_SESSION_ID:-} && -n ${LOOM_SCRATCH_DIR:-} ]] || exit 0

# Only the input read and one jq call run before the fast path below; the
# libraries load after it. Without jq the guard fails closed, so that one case
# loads _common.sh early to print the install guidance and exit 2.
source "$(dirname "${BASH_SOURCE[0]}")/_bounded.sh"
INPUT_JSON=$(loom_read_input 1 2>/dev/null || true)
if ! command -v jq >/dev/null 2>&1; then
	source "$(dirname "${BASH_SOURCE[0]}")/_common.sh"
	loom_require_jq loom-relay-guard.sh
fi

# One jq call: tool name, tool_use_id, background flag, then the command (which
# may span lines) last. The trailing "." keeps trailing newlines of the command.
FIELDS=$(printf '%s' "$INPUT_JSON" | jq -r '
	(.tool_name // "" | tostring | gsub("[\r\n]"; " ")),
	(.tool_use_id // "" | tostring | gsub("[\r\n]"; " ")),
	(.tool_input.run_in_background // false | tostring),
	(.tool_input.command // "" | tostring), "."' 2>/dev/null) || exit 0
FIELDS=${FIELDS%$'\n.'}
TOOL_NAME=${FIELDS%%$'\n'*}
FIELDS=${FIELDS#*$'\n'}
TOOL_USE_ID=${FIELDS%%$'\n'*}
FIELDS=${FIELDS#*$'\n'}
BACKGROUND=${FIELDS%%$'\n'*}
COMMAND=${FIELDS#*$'\n'}
[[ "$TOOL_NAME" == Bash ]] || exit 0
case "$COMMAND" in
*loom* | *LOOM_R* | *RELAY_V1*) ;;
*) exit 0 ;;
esac

source "$(dirname "${BASH_SOURCE[0]}")/_common.sh"
source "$(dirname "${BASH_SOURCE[0]}")/_relay_kinds.sh"
loom_require_jq loom-relay-guard.sh

# An untokenizable command (a quoted heredoc strip_embedded_content cannot
# model, say) is left to loom-relay.sh, which tells the agent to rerun alone.
loom_tokenize_command "$(strip_embedded_content "$COMMAND")" || exit 0

# guarded_kind <kind> - Exit 0 for the kinds whose relay line this guard protects.
guarded_kind() {
	case "$1" in
	block | commit | dispute | file-dispute | merge-resolved | verdict | freeze-contracts | handoff | check) return 0 ;;
	esac
	return 1
}

# command_words <index> - `loom <sub> <verb>`: the invocation's words up to the
# first option or separator, for the refusal text.
command_words() {
	local k=$(($1 + 1)) n=${#LOOM_TOKENS[@]} words=loom
	while ((k < n && k < $1 + 3)) && [[ "${LOOM_TOKENS[$k]}" != "%%SEP%%" && "${LOOM_TOKENS[$k]}" != -* ]]; do
		words="$words ${LOOM_TOKENS[$k]}"
		k=$((k + 1))
	done
	printf '%s' "$words"
}

# block <rule> <index> <kind> <reason> - Refuse the call.
block() {
	printf '[loom-relay-guard/%s] BLOCKED: `%s` relays a %s request through the LOOM_RELAY_V1 line on its stdout, and this call would hide it (%s). Run it alone, in the foreground, unredirected; `2>&1 | tail -N` is the only filter allowed.\n' \
		"$1" "$(command_words "$2")" "$3" "$4" >&2
	loom_record_block loom-relay-guard "$1" Bash "$COMMAND"
	exit 2
}

# in_capture <index> - True when the invocation's command word sits inside a
# substitution, backticks, a subshell or an `sh -c` payload at any depth, in any
# segment of it (`out=$(cd d && loom ...)`).
in_capture() {
	local depth
	loom_token_depth "$1" depth
	((depth > 0))
}

check_stdout_rule() {
	local idx kind fate
	while read -r idx kind; do
		guarded_kind "$kind" || continue
		if [[ "$BACKGROUND" == true ]]; then
			block relay-stdout "$idx" "$kind" "it runs in the background"
		fi
		if in_capture "$idx"; then
			block relay-stdout "$idx" "$kind" "it runs inside a command substitution, subshell or sh -c payload; run it as a bare top-level command"
		fi
		fate=$(relay_stdout_fate "$idx")
		case "$fate" in
		captured) block relay-stdout "$idx" "$kind" "its stdout is redirected or piped away" ;;
		detached) block relay-stdout "$idx" "$kind" "it is detached with &" ;;
		esac
	done < <(relay_each_invocation)
}

# is_filter_segment <start> <end> - True when the segment [start, end) is a
# filter command that names the relay line: a known filter as its command word
# with a token holding LOOM_R, RELAY_V1 or ^LOOM.
is_filter_segment() {
	local start=$1 end=$2 j k tok
	j=$(loom_tokens_command_word_index "$start") || return 1
	case "${LOOM_TOKENS[$j]##*/}" in
	grep | egrep | fgrep | rg | ag | sed | awk | perl | python | python3 | jq | head | cut | tr | sort | uniq | tee | wc | cat) ;;
	*) return 1 ;;
	esac
	for ((k = j + 1; k < end; k++)); do
		tok=${LOOM_TOKENS[$k]}
		case "$tok" in
		*LOOM_R* | *RELAY_V1* | *'^LOOM'*) return 0 ;;
		esac
	done
	return 1
}

# check_filter_rule - Walk the segments, flushing a pipeline at `;`, `&&`, `||`,
# a lone `&` or a bare newline. A pipeline that holds a loom relay command and a
# consumer that filters on the relay line is refused. A group (`{ }`, `if`, a
# loop, `case`) counts as one pipeline stage holding every relay command inside
# it, so `{ loom ...; echo; } | grep -v LOOM_RELAY` is refused. `first` is the
# "<index>:<kind>" of the first relay command of the current pipeline, `grp` of
# the current group; each open group saves both on a stack.
check_filter_rule() {
	local n=${#LOOM_TOKENS[@]} marks=" " idx kind pair first="" grp="" gk sp=0
	local i=0 start run=""
	local stk_first=() stk_grp=()
	while read -r idx kind; do marks="$marks$idx:$kind "; done < <(relay_each_invocation)
	[[ "$marks" != " " ]] || return 0
	while ((i <= n)); do
		start=$i
		while ((i < n)) && [[ "${LOOM_TOKENS[$i]}" != "%%SEP%%" ]]; do i=$((i + 1)); done
		if ((i > start)); then
			case "$run" in
			'&>'*) ;;
			';'* | '&&'* | '||'* | '&'*) first="" ;;
			esac
			_relay_segment_group "$start" gk
			if [[ "$gk" == open ]]; then
				stk_first[$sp]=$first
				stk_grp[$sp]=$grp
				sp=$((sp + 1))
				first=""
				grp=""
			elif [[ "$gk" == close ]] && ((sp > 0)); then
				sp=$((sp - 1))
				pair=$grp
				first=${stk_first[$sp]:-}
				grp=${stk_grp[$sp]:-}
				[[ -n "$first" ]] || first=$pair
				[[ -n "$grp" ]] || grp=$pair
			fi
			idx=$(loom_tokens_command_word_index "$start" || true)
			if [[ -n "$idx" && "$marks" == *" $idx:"* ]]; then
				kind=${marks#*" $idx:"}
				pair="$idx:${kind%% *}"
				[[ -n "$first" ]] || first=$pair
				[[ -n "$grp" ]] || grp=$pair
			fi
			case "$run" in
			'||'*) ;;
			'|'*)
				if [[ -n "$first" ]] && is_filter_segment "$start" "$i"; then
					block relay-filter "${first%%:*}" "${first#*:}" "a pipe consumer filters on the relay line"
				fi
				;;
			esac
		fi
		if ((i < n)); then
			_relay_sep_core "$i" run
		fi
		i=$((i + 1))
	done
}

# write_call_marker - Leave `.pre-<tool_use_id>` in the scratch directory for a
# call that runs a control kind. A tool_use_id of any other shape gets none, and
# the relay treats the marker as absent.
write_call_marker() {
	local idx kind marker has_control=false
	local id_pattern='^[A-Za-z0-9_-]{1,128}$'
	while read -r idx kind; do
		if relay_kind_is_control "$kind"; then
			has_control=true
			break
		fi
	done < <(relay_each_invocation)
	[[ "$has_control" == true && "$TOOL_USE_ID" =~ $id_pattern ]] || return 0
	marker="$LOOM_SCRATCH_DIR/.pre-$TOOL_USE_ID"
	find "$LOOM_SCRATCH_DIR" -maxdepth 1 -type f -name '.pre-*' -mmin +60 -exec rm -f {} + 2>/dev/null || true
	rm -f "$marker" 2>/dev/null || true
	# noclobber makes the redirect create the file exclusively, so a link planted
	# at the marker path between the rm and the write is not followed.
	(umask 077 && set -C && : >"$marker") 2>/dev/null || true
}

check_stdout_rule
check_filter_rule
write_call_marker
exit 0
