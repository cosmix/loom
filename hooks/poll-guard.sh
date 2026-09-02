#!/usr/bin/env bash
# poll-guard.sh - PreToolUse hook (matcher: Bash) discouraging wasted turns:
#
#   1. A long `sleep N` (N >= 30s) - wait on `loom subagents watch` instead.
#   2. A read-only polling command line (git status, ls, ...) repeated 3+
#      times this session - build/test/lint runners are exempt outright.
#   3. Bash-side `cat`/`head`/`tail`/`sed -n` reads of a file - reuses
#      read-guard.sh's rules 1-3 verbatim via hooks/_read_discipline.sh.
#   4. A pathless `git show`/`git diff` - the largest output producer after
#      Read.
#
# See CLAUDE.md rule 14 (token efficiency) and rule 6 ("loom subagents
# watch", not a hand-rolled poll loop).
#
# Input: JSON from stdin - {"tool_name": "Bash", "tool_input": {"command":
# ...}, "agent_id": ..., "session_id": ...}
# Exit codes: 0 = allow (optionally with a LOOM_HOOK_WARN additionalContext
# joining every rule that fired), 2 = deny with guidance on stderr (only
# when the deny switch is enabled AND a live loom session is running above
# this process - see loom_hook_deny_or_warn in _read_discipline.sh).

set -euo pipefail

source "$(dirname "$0")/_common.sh"
source "$(dirname "$0")/_read_discipline.sh"

# --- Rule 1: long sleep -------------------------------------------------

# _loom_sleep_argument - echo the first `sleep` invocation's argv[1] found
# in LOOM_TOKENS (command position, wrapper-unwrapped), or return 1 if there
# is none.
_loom_sleep_argument() {
	local n=${#LOOM_TOKENS[@]} i=0 at_cmd_pos=1 j base arg
	while ((i < n)); do
		if [[ "${LOOM_TOKENS[$i]}" == "%%SEP%%" ]]; then
			at_cmd_pos=1
			i=$((i + 1))
			continue
		fi
		if [[ $at_cmd_pos -eq 1 ]] && j=$(loom_tokens_command_word_index "$i"); then
			base="${LOOM_TOKENS[$j]##*/}"
			if [[ "$base" == "sleep" ]]; then
				arg="${LOOM_TOKENS[$((j + 1))]:-}"
				if [[ -n "$arg" && "$arg" != "%%SEP%%" ]]; then
					printf '%s' "$arg"
					return 0
				fi
			fi
		fi
		at_cmd_pos=0
		i=$((i + 1))
	done
	return 1
}

# _loom_poll_rule_sleep - warn on `sleep N` >= 30s, accepting sleep's own
# s/m/h/d unit suffixes. Anything this cannot parse as a number is treated
# as not matching.
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

# --- Rule 2: repeated read-only command lines ---------------------------

# _loom_poll_git_is_countable - `git status` always counts; `git log` counts only with no path argument.
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

# _loom_poll_cat_is_countable - `cat` counts only when its argument is a
# state-directory path (legacy `.work` or current `.loom/work`).
_loom_poll_cat_is_countable() {
	local j="$1"
	local arg="${LOOM_TOKENS[$((j + 1))]:-}"
	[[ -z "$arg" || "$arg" == "%%SEP%%" ]] && return 1
	case "$arg" in
	*.work/* | */.work | *.loom/work/* | */.loom/work) return 0 ;;
	esac
	return 1
}

# _loom_poll_is_countable - rule 2's allowlist over the already-tokenized
# command (segment 0's effective command word). Never counts a build/test/
# lint runner. Deliberately narrow - "when in doubt, do not count".
_loom_poll_is_countable() {
	_loom_is_verify_runner_command && return 1
	local j base
	j=$(loom_tokens_command_word_index 0) || return 1
	base="${LOOM_TOKENS[$j]##*/}"
	case "$base" in
	ls | wc | test | '[' | stat | sleep | date | pwd) return 0 ;;
	git) _loom_poll_git_is_countable "$j" ;;
	cat) _loom_poll_cat_is_countable "$j" ;;
	*) return 1 ;;
	esac
}

# _loom_poll_rule_repeat <stripped-command> <agent-id> <fallback-sid> - warn
# at 3rd/4th identical read-only polling line this session, deny at 5th+.
_loom_poll_rule_repeat() {
	local stripped="$1" agent_id="$2" fallback_sid="$3"
	_loom_poll_is_countable || return 0

	local key
	key=$(printf '%s' "$stripped" | tr -s '[:space:]' ' ')
	key="${key# }"
	key="${key% }"

	local ledger occ
	ledger=$(_loom_ledger_file "polls" "$agent_id" "$fallback_sid")
	occ=$(($(_loom_polls_count "$ledger" "$key") + 1))

	if ((occ >= 5)); then
		loom_hook_deny_or_warn "\`${key}\` has run ${occ} times this session - stop polling and act on what you already know instead of checking again."
	elif ((occ >= 3)); then
		loom_hook_note_warn "\`${key}\` has run ${occ} times this session - act on what you already know instead of checking again."
	fi
	_loom_ledger_append "$ledger" "$key"
	return 0
}

# --- Rule 3: Bash-side file reads (Task C rules 1-3, reused verbatim) ---

# _loom_read_bound_for_head_tail <base> <arg>... - echo "<kind> <lines>" for
# a head/tail invocation. A byte count (-c/-c<N>/--bytes...) or a follow flag
# (-f/--follow) is "skip" - out of scope for a LINE-count check entirely,
# not an unbounded "full" read: `head -c 200 <huge file>` is the least
# wasteful read possible, and `tail -f` never terminates.
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

# _loom_read_bound_for_sed <arg>... - echo "<kind> <lines>" for a
# `sed -n '<a>,<b>p'` invocation. A range ending at `$` (`<a>,$p`) reads to
# the last line - "full", not a bounded range, or `sed -n '1,$p' bigfile`
# would escape the large-file rule entirely. Any other sed usage is "skip" -
# sed can rewrite text in place without ever printing the whole file.
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

# _loom_bash_read_check <cmd-idx> <base> <agent-id> <fallback-sid> - run
# loom_read_discipline_check against every existing-regular-file path
# argument of the cat/head/tail/sed segment starting at LOOM_TOKENS[cmd-idx].
# Stops at the first hard deny - the first path that would deny decides.
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

	# A "full" kind's line count is PATH-specific (`cat a b` names two files),
	# so it is computed once per file here via _loom_read_full_lines, not by
	# the caller.
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

# loom_bash_reads_scan <agent-id> <fallback-sid> - walk LOOM_TOKENS and run
# _loom_bash_read_check on every cat/head/tail/sed command segment.
loom_bash_reads_scan() {
	local agent_id="$1" fallback_sid="$2"
	local n=${#LOOM_TOKENS[@]} i=0 at_cmd_pos=1 j base
	while ((i < n)); do
		if [[ "${LOOM_TOKENS[$i]}" == "%%SEP%%" ]]; then
			at_cmd_pos=1
			i=$((i + 1))
			continue
		fi
		if [[ $at_cmd_pos -eq 1 ]] && j=$(loom_tokens_command_word_index "$i"); then
			base="${LOOM_TOKENS[$j]##*/}"
			case "$base" in
			cat | head | tail | sed) _loom_bash_read_check "$j" "$base" "$agent_id" "$fallback_sid" ;;
			esac
		fi
		at_cmd_pos=0
		i=$((i + 1))
	done
	return 0
}

# --- Rule 4: pathless `git show`/`git diff` -----------------------------

# _loom_git_arg_names_path <token> - Return 0 when <token> (an argv word of
# a `git show`/`git diff` segment) names a path rather than a revision: it
# exists on disk, or contains a `/` or a `:` selector (`HEAD:src/foo.rs`). A
# revision RANGE (`main..HEAD`, `origin/main..HEAD`) is excluded via the
# `..` check even with a `/` on one side - never a path. A bare flag or sha
# falls through to "not a path", leaving the segment pathless.
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

# _loom_git_segment_is_pathless_show_diff <cmd-word-idx> - Return 0 when the
# git segment beginning at LOOM_TOKENS[cmd-word-idx] is a `show`/`diff` with
# no `--`/--stat/--name-only/--name-status AND no argument
# _loom_git_arg_names_path recognizes as a path (`git diff src/main.rs`,
# `git show HEAD:src/foo.rs` already scope the read - NOT pathless).
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

# _loom_git_segment_text <cmd-word-idx> - echo the space-joined tokens of
# the segment starting at LOOM_TOKENS[cmd-word-idx], to name it in a warning.
_loom_git_segment_text() {
	local j="$1" n=${#LOOM_TOKENS[@]} i="$1" out=""
	while ((i < n)) && [[ "${LOOM_TOKENS[$i]}" != "%%SEP%%" ]]; do
		[[ -z "$out" ]] && out="${LOOM_TOKENS[$i]}" || out="${out} ${LOOM_TOKENS[$i]}"
		i=$((i + 1))
	done
	printf '%s' "$out"
}

# _loom_poll_rule_git_pathless - warn ONCE, naming the command, on the FIRST
# pathless `git show`/`git diff` segment - per-segment used to duplicate the
# sentence when a call chains more than one (`git diff && git show`).
_loom_poll_rule_git_pathless() {
	local n=${#LOOM_TOKENS[@]} i=0 at_cmd_pos=1 j base
	while ((i < n)); do
		if [[ "${LOOM_TOKENS[$i]}" == "%%SEP%%" ]]; then
			at_cmd_pos=1
			i=$((i + 1))
			continue
		fi
		if [[ $at_cmd_pos -eq 1 ]] && j=$(loom_tokens_command_word_index "$i"); then
			base="${LOOM_TOKENS[$j]##*/}"
			if [[ "$base" == "git" ]] && _loom_git_segment_is_pathless_show_diff "$j"; then
				loom_hook_note_warn "\`$(_loom_git_segment_text "$j")\` ran with no path - run --stat first, then per-file -- <path>"
				return 0
			fi
		fi
		at_cmd_pos=0
		i=$((i + 1))
	done
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

# An unterminated quote leaves LOOM_TOKENS untrustworthy - allow with no
# rule evaluated, rather than trust a partial token list (prefer-modern-
# tools.sh does the same).
if loom_tokenize_command "$STRIPPED"; then
	_loom_poll_rule_sleep
	_loom_poll_rule_repeat "$STRIPPED" "$AGENT_ID" "${PAYLOAD_SID:-unknown}"
	loom_bash_reads_scan "$AGENT_ID" "${PAYLOAD_SID:-unknown}"
	_loom_poll_rule_git_pathless
fi

loom_hook_emit_warns
exit 0
