#!/usr/bin/env bash
# loom_run_bounded (_read_ledger.sh) must bound a command even on a machine with
# neither gtimeout nor timeout, as stock macOS is: the bash watchdog stops a
# hung command, kills one that ignores TERM, leaves no sleep holding a
# surrounding $(...) open, never runs the caller's EXIT trap a second time, and
# still passes stdin, stdout and the exit status through. The guard below fails
# the test when a timeout binary is still visible; otherwise it would exercise
# whichever binary the host happens to have.
set -euo pipefail
# _read_ledger.sh runs `PATH="${LOOM_HOOK_PATH:-$PATH}"` when sourced, so a live
# stage's LOOM_HOOK_PATH would restore a real timeout over the PATH built below.
unset LOOM_WORK_DIR LOOM_SESSION_ID LOOM_STAGE_ID LOOM_SESSION_TYPE LOOM_HOOK_PATH

HOOKS_DIR="$(cd "$(dirname "$0")/.." && pwd)"
source "$HOOKS_DIR/tests/_path_without.sh"

NO_TIMEOUT_DIR=$(path_without gtimeout timeout)
trap 'rm -rf -- "$NO_TIMEOUT_DIR"' EXIT
PATH="$NO_TIMEOUT_DIR"
if command -v gtimeout >/dev/null 2>&1 || command -v timeout >/dev/null 2>&1; then
	echo "FAIL: a timeout binary is still on PATH, the watchdog fallback would go untested"
	exit 90
fi
source "$HOOKS_DIR/_read_ledger.sh"

fail() {
	echo "FAIL: $1"
	exit 1
}

# assert_elapsed_within <start-seconds> <max-seconds> <label>
assert_elapsed_within() {
	local elapsed=$((SECONDS - $1))
	((elapsed <= $2)) || fail "$3 took ${elapsed}s, the bound allows ${2}s"
}

status=0
out=$(loom_run_bounded 2 bash -c 'echo hi; echo err >&2; exit 3' 2>/dev/null) || status=$?
[[ "$out" == "hi" && "$status" == "3" ]] || fail "stdout/status not passed through: '$out' / $status"

out=$(printf 'abc' | loom_run_bounded 2 cat)
[[ "$out" == "abc" ]] || fail "stdin did not reach the command: '$out'"

start=$SECONDS
status=0
out=$(loom_run_bounded 1 sleep 30) || status=$?
assert_elapsed_within "$start" 4 "a hung command inside \$(...)"
[[ "$status" == "143" ]] || fail "a hung command should end TERM-killed (143), got $status"

# An ignored signal is inherited across exec, so the sleep below survives TERM.
start=$SECONDS
status=0
loom_run_bounded 1 bash -c 'trap "" TERM; exec sleep 30' || status=$?
assert_elapsed_within "$start" 5 "a command that ignores TERM"
[[ "$status" == "137" ]] || fail "a TERM-ignoring command should end KILLed (137), got $status"

# The caller's EXIT trap runs once, in the caller: bash 3.2 would also run it in
# a watchdog job that the caller's TERM reaches early, which a command that
# finishes at once (the repeated `true`) makes certain.
marker="$NO_TIMEOUT_DIR/exit-trap-marker"
bash -c 'source "$1"; trap "echo ran >>\"$2\"" EXIT
	for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do loom_run_bounded 5 true; done
	loom_run_bounded 1 sleep 30 || true' _ "$HOOKS_DIR/_read_ledger.sh" "$marker"
runs=$(wc -l <"$marker" | tr -d '[:space:]')
[[ "$runs" == "1" ]] || fail "the caller's EXIT trap ran $runs times, expected once"

start=$SECONDS
loom_run_bounded 5 true
assert_elapsed_within "$start" 2 "a command that finishes at once (the watchdog must not be waited for)"

echo "PASS"
