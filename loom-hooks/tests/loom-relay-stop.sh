#!/usr/bin/env bash
# loom-relay-stop.sh: a control relay ticket left in the scratch directory blocks
# the stop once with a JSON decision naming its recovery; swept kinds, a marked
# ticket and a malformed file do not produce ticket warnings. A stage idle stop
# can also relay a validated block from the CLI. Every case runs under `env -i`.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
HOOK="$ROOT/loom-hooks/loom-relay-stop.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-relaystop.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
SCRATCH="$TMP/scratch"

FAILED=0
fail() {
	echo "FAIL: $*" >&2
	FAILED=1
}

ID_A=0123456789abcdef0123456789abcdef
ID_B=fedcba9876543210fedcba9876543210

# reset - An empty scratch directory.
reset() {
	rm -rf "$SCRATCH"
	mkdir -p "$SCRATCH"
}

# ticket <id> <kind> - Write a ticket file the way the CLI does.
ticket() {
	jq -nc --arg id "$1" --arg kind "$2" \
		'{v: 1, id: $id, kind: $kind, created_at: "2026-11-17T00:00:00Z", payload: {}}' >"$SCRATCH/$1.req"
}

# stop [active|inactive] [env assignments...] - Run the hook and print its stdout.
stop() {
	local active=false rc=0 out
	[[ "${1:-}" == active ]] && active=true
	[[ $# -eq 0 ]] || shift
	out=$(printf '{"hook_event_name":"Stop","stop_hook_active":%s}' "$active" |
		env -i HOME="$TMP" PATH="$PATH" LOOM_SESSION_ID=session-1 LOOM_SCRATCH_DIR="$SCRATCH" \
			"$@" bash "$HOOK" 2>"$TMP/stderr") || rc=$?
	[[ $rc -eq 0 ]] || fail "hook exited $rc: $(cat "$TMP/stderr")"
	printf '%s' "$out"
	return "$rc"
}

# expect_silent <label> <output>
expect_silent() {
	[[ -z "$2" ]] || fail "$1: expected no output, got: $2"
}

# decision <output> - The decision field of the hook's JSON, or <not-json>.
decision() {
	printf '%s' "$1" | jq -r '.decision' 2>/dev/null || printf '<not-json>'
}

# A leftover file-dispute blocks once; the reason checks status before any rerun.
reset
ticket "$ID_A" file-dispute
out=$(stop)
[[ "$(decision "$out")" == block ]] || fail "file-dispute ticket: expected a block, got: $out"
reason=$(printf '%s' "$out" | jq -r '.reason' 2>/dev/null || true)
case "$reason" in
*"file-dispute relay ticket 01234567 is still on disk"*) ;;
*) fail "reason does not name kind and short id: $reason" ;;
esac
status_at=${reason%%"loom request status $ID_A"*}
rerun_at=${reason%%"rerun it alone, in the foreground, with stdout unfiltered"*}
if [[ "$status_at" == "$reason" || "$rerun_at" == "$reason" || ${#status_at} -ge ${#rerun_at} ]]; then
	fail "reason must name 'loom request status <full id>' before the rerun sentence: $reason"
fi
expect_silent "second stop after the warning" "$(stop)"

# Two control tickets: one block naming both, then silence.
reset
ticket "$ID_A" block
ticket "$ID_B" commit
out=$(stop)
reason=$(printf '%s' "$out" | jq -r '.reason' 2>/dev/null || true)
[[ "$(decision "$out")" == block && "$reason" == *"$ID_A"* && "$reason" == *"$ID_B"* ]] ||
	fail "two tickets: expected one block naming both, got: $out"
[[ "$(printf '%s' "$out" | wc -l | tr -d ' ')" == 0 ]] || fail "two tickets: expected one JSON object"
expect_silent "second stop after two warnings" "$(stop)"

# Swept kinds never block.
for kind in memory telemetry check; do
	reset
	ticket "$ID_A" "$kind"
	expect_silent "$kind ticket" "$(stop)"
done

# stop_hook_active: no block and no marker, so the next plain stop still warns once.
reset
ticket "$ID_A" verdict
expect_silent "stop_hook_active" "$(stop active)"
[[ "$(decision "$(stop)")" == block ]] || fail "stop_hook_active must not consume the warning"

# A complete ticket points at the Host job line instead of a plain rerun.
reset
ticket "$ID_A" complete
reason=$(stop | jq -r '.reason' 2>/dev/null || true)
[[ "$reason" == *'follow its `Host job:` line'* ]] || fail "complete ticket: reason lacks the Host job line: $reason"
[[ "$reason" != *"with stdout unfiltered"* ]] || fail "complete ticket: reason must not use the plain rerun sentence"

# A malformed ticket, a non-object ticket and a symlinked ticket never block or fail.
reset
printf 'not json' >"$SCRATCH/$ID_A.req"
printf '[1,2]' >"$SCRATCH/$ID_B.req"
real="$TMP/real.tmp"
jq -nc '{v: 1, id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", kind: "block", payload: {}}' >"$real"
ln -s "$real" "$SCRATCH/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.req"
expect_silent "malformed tickets" "$(stop)"

# Without the session variables the hook does nothing.
reset
ticket "$ID_A" block
out=$(printf '{"stop_hook_active":false}' | env -i HOME="$TMP" PATH="$PATH" bash "$HOOK" 2>&1) || fail "env gate unset: exit $?"
expect_silent "env gate unset" "$out"
out=$(printf '{"stop_hook_active":false}' | env -i HOME="$TMP" PATH="$PATH" LOOM_SESSION_ID=session-1 bash "$HOOK" 2>&1) ||
	fail "scratch unset: exit $?"
expect_silent "scratch gate unset" "$out"

STUB="$TMP/loom-stub"
CALLS="$TMP/calls"
STDIN_LOG="$TMP/stub-stdin"
REPLY="$TMP/idle-stop-reply"
cat >"$STUB" <<'STUB_EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$CALLS"
cat >"$STDIN_LOG"
cat "$REPLY"
exit "${STUB_EXIT:-0}"
STUB_EOF
chmod +x "$STUB"

run_idle_stop() {
	local state=$1
	shift
	rm -f "$CALLS" "$STDIN_LOG"
	out=$(stop "$state" LOOM_BIN="$STUB" CALLS="$CALLS" STDIN_LOG="$STDIN_LOG" REPLY="$REPLY" "$@") ||
		fail "idle-stop: expected the hook to exit 0"
}

# A stage stop relays one block, preserving the reason and forwarding the payload.
reset
expected=$(jq -nc '{decision: "block", reason: "Resume the \"stage\".\n"}')
printf ' %s\n' "$expected" >"$REPLY"
run_idle_stop inactive LOOM_STAGE_ID=stage-1
[[ "$out" == "$expected" ]] || fail "idle-stop: expected one normalized block, got: $out"
[[ -f "$CALLS" && "$(cat "$CALLS")" == 'hook idle-stop' ]] ||
	fail "idle-stop: expected one call to loom hook idle-stop"
[[ -f "$STDIN_LOG" && "$(cat "$STDIN_LOG")" == '{"hook_event_name":"Stop","stop_hook_active":false}' ]] ||
	fail "idle-stop: expected the Stop payload on stdin"

# Malformed output, empty reasons, multiple objects and unsuccessful calls stay silent.
printf 'not json\n' >"$REPLY"
run_idle_stop inactive LOOM_STAGE_ID=stage-1
expect_silent "idle-stop invalid JSON" "$out"

printf '%s\n' '{"decision":"block","reason":""}' >"$REPLY"
run_idle_stop inactive LOOM_STAGE_ID=stage-1
expect_silent "idle-stop empty reason" "$out"

printf '%s\n' "$expected" "$expected" >"$REPLY"
run_idle_stop inactive LOOM_STAGE_ID=stage-1
expect_silent "idle-stop multiple objects" "$out"

printf '%s\n' "$expected" >"$REPLY"
run_idle_stop inactive LOOM_STAGE_ID=stage-1 STUB_EXIT=3
expect_silent "idle-stop nonzero exit with a valid reply" "$out"

# A ticket takes precedence over the delegate.
ticket "$ID_A" block
run_idle_stop inactive LOOM_STAGE_ID=stage-1
reason=$(printf '%s' "$out" | jq -r '.reason' 2>/dev/null || true)
[[ "$(decision "$out")" == block && "$reason" == *"block relay ticket 01234567 is still on disk"* ]] ||
	fail "idle-stop ticket precedence: expected a ticket block, got: $out"
[[ ! -e "$CALLS" ]] || fail "idle-stop: called with a stranded control ticket"

# An active Stop hook and a session without a stage both skip the delegate.
reset
run_idle_stop active LOOM_STAGE_ID=stage-1
expect_silent "idle-stop stop_hook_active" "$out"
[[ ! -e "$CALLS" ]] || fail "idle-stop: called with stop_hook_active"

run_idle_stop inactive
expect_silent "idle-stop stage unset" "$out"
[[ ! -e "$CALLS" ]] || fail "idle-stop: called without LOOM_STAGE_ID"

[[ $FAILED -eq 0 ]] || exit 1
echo "loom-relay-stop: ok"
