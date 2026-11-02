#!/usr/bin/env bash
# Shared hook helpers; Bash 3.2 compatible.
if [[ "${_LOOM_HEARTBEAT_LOADED:-}" == 1 ]]; then return 0; fi
_LOOM_HEARTBEAT_LOADED=1

# loom_heartbeat_lock_acquire <lock-dir>
# loom_heartbeat_lock_release <lock-dir>
#
# Serialize every shell heartbeat and lifecycle-journal writer with mkdir and
# rmdir alone. Both are atomic on macOS and Linux; stock macOS Bash 3.2 has no
# flock(1). A held lock is <lock-dir>/owner.<pid>.<created>.<nonce>/. Recovery
# cannot cross generations, and needs no reclaimer serialization, because:
#
# - An owner name belongs to one acquisition. A reclaimer removes only the name
#   it examined, so a stale decision is a no-op against any later owner.
# - <lock-dir> is only ever removed by rmdir, which refuses while any owner is
#   published. A stale rmdir can hit only a creator that has not entered yet.
# - A creator enters only after publishing its owner and then scanning for any
#   other. If its empty directory was reclaimed, publication either fails or
#   lands beside a successor. Of two concurrent publishers at least one sees
#   the other, and it backs off; both backing off just costs a retry.
#
# A live PID is reclaimed only when it provably is not the owner: the owner read
# <created> after it started, so a process that started later is a reuse of the
# PID. Without a readable start time the PID wins, as it does for any live owner.
# An unparseable entry is never removed, so its lock stays held and writers skip
# their refresh: unknown state fails closed.
#
# LOOM_HEARTBEAT_LOCK_STALE_SECONDS exists chiefly for regression tests. Keep
# the production default deliberately longer than a normal hook invocation.
loom_heartbeat_lock_epoch() {
    local path="$1" epoch=""
    epoch=$(stat -f '%m' "$path" 2>/dev/null || true)
    if [[ ! "$epoch" =~ ^[0-9]+$ ]]; then
        epoch=$(stat -c '%Y' "$path" 2>/dev/null || true)
    fi
    [[ "$epoch" =~ ^[0-9]+$ ]] && printf '%s\n' "$epoch"
}

# _loom_heartbeat_pid_started_after <pid> <created> <now> - Whether <pid> started
# after <created> by more than the slack below. Elapsed time is the one start
# figure every `ps` prints alike ([[dd-]hh:]mm:ss, POSIX and BSD): `lstart` would
# need a date parser that differs between them. A sandbox can deny `ps`, and the
# restricted hook PATH can lack it; either answers no.
#
# The slack absorbs both figures' one-second rounding and a small clock step
# while the owner holds the lock. A larger forward step can misread a live owner
# that has also held the lock past the stale age. Revisit if a writer ever holds
# it for longer than a hook invocation.
_loom_heartbeat_pid_started_after() {
    local pid="$1" created="$2" now="$3" elapsed="" slack=5
    local pattern='^(([0-9]+)-)?(([0-9]+):)?([0-9]+):([0-9]+)$'
    elapsed=$(LC_ALL=C ps -o etime= -p "$pid" 2>/dev/null) || return 1
    elapsed="${elapsed//[[:space:]]/}"
    [[ "$elapsed" =~ $pattern ]] || return 1
    elapsed=$((10#${BASH_REMATCH[2]:-0} * 86400 + 10#${BASH_REMATCH[4]:-0} * 3600 + \
        10#${BASH_REMATCH[5]} * 60 + 10#${BASH_REMATCH[6]}))
    ((now - elapsed > 10#$created + slack))
}

# _loom_heartbeat_owner_is_abandoned <owner-dir> <now> <stale-after>
_loom_heartbeat_owner_is_abandoned() {
    local owner="$1" now="$2" stale_after="$3" name="${1##*/}" pid="" created=""
    local pattern='^owner\.([0-9]+)\.([0-9]+)\.[0-9]+$'
    [[ -d "$owner" && ! -L "$owner" && "$name" =~ $pattern ]] || return 1
    pid="${BASH_REMATCH[1]}"
    created="${BASH_REMATCH[2]}"
    ((now - 10#$created >= stale_after)) || return 1
    kill -0 "$pid" 2>/dev/null || return 0
    _loom_heartbeat_pid_started_after "$pid" "$created" "$now"
}

loom_heartbeat_lock_recover_if_abandoned() {
    local lock_dir="$1" stale_after="${LOOM_HEARTBEAT_LOCK_STALE_SECONDS:-30}"
    local now="" owner="" created="" has_owner=0
    [[ "$stale_after" =~ ^[0-9]+$ ]] || stale_after=30
    [[ -d "$lock_dir" && ! -L "$lock_dir" ]] || return 1
    now=$(date +%s 2>/dev/null || true)
    [[ "$now" =~ ^[0-9]+$ ]] || return 1
    for owner in "$lock_dir"/owner.*; do
        [[ -e "$owner" || -L "$owner" ]] || continue
        has_owner=1
        if _loom_heartbeat_owner_is_abandoned "$owner" "$now" "$stale_after"; then
            rmdir "$owner" 2>/dev/null || true
        fi
    done
    # A creator can die between creating the directory and publishing an owner.
    if ((has_owner == 0)); then
        created=$(loom_heartbeat_lock_epoch "$lock_dir")
        [[ "$created" =~ ^[0-9]+$ ]] || return 1
        ((now - created >= stale_after)) || return 1
    fi
    rmdir "$lock_dir" 2>/dev/null || return 1
    loom_debug "heartbeat: recovered abandoned lock $lock_dir"
}

# _loom_heartbeat_owner_publish <lock-dir> <owner-dir>
_loom_heartbeat_owner_publish() {
    local lock_dir="$1" owner="$2" entry=""
    mkdir -m 700 "$owner" 2>/dev/null || return 1
    for entry in "$lock_dir"/owner.*; do
        if [[ "$entry" != "$owner" ]]; then
            rmdir "$owner" 2>/dev/null || true
            rmdir "$lock_dir" 2>/dev/null || true
            return 1
        fi
    done
}

loom_heartbeat_lock_acquire() {
    local lock_dir="$1" attempts=0 created="" owner=""
    [[ -n "$lock_dir" && ! -L "$lock_dir" ]] || return 1
    created=$(date +%s 2>/dev/null || true)
    [[ "$created" =~ ^[0-9]+$ ]] || return 1
    owner="$lock_dir/owner.$$.$created.$RANDOM$RANDOM"
    while ((attempts < 200)); do
        if mkdir -m 700 "$lock_dir" 2>/dev/null; then
            _loom_heartbeat_owner_publish "$lock_dir" "$owner" && return 0
        else
            loom_heartbeat_lock_recover_if_abandoned "$lock_dir" || true
        fi
        attempts=$((attempts + 1))
        sleep 0.01
    done
    loom_debug "heartbeat: timed out acquiring $lock_dir; skipping refresh"
    return 1
}

loom_heartbeat_lock_release() {
    local lock_dir="$1" owner="" released=0
    [[ -n "$lock_dir" && -d "$lock_dir" && ! -L "$lock_dir" ]] || return 0
    for owner in "$lock_dir"/owner."$$".*; do
        if [[ -d "$owner" && ! -L "$owner" ]] && rmdir "$owner" 2>/dev/null; then
            released=1
        fi
    done
    if ((released == 1)); then
        rmdir "$lock_dir" 2>/dev/null || true
    fi
    return 0
}

# loom_heartbeat_owner_is_current <work-dir> <stage-id> <session-id> <file>
#
# Stage assignment is authoritative when its readable stage document names a
# session. Otherwise a readable existing heartbeat supplies the current owner.
# This lets the successor named by the stage replace an old heartbeat, while a
# delayed hook from the old session can never reclaim it. With no readable
# ownership record (the ordinary first SessionStart) the caller may establish
# ownership by atomically creating the heartbeat.
loom_heartbeat_owner_is_current() {
    local work_dir="$1" stage_id="$2" session_id="$3" heartbeat_file="$4"
    local stage_file="" stage_lookup_status=0 stage_session="" heartbeat_session=""

    if stage_file=$(loom_find_stage_file "$work_dir" "$stage_id"); then
        stage_lookup_status=0
    else
        stage_lookup_status=$?
        stage_file=""
    fi
    if [[ "$stage_lookup_status" -eq 2 ]]; then
        loom_debug "heartbeat: refusing ownership decision for ambiguous stage $stage_id"
        return 1
    fi

    if [[ -n "$stage_file" && -r "$stage_file" && ! -L "$stage_file" ]]; then
        stage_session=$(loom_stage_frontmatter_field "$stage_file" session || true)
        [[ "$stage_session" =~ ^[A-Za-z0-9._-]+$ ]] || stage_session=""
        if [[ -n "$stage_session" ]]; then
            if [[ "$stage_session" != "$session_id" ]]; then
                loom_debug "heartbeat: session $session_id no longer owns stage $stage_id"
                return 1
            fi
            return 0
        fi
    fi

    if [[ -r "$heartbeat_file" && ! -L "$heartbeat_file" ]]; then
        if command -v jq &>/dev/null; then
            heartbeat_session=$(jq -r '.session_id // empty' "$heartbeat_file" 2>/dev/null || true)
        else
            heartbeat_session=$(sed -n 's/.*"session_id"[[:space:]]*:[[:space:]]*"\([A-Za-z0-9._-][A-Za-z0-9._-]*\)".*/\1/p' "$heartbeat_file" 2>/dev/null | head -n 1)
        fi
        if [[ -n "$heartbeat_session" && "$heartbeat_session" != "$session_id" ]]; then
            loom_debug "heartbeat: session $session_id no longer owns heartbeat for stage $stage_id"
            return 1
        fi
    fi
    return 0
}

# loom_heartbeat_atomic_write <heartbeat-file> <json>
#
# Write complete JSON to a same-directory temporary file and rename it into
# place. Readers therefore observe either the previous complete document or
# the new complete document, never a shell-redirection truncation. The caller
# must hold the heartbeat lock and has already checked ownership; re-checking
# the leaf symlink immediately before rename preserves the hook's no-follow
# policy even when a non-hook actor changes the path while we hold the lock.
loom_heartbeat_atomic_write() {
    local heartbeat_file="$1" heartbeat_json="$2" temp_file=""
    [[ -n "$heartbeat_file" && ! -L "$heartbeat_file" && ! -d "$heartbeat_file" ]] || return 1
    temp_file=$(mktemp "${heartbeat_file}.tmp.XXXXXX" 2>/dev/null) || return 1
    if ! printf '%s\n' "$heartbeat_json" >"$temp_file" 2>/dev/null; then
        rm -f "$temp_file" 2>/dev/null || true
        return 1
    fi
    chmod 600 "$temp_file" 2>/dev/null || true
    if [[ -L "$heartbeat_file" ]]; then
        rm -f "$temp_file" 2>/dev/null || true
        return 1
    fi
    if ! mv -f "$temp_file" "$heartbeat_file" 2>/dev/null; then
        rm -f "$temp_file" 2>/dev/null || true
        return 1
    fi
    chmod 600 "$heartbeat_file" 2>/dev/null || true
    return 0
}

# loom_heartbeat_prior_progress_at <heartbeat-file> <fallback-timestamp>
#
# Preserve a validated progress timestamp that does not exceed the prior
# heartbeat timestamp. Fall back to that timestamp, then to the caller's.
loom_heartbeat_prior_progress_at() {
    local heartbeat_file="$1" fallback="$2" prior=""
    if [[ -r "$heartbeat_file" ]] && command -v jq &>/dev/null; then
        prior=$(jq -r '
            def valid: if type == "string" then test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}\\.000Z$") else false end;
            .timestamp as $timestamp | (.progress_at // null) as $progress |
            if (($progress | valid) and ($timestamp | valid) and $progress <= $timestamp)
            then $progress elif ($timestamp | valid) then $timestamp else empty end
        ' "$heartbeat_file" 2>/dev/null || true)
    fi
    printf '%s\n' "${prior:-$fallback}"
}
