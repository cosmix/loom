#!/usr/bin/env bash
# A creator paused between mkdir and owner publication can lose its empty
# directory to a reclaimer. It must never share the critical section.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
COMMON="$ROOT/loom-hooks/_common.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/heartbeat-abort.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
wait_for() {
    local i
    for ((i=0; i<400; i++)); do [[ -e "$1" ]] && return; sleep 0.01; done
    echo "FAIL: missing $1"; exit 1
}
# Pauses after the first mkdir of the lock directory; marks each publication.
start_creator() {
    bash -c '
        source "$1"; lock="$2"; tmp="$3"
        mkdir() {
            command mkdir "$@" || return
            if [[ "${@: -1}" == "$lock" && ! -e "$tmp/creator-made-directory" ]]; then
                : >"$tmp/creator-made-directory"
                while [[ ! -e "$tmp/creator-go" ]]; do sleep 0.01; done
            elif [[ "${@: -1}" == "$lock"/owner.* ]]; then
                : >"$tmp/creator-published"
            fi
        }
        rmdir() {
            [[ "$1" == "$lock"/owner.* ]] && : >"$tmp/creator-backed-off"
            command rmdir "$@"
        }
        if loom_heartbeat_lock_acquire "$lock"; then
            [[ -e "$tmp/successor-released" ]] && : >"$tmp/creator-entered-after-release"
            : >"$tmp/creator-entered"
            loom_heartbeat_lock_release "$lock"
        fi
    ' _ "$COMMON" "$1" "$2" &
}
reclaim_empty() {
    touch -t 200001010000 "$1"
    LOOM_HEARTBEAT_LOCK_STALE_SECONDS=1 bash -c 'source "$1"; loom_heartbeat_lock_recover_if_abandoned "$2"' _ "$COMMON" "$1"
    [[ ! -e "$1" ]] || { echo 'FAIL: abandoned empty directory was not reclaimed'; exit 1; }
}

# The successor holds the replacement directory when the creator resumes: the
# creator's owner lands beside the successor's, and the creator backs off.
BESIDE="$TMP/beside"; mkdir "$BESIDE"; lock="$BESIDE/lock"
start_creator "$lock" "$BESIDE"
creator=$!
wait_for "$BESIDE/creator-made-directory"
reclaim_empty "$lock"
bash -c '
    source "$1"; loom_heartbeat_lock_acquire "$2" || exit 1
    : >"$3/successor-entered"
    while [[ ! -e "$3/successor-go" ]]; do sleep 0.01; done
    : >"$3/successor-released"
    loom_heartbeat_lock_release "$2"
' _ "$COMMON" "$lock" "$BESIDE" &
successor=$!
wait_for "$BESIDE/successor-entered"
successor_owner=$(compgen -G "$lock/owner.*")
: >"$BESIDE/creator-go"
wait_for "$BESIDE/creator-published"
wait_for "$BESIDE/creator-backed-off"
sleep 0.1
[[ ! -e "$BESIDE/creator-entered" ]] || { echo 'FAIL: creator entered a successor generation'; exit 1; }
[[ "$(compgen -G "$lock/owner.*")" == "$successor_owner" ]] || {
    echo 'FAIL: successor ownership changed while the creator backed off'; exit 1
}
: >"$BESIDE/successor-go"
wait "$successor"
wait "$creator"
if [[ -e "$BESIDE/creator-entered" && ! -e "$BESIDE/creator-entered-after-release" ]]; then
    echo 'FAIL: creator entered before the successor released'; exit 1
fi
[[ ! -e "$lock" ]] || { echo 'FAIL: lock directory outlived its last owner'; exit 1; }

# With no successor the creator's publication fails, it retries, and it enters
# its own fresh generation.
ALONE="$TMP/alone"; mkdir "$ALONE"; lock="$ALONE/lock"
start_creator "$lock" "$ALONE"
creator=$!
wait_for "$ALONE/creator-made-directory"
reclaim_empty "$lock"
: >"$ALONE/creator-go"
wait "$creator"
[[ -e "$ALONE/creator-entered" ]] || { echo 'FAIL: creator did not retry into a fresh generation'; exit 1; }
[[ ! -e "$lock" ]] || { echo 'FAIL: lock directory outlived its last owner'; exit 1; }
echo PASS
