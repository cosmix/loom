#!/usr/bin/env bash
# Shared bounded command and stdin collection; Bash 3.2 compatible.
if [[ "${_LOOM_BOUNDED_LOADED:-}" == 1 ]]; then return 0; fi
_LOOM_BOUNDED_LOADED=1

# _loom_watchdog <pid> <seconds> - the timer behind loom_run_bounded's bash
# fallback. Its stdin is a pipe on which the caller writes one line once the
# command has ended. With no line after <seconds> it sends TERM to <pid>, and
# KILL a second later. `read -t` keeps time, so no sleep process exists to
# outlive the call. Every path returns only after that line (or EOF, if the
# caller died) has arrived: a watchdog gone before the caller's write would
# turn the write into a SIGPIPE that ends the caller. The `|| true` guards
# keep errexit, which a `set -e` caller passes down, from ending the watchdog
# when <pid> has already exited.
_loom_watchdog() {
	local line
	read -r -t "$2" line && return 0
	kill -TERM "$1" 2>/dev/null || true
	read -r -t 1 line && return 0
	kill -KILL "$1" 2>/dev/null || true
	read -r line || true
}

# loom_run_bounded <seconds> <command...>
# Prefer GNU coreutils on macOS, then Linux timeout. Stock macOS has neither,
# so the fallback is a pure-bash watchdog and the call is never left
# unbounded: the command runs in the background, _loom_watchdog runs as a
# process substitution reading a pipe the caller holds on fd 9, and the caller
# waits for the command, then writes one line to that pipe to retire the
# watchdog. Returns the command's exit status, 143 (TERM) or 137 (KILL) when
# the watchdog had to stop it, where timeout returns 124. The fallback signals
# only the direct child pid (GNU timeout signals the process group), so the
# command should not fork children that hold the caller's pipe. stdin reaches
# the command (`<&0`: a bare background command would get /dev/null), and the
# watchdog's output goes to /dev/null: holding the caller's stdout pipe open,
# it would make a surrounding $(...) block until it exited. The command starts
# before the pipe opens, so it never holds the watchdog's pipe.
# The caller never signals the watchdog. A forked bash keeps the caller's EXIT
# trap and signal handlers until it resets them, and a signal that lands in
# that window runs the caller's EXIT trap in the child; a command that ends at
# once would make a retiring signal land there often. A line on a pipe has no
# such window. The caller writes a line rather than closing the pipe because
# bash 3.2's `read -t` returns 1 on a timeout and on EOF alike.
# Shared here so every hook delegate gets the same bound.
loom_run_bounded() {
	local seconds="$1" pid="" status=0
	shift
	if command -v gtimeout &>/dev/null; then
		gtimeout -k 1 "$seconds" "$@"
	elif command -v timeout &>/dev/null; then
		timeout -k 1 "$seconds" "$@"
	else
		"$@" <&0 &
		pid=$!
		{
			wait "$pid" 2>/dev/null || status=$?
			printf '\n' >&9
		} 9> >(_loom_watchdog "$pid" "$seconds" >/dev/null 2>&1)
		return "$status"
	fi
}

loom_read_input() {
	loom_run_bounded "${1:-1}" cat 2>/dev/null
}
