#!/usr/bin/env bash
# Shared bounded command and stdin collection; Bash 3.2 compatible.
if [[ "${_LOOM_BOUNDED_LOADED:-}" == 1 ]]; then return 0; fi
_LOOM_BOUNDED_LOADED=1

# _loom_watchdog <seconds> - the timer behind loom_run_bounded's bash
# fallback. Its stdin is the pipe on which the starter writes the command's
# pid, then, once the command has ended, its exit status. With no status after
# <seconds> it sends TERM to the pid, and KILL a second later, then returns the
# status the starter reports for the stopped command. `read -t` keeps time, so
# no sleep process exists to outlive the call. Every path returns only after
# the status line (or EOF, if the starter died) has arrived: a watchdog gone
# before the starter's write would turn the write into a SIGPIPE. The
# `|| true` guards keep errexit, which a `set -e` caller passes down, from
# ending the watchdog when the pid has already exited.
_loom_watchdog() {
	local pid="" line=""
	read -r pid || return 1
	read -r -t "$1" line && return "${line:-1}"
	kill -TERM "$pid" 2>/dev/null || true
	read -r -t 1 line && return "${line:-1}"
	kill -KILL "$pid" 2>/dev/null || true
	read -r line || true
	return "${line:-137}"
}

# loom_run_bounded <seconds> <command...>
# Prefer GNU coreutils on macOS, then Linux timeout. Stock macOS has neither,
# so the fallback is a pure-bash watchdog and the call is never left
# unbounded. A starter (the left side of a pipe) runs the command in the
# background, writes its pid to the pipe, waits for it and writes its exit
# status; _loom_watchdog reads both from the other end. The pipeline's status
# is the watchdog's: the command's exit status, or 143 (TERM) or 137 (KILL)
# when the watchdog had to stop it, where timeout returns 124. The fallback
# signals only the direct child pid (GNU timeout signals the process group),
# so the command should not fork children that hold the caller's pipes.
# The fallback opens no /dev/fd path: a process substitution would open /dev/fd/<n>,
# which a confined host (a macOS host job's Seatbelt profile) denies, leaving
# the command unrun and its output empty. A plain pipe needs no path.
# fd 8 carries the caller's stdout past the pipe to the command, so the command
# never holds the watchdog's pipe and the command's output never enters it;
# stdin reaches the command (`<&0`: a bare background command would get
# /dev/null). The watchdog writes to /dev/null and closes fd 8: holding the
# caller's stdout, it would make a surrounding $(...) wait for it.
# The caller never signals the watchdog or the starter. A forked bash keeps the
# caller's EXIT trap and signal handlers until it resets them, and a signal
# that lands in that window runs the caller's EXIT trap in the child; a
# command that ends at once would make a retiring signal land there often. A
# line on a pipe has no such window. Lines rather than EOF mark each step
# because bash 3.2's `read -t` returns 1 on a timeout and on EOF alike.
# Shared here so every hook delegate gets the same bound.
loom_run_bounded() {
	local seconds="$1"
	shift
	if command -v gtimeout &>/dev/null; then
		gtimeout -k 1 "$seconds" "$@"
	elif command -v timeout &>/dev/null; then
		timeout -k 1 "$seconds" "$@"
	else
		{
			_loom_bounded_start "$@" | _loom_watchdog "$seconds" >/dev/null 2>&1 8>&-
		} 8>&1
	fi
}

# _loom_bounded_start <command...> - loom_run_bounded's starter: runs the
# command on the caller's stdin and fd 8, and writes its pid and then its exit
# status, one line each, to stdout (the watchdog's pipe).
_loom_bounded_start() {
	local pid="" status=0
	"$@" <&0 >&8 8>&- &
	pid=$!
	printf '%s\n' "$pid"
	wait "$pid" 2>/dev/null || status=$?
	printf '%s\n' "$status"
}

loom_read_input() {
	loom_run_bounded "${1:-1}" cat 2>/dev/null
}

# loom_require_payload <hook-name> - blocking guards only: refuse (exit 2) when
# the global INPUT_JSON is not a JSON object with a non-empty string tool_name,
# or when jq is missing. Call it as a plain statement: inside $(...) the exit
# would end only the subshell.
loom_require_payload() {
	local guard="${1%.sh}"
	if command -v jq >/dev/null 2>&1 &&
		printf '%s' "${INPUT_JSON:-}" |
		jq -e 'type == "object" and (.tool_name | type == "string" and length > 0)' >/dev/null 2>&1; then
		return 0
	fi
	if declare -F loom_record_block >/dev/null 2>&1; then
		loom_record_block "$guard" bad-payload unknown ""
	fi
	printf '[%s/bad-payload] LOOM: BLOCKED - tool call metadata could not be parsed (the hook input was empty or not a JSON object with a tool_name). Retry the call.\n' "$guard" >&2 || true
	exit 2
}

# loom_read_payload_or_block <seconds> <hook-name> - reads stdin into
# INPUT_JSON, then loom_require_payload. Plain statement only, never $(...).
loom_read_payload_or_block() {
	INPUT_JSON=$(loom_read_input "$1" 2>/dev/null || true)
	loom_require_payload "$2"
}
