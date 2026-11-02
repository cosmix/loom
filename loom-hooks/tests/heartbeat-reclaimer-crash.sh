#!/usr/bin/env bash
# A reclaimer killed mid-recovery cannot strand the lock, and the lock needs no
# loom binary, flock(1) or state beyond the lock directory itself.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
COMMON="$ROOT/loom-hooks/_common.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/heartbeat-crash.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
lock="$TMP/locks/lock"
mkdir -p "$lock/owner.999999.1.0"

# SIGKILL the reclaimer after it retired the dead owner, before it removes the
# directory. The remaining empty directory is itself recoverable state.
bash -c '
    source "$1"; lock="$2"; marker="$3"
    rmdir() {
        if [[ "$1" == "$lock" ]]; then
            printf "%s\n" "$$" >"$marker"
            builtin kill -STOP "$$"
        fi
        command rmdir "$@"
    }
    loom_heartbeat_lock_recover_if_abandoned "$lock"
' _ "$COMMON" "$lock" "$TMP/pid" &
reclaimer=$!
for ((i=0; i<400; i++)); do [[ -s "$TMP/pid" ]] && break; sleep 0.01; done
[[ -s "$TMP/pid" ]] || { echo 'FAIL: reclaimer never reached the directory removal'; exit 1; }
[[ -d "$lock" && -z "$(compgen -G "$lock/owner.*" || true)" ]] || {
    echo 'FAIL: reclaimer did not retire exactly the dead owner first'; exit 1
}
kill -KILL "$(<"$TMP/pid")"
wait "$reclaimer" 2>/dev/null || true
LOOM_HEARTBEAT_LOCK_STALE_SECONDS=30 bash -c '
    source "$1"; ! loom_heartbeat_lock_recover_if_abandoned "$2"
' _ "$COMMON" "$lock" || { echo 'FAIL: a fresh empty directory was reclaimed'; exit 1; }
touch -t 200001010000 "$lock"
LOOM_HEARTBEAT_LOCK_STALE_SECONDS=1 bash -c '
    source "$1"; loom_heartbeat_lock_acquire "$2" && loom_heartbeat_lock_release "$2"
' _ "$COMMON" "$lock" || { echo 'FAIL: a later writer could not acquire'; exit 1; }
[[ ! -e "$lock" ]] || { echo 'FAIL: release left the lock directory behind'; exit 1; }

# No loom binary or flock(1) reachable: the protocol runs on Bash and coreutils,
# and leaves nothing behind in the directory it serializes.
TOOLS="$TMP/tools"
mkdir "$TOOLS"
# bash itself: under LOOM_HOOK_TEST_BSD the date and stat here are bash shims.
for tool in bash date dirname mkdir rmdir sleep stat; do
    ln -s "$(command -v "$tool")" "$TOOLS/$tool"
done
BASH_BIN=$(command -v bash)
PATH="$TOOLS" LOOM_BIN=/missing/loom "$BASH_BIN" -c '
    source "$1"; loom_heartbeat_lock_acquire "$2" || exit 1; loom_heartbeat_lock_release "$2"
' _ "$COMMON" "$lock" || { echo 'FAIL: the lock needs a tool beyond Bash and coreutils'; exit 1; }
LEFT=$(compgen -G "$TMP/locks/*" || true)
[[ -z "$LEFT" ]] || { echo "FAIL: the lock left state behind: $LEFT"; exit 1; }

# The same restricted PATH under contention: a second writer stays out while the
# first holds the lock, and enters once it lets go.
PATH="$TOOLS" LOOM_BIN=/missing/loom "$BASH_BIN" -c '
    source "$1"; loom_heartbeat_lock_acquire "$2" || exit 1
    : >"$3/holder-entered"
    while [[ ! -e "$3/holder-release" ]]; do sleep 0.01; done
    loom_heartbeat_lock_release "$2"
' _ "$COMMON" "$lock" "$TMP" &
holder=$!
for ((i=0; i<400; i++)); do [[ -e "$TMP/holder-entered" ]] && break; sleep 0.01; done
[[ -e "$TMP/holder-entered" ]] || { echo 'FAIL: the restricted holder never entered'; exit 1; }
if PATH="$TOOLS" LOOM_BIN=/missing/loom "$BASH_BIN" -c '
    source "$1"; loom_heartbeat_lock_acquire "$2"
' _ "$COMMON" "$lock"; then
    : >"$TMP/holder-release"
    wait "$holder" || true
    echo 'FAIL: a restricted writer entered beside a live holder'
    exit 1
fi
: >"$TMP/holder-release"
wait "$holder"
PATH="$TOOLS" LOOM_BIN=/missing/loom "$BASH_BIN" -c '
    source "$1"; loom_heartbeat_lock_acquire "$2" && loom_heartbeat_lock_release "$2"
' _ "$COMMON" "$lock" || { echo 'FAIL: a restricted writer could not enter after release'; exit 1; }

# PID reuse needs a start time. With ps unreachable or denied, as in a sandbox
# that withholds process information, a live PID keeps winning: an owner stamped
# before that process existed stays put rather than risking a live writer.
sleep 30 &
live=$!
trap 'kill "$live" 2>/dev/null || true; rm -rf "$TMP"' EXIT
reused="$lock/owner.$live.1.0"
mkdir -p "$reused"
PATH="$TOOLS" LOOM_HEARTBEAT_LOCK_STALE_SECONDS=1 "$BASH_BIN" -c '
    source "$1"; ! loom_heartbeat_lock_recover_if_abandoned "$2"
' _ "$COMMON" "$lock" || { echo 'FAIL: reclaimed a live PID with no start time to compare'; exit 1; }
[[ -d "$reused" ]] || { echo 'FAIL: removed a live PID owner with no start time to compare'; exit 1; }
rmdir "$reused" "$lock"

# The elapsed-time forms POSIX and BSD ps print, against one reader. Only the
# start this implies decides: ten days up, stamped five days ago, is the holder.
PS_SHIMS="$TMP/ps-shims"
mkdir "$PS_SHIMS"
ps_says() {
    printf '#!/bin/sh\nprintf "%%s\\n" %q\n' "$1" >"$PS_SHIMS/ps"
    chmod +x "$PS_SHIMS/ps"
}
owner_at() {
    local age="$1" stamped
    stamped=$(($(date +%s) - age))
    mkdir -p "$lock/owner.$live.$stamped.0"
}
recovered() {
    PATH="$PS_SHIMS:$PATH" LOOM_HEARTBEAT_LOCK_STALE_SECONDS=1 bash -c '
        source "$1"; loom_heartbeat_lock_recover_if_abandoned "$2"
    ' _ "$COMMON" "$lock"
}
ps_says ' 10-00:00:00'
owner_at 432000
if recovered; then echo 'FAIL: a holder older than its stamp was reclaimed'; exit 1; fi
rm -rf "$lock"
ps_says '   01:05'
owner_at 7200
recovered || { echo 'FAIL: mm:ss form: a process younger than the stamp held the lock'; exit 1; }
[[ ! -e "$lock" ]] || { echo 'FAIL: mm:ss form left the lock'; exit 1; }
ps_says ' 01:00:09'
owner_at 7200
recovered || { echo 'FAIL: hh:mm:ss form: a process younger than the stamp held the lock'; exit 1; }
ps_says ' 01:59:58'
owner_at 7200
if recovered; then echo 'FAIL: a stamp within the clock slack of the start was reclaimed'; exit 1; fi
rm -rf "$lock"
ps_says 'garbage'
owner_at 7200
if recovered; then echo 'FAIL: unreadable ps output reclaimed a live PID'; exit 1; fi
rm -rf "$lock"
echo PASS
