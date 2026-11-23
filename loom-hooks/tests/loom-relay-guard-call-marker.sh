#!/usr/bin/env bash
# loom-relay-guard.sh writes the call marker the relay hook needs to admit a
# control ticket: a call that runs a control kind and passes the guard's checks
# gets an empty $LOOM_SCRATCH_DIR/.pre-<tool_use_id>. A blocked call, a memory
# call and a tool_use_id that is not a plain token get none, and a control call
# removes markers older than an hour.
# Every case runs under `env -i`: an inherited LOOM_WORK_DIR or LOOM_STAGE_ID
# would write the live block ledger.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
HOOK="$ROOT/loom-hooks/loom-relay-guard.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-relaymark.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
SCRATCH="$TMP/scratch"
mkdir -p "$SCRATCH" "$TMP/work"

FAILED=0

# run_guard <command> <tool_use_id> - Print the guard's exit status.
run_guard() {
	local rc=0 payload
	payload=$(jq -nc --arg c "$1" --arg id "$2" \
		'{tool_name: "Bash", tool_use_id: $id, tool_input: {command: $c}}')
	# A here-string, not a pipe: a guard that exits before reading stdin
	# would fail the writer under pipefail.
	(cd "$TMP" && env -i HOME="$TMP" PATH="$PATH" \
		LOOM_SESSION_ID=session-1 LOOM_SCRATCH_DIR="$SCRATCH" \
		LOOM_WORK_DIR="$TMP/work" LOOM_STAGE_ID=stage-a \
		bash "$HOOK" <<<"$payload" 2>"$TMP/stderr" >/dev/null) || rc=$?
	printf '%s' "$rc"
}

expect_exit() {
	local want=$1 got
	got=$(run_guard "$2" "$3")
	if [[ "$got" != "$want" ]]; then
		echo "FAIL: [$2] id=$3: expected exit $want, got $got: $(cat "$TMP/stderr")" >&2
		FAILED=1
	fi
}

expect_marker() {
	if [[ ! -f "$SCRATCH/$1" || -s "$SCRATCH/$1" ]]; then
		echo "FAIL: $1 is not an empty regular file in the scratch directory" >&2
		FAILED=1
	fi
}

expect_no_marker() {
	if [[ -e "$SCRATCH/$1" ]]; then
		echo "FAIL: $1 exists but must not ($2)" >&2
		FAILED=1
	fi
}

# A passing control call leaves its marker.
expect_exit 0 'loom stage block s1 "why"' toolu_A
expect_marker .pre-toolu_A

# A control kind the guard does not guard (a lead's `loom stage complete`) still
# gets one, as does `loom subagents start`.
expect_exit 0 'loom stage complete s1' toolu_C
expect_marker .pre-toolu_C

# memory is swept from disk and needs no marker.
expect_exit 0 'loom memory note x' toolu_M
expect_no_marker .pre-toolu_M "a memory call"

# An id that is not a plain token gets no file anywhere.
expect_exit 0 'loom stage block s1 why' '../evil'
if [[ -n "$(find "$TMP" -name '*evil*')" ]]; then
	echo "FAIL: a tool_use_id with a path separator created a file" >&2
	FAILED=1
fi
expect_exit 0 'loom stage block s1 why' ''
expect_exit 0 'loom stage block s1 why' "$(printf 'a%.0s' $(seq 1 129))"
if [[ -n "$(find "$SCRATCH" -name '.pre-aaaa*')" ]]; then
	echo "FAIL: a 129-character tool_use_id got a marker" >&2
	FAILED=1
fi

# A blocked call leaves no marker.
expect_exit 2 'loom stage block s1 why > out.txt' toolu_B
expect_no_marker .pre-toolu_B "the call was blocked"

# A control call sweeps stale markers and keeps fresh ones.
touch "$SCRATCH/.pre-recent"
touch -t 202001010000 "$SCRATCH/.pre-old"
expect_exit 0 'loom stage block s1 why' toolu_D
expect_no_marker .pre-old "older than an hour"
expect_marker .pre-recent
expect_marker .pre-toolu_D

if [[ "$FAILED" != 0 ]]; then
	exit 1
fi
echo "ok: loom-relay-guard call marker"
