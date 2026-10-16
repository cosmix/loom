#!/usr/bin/env bash
# Shared bounded command and stdin collection; Bash 3.2 compatible.
if [[ "${_LOOM_BOUNDED_LOADED:-}" == 1 ]]; then return 0; fi
_LOOM_BOUNDED_LOADED=1

# _loom_watchdog <pid> <seconds> - the timer behind loom_run_bounded's bash
# fallback: TERM <pid> once <seconds> pass, KILL it a second later if it is
# still there. A TERM aimed at the watchdog retires it at once: the trap kills
# its pending sleep (found through `jobs -p`, so even one forked a moment ago
# is caught) and exits, so no sleep outlives the call.
_loom_watchdog() {
	local pid="$1" seconds="$2"
	trap 'kill $(jobs -p) 2>/dev/null; exit 0' TERM
	sleep "$seconds" & wait $!
	kill -TERM "$pid" 2>/dev/null
	sleep 1 & wait $!
	kill -KILL "$pid" 2>/dev/null
}

# loom_run_bounded <seconds> <command...>
# Prefer GNU coreutils on macOS, then Linux timeout. Stock macOS has neither,
# so the fallback is a pure-bash watchdog and the call is never left
# unbounded: the command runs in the background beside _loom_watchdog, the
# caller waits for it, then retires the watchdog. Returns the command's exit
# status, 143 (TERM) or 137 (KILL) when the watchdog had to stop it, where
# timeout returns 124. The fallback signals only the direct child pid (GNU
# timeout signals the process group), so the command should not fork children
# that hold the caller's pipe. stdin reaches the command (`<&0`: a bare
# background command would get /dev/null), and the watchdog's output goes to
# /dev/null: holding the caller's stdout pipe open, it would make a surrounding
# $(...) block until its sleep ended.
# The watchdog runs as an explicit `( ... ) &` subshell: bash 3.2 runs the
# caller's EXIT trap in a bare `func &` or `cmd &` job that a signal reaches
# early, and a command that finishes at once makes the retiring TERM arrive
# early every time.
# Shared here so PreToolUse and PostToolUse receipt calls get the same bound.
loom_run_bounded() {
	local seconds="$1" pid="" watchdog="" status=0
	shift
	if command -v gtimeout &>/dev/null; then
		gtimeout -k 1 "$seconds" "$@"
	elif command -v timeout &>/dev/null; then
		timeout -k 1 "$seconds" "$@"
	else
		"$@" <&0 &
		pid=$!
		( _loom_watchdog "$pid" "$seconds" ) >/dev/null 2>&1 &
		watchdog=$!
		wait "$pid" 2>/dev/null || status=$?
		kill "$watchdog" 2>/dev/null || true
		wait "$watchdog" 2>/dev/null || true
		return "$status"
	fi
}

loom_read_input() {
	loom_run_bounded "${1:-1}" cat 2>/dev/null
}
