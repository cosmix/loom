#!/usr/bin/env bash
# Shared hook helpers; Bash 3.2 compatible.
if [[ "${_LOOM_HEARTBEAT_LOADED:-}" == 1 ]]; then return 0; fi
_LOOM_HEARTBEAT_LOADED=1

# loom_heartbeat_lock_acquire <lock-dir>
# loom_heartbeat_lock_release <lock-dir>
#
# Serialize every shell heartbeat writer's ownership check and replacement.
# `mkdir` is the one portable atomic primitive available on both macOS and
# Linux; `flock` is not installed by default on macOS. A lock records its PID
# and creation epoch. After a conservative grace period an old lock whose PID
# is gone (or whose creator died between mkdir and metadata publication) is
# reclaimed, so SIGKILL cannot strand a stage permanently. Metadata is built
# before acquisition and hard-linked into the winning directory: if an empty
# lock is reclaimed while its creator is paused, only one contender can claim
# the owner path and enter the critical section. A valid live owner is never
# stolen; malformed published metadata fails closed.
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

loom_heartbeat_lock_recover_if_abandoned() {
    local lock_dir="$1" stale_after="${LOOM_HEARTBEAT_LOCK_STALE_SECONDS:-30}"
    local owner_file="${lock_dir}/owner" now="" created="" pid=""
    [[ "$stale_after" =~ ^[0-9]+$ ]] || stale_after=30
    [[ -d "$lock_dir" && ! -L "$lock_dir" && ! -L "$owner_file" ]] || return 1

    now=$(date +%s 2>/dev/null || true)
    [[ "$now" =~ ^[0-9]+$ ]] || return 1
    if [[ -e "$owner_file" ]]; then
        [[ -f "$owner_file" && -r "$owner_file" ]] || return 1
        created=$(sed -n 's/^created=\([0-9][0-9]*\)$/\1/p' "$owner_file" 2>/dev/null | head -n 1)
        pid=$(sed -n 's/^pid=\([0-9][0-9]*\)$/\1/p' "$owner_file" 2>/dev/null | head -n 1)
        # Published owner metadata is atomic and complete. Anything else is
        # external corruption or an unknown writer, so never steal it.
        [[ "$created" =~ ^[0-9]+$ && "$pid" =~ ^[0-9]+$ ]] || return 1
    else
        created=$(loom_heartbeat_lock_epoch "$lock_dir")
    fi
    [[ "$created" =~ ^[0-9]+$ ]] || return 1
    (( now - created >= stale_after )) || return 1

    # A valid, live PID always wins, even if the directory is unusually old.
    if [[ "$pid" =~ ^[0-9]+$ ]] && kill -0 "$pid" 2>/dev/null; then
        return 1
    fi

    # No compliant writer can replace this directory while it exists. Remove
    # only its known metadata then rmdir; a contender can acquire only after
    # rmdir, and the outer acquisition loop will contend normally again.
    if [[ -e "$owner_file" ]]; then
        rm -f "$owner_file" 2>/dev/null || return 1
    fi
    rmdir "$lock_dir" 2>/dev/null || return 1
    loom_debug "heartbeat: recovered abandoned lock $lock_dir"
    return 0
}

loom_heartbeat_lock_acquire() {
    local lock_dir="$1" attempts=0 claim_file="" created=""
    [[ -n "$lock_dir" && ! -L "$lock_dir" ]] || return 1
    created=$(date +%s 2>/dev/null || true)
    [[ "$created" =~ ^[0-9]+$ ]] || return 1
    claim_file=$(mktemp "${lock_dir}.claim.XXXXXX" 2>/dev/null) || return 1
    chmod 600 "$claim_file" 2>/dev/null || true
    if ! printf 'pid=%s\ncreated=%s\n' "$$" "$created" >"$claim_file" 2>/dev/null; then
        rm -f "$claim_file" 2>/dev/null || true
        return 1
    fi
    while ! mkdir -m 700 "$lock_dir" 2>/dev/null; do
        loom_heartbeat_lock_recover_if_abandoned "$lock_dir" || true
        attempts=$((attempts + 1))
        if [[ "$attempts" -ge 200 ]]; then
            loom_debug "heartbeat: timed out acquiring $lock_dir; skipping refresh"
            rm -f "$claim_file" 2>/dev/null || true
            return 1
        fi
        sleep 0.01
    done
    # `ln` is the atomic ownership claim. If this directory was recovered and
    # re-created while we were paused after mkdir, the successor's link wins
    # and we must not enter or release its critical section.
    if ! ln "$claim_file" "${lock_dir}/owner" 2>/dev/null; then
        rmdir "$lock_dir" 2>/dev/null || true
        rm -f "$claim_file" 2>/dev/null || true
        return 1
    fi
    rm -f "$claim_file" 2>/dev/null || true
    return 0
}

loom_heartbeat_lock_release() {
    local lock_dir="$1" owner_file="${1}/owner" owner_pid=""
    [[ -n "$lock_dir" && -d "$lock_dir" && ! -L "$lock_dir" ]] || return 0
    [[ -f "$owner_file" && ! -L "$owner_file" ]] || return 0
    owner_pid=$(sed -n 's/^pid=\([0-9][0-9]*\)$/\1/p' "$owner_file" 2>/dev/null | head -n 1)
    if [[ "$owner_pid" != "$$" ]]; then
        loom_debug "heartbeat: refusing to release lock owned by pid ${owner_pid:-unknown}"
        return 0
    fi
    rm -f "$owner_file" 2>/dev/null || return 0
    rmdir "$lock_dir" 2>/dev/null || true
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
        # Only the top-level FRONTMATTER field is authoritative. Stage bodies
        # contain the raw description, which may itself include an unindented
        # `session:` line or fenced example. Stop at the second exact delimiter
        # so prose can never impersonate ownership.
        stage_session=$(awk '
            NR == 1 && $0 == "---" { in_frontmatter = 1; next }
            in_frontmatter && $0 == "---" { exit }
            in_frontmatter && $0 ~ /^session:[[:space:]]*[A-Za-z0-9._-]+[[:space:]]*$/ {
                value = $0
                sub(/^session:[[:space:]]*/, "", value)
                sub(/[[:space:]]*$/, "", value)
                print value
                exit
            }
        ' "$stage_file" 2>/dev/null || true)
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
