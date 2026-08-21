#!/usr/bin/env bash
# guarded-cargo.sh — run a command under a RAM watchdog.
#
# Runs the given command in its OWN process group (setsid), samples available
# system memory, and kills the entire group if headroom drops below the floor.
# Killing the group matters: cargo's memory is spent by its rustc children, so
# killing cargo alone would orphan the processes actually holding the pages.
#
# Also reports any `loom` process left running after the command exits — a
# detached child that outlives its parent is invisible to `cargo test`, which
# exits green regardless. That leak is what forced a reboot on 2026-08-21.
#
#   MIN_AVAIL_GB=32 ./guarded-cargo.sh cargo test --all-targets
#
# Exit codes: the command's own, or 97 if the watchdog fired.
set -uo pipefail

MIN_AVAIL_GB=${MIN_AVAIL_GB:-32}
POLL_SECS=${POLL_SECS:-2}

avail_gb() { free -g | awk '/^Mem:/ {print $7}'; }

start_avail=$(avail_gb)
echo "watchdog: floor ${MIN_AVAIL_GB}GB, available at start ${start_avail}GB" >&2

setsid "$@" &
cmd_pid=$!
low_water=$start_avail

while kill -0 "$cmd_pid" 2>/dev/null; do
	avail=$(avail_gb)
	[ -n "$avail" ] && [ "$avail" -lt "$low_water" ] && low_water=$avail
	if [ -n "$avail" ] && [ "$avail" -lt "$MIN_AVAIL_GB" ]; then
		echo "WATCHDOG TRIPPED: available ${avail}GB < floor ${MIN_AVAIL_GB}GB" >&2
		kill -9 -- -"$cmd_pid" 2>/dev/null
		wait "$cmd_pid" 2>/dev/null
		echo "watchdog: killed process group $cmd_pid" >&2
		exit 97
	fi
	sleep "$POLL_SECS"
done

wait "$cmd_pid"
status=$?

echo "watchdog: lowest available RAM seen ${low_water}GB (floor ${MIN_AVAIL_GB}GB)" >&2

# `pgrep -c` prints a count AND exits non-zero when that count is zero, so a
# `|| echo 0` fallback would concatenate a second line onto a valid "0".
leaked=$(pgrep -c -x loom 2>/dev/null)
leaked=${leaked:-0}
if [ "$leaked" -gt 0 ]; then
	echo "WARNING: ${leaked} 'loom' process(es) still running after exit:" >&2
	pgrep -a -x loom >&2
fi

exit "$status"
