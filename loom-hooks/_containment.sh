#!/usr/bin/env bash
if [[ "${_LOOM_CONTAINMENT_LOADED:-}" == 1 ]]; then return 0; fi
_LOOM_CONTAINMENT_LOADED=1

# loom_deny_enabled - Return 0 when the operator has opted in to the deny
# branches of the read/poll discipline hooks, non-zero otherwise (the
# default).
#
# The switch lives in the loom work directory's own config, not in the repo
# or a plan: a `true` value under a `[hooks]` table in
# "$LOOM_WORK_DIR/config.toml":
#
#   [hooks]
#   deny_enabled = true
#
# LOOM_WORK_DIR being unset, the file being absent or unreadable, no
# `[hooks]` table, the key sitting outside that table, a value other than an
# exact `true`, or any doubt while parsing all mean DISABLED - this only ever
# answers "enabled" on an unambiguous match, because a false "enabled" here
# starts hard-blocking real tool calls in every Claude Code session on the
# machine. The answer is computed once and cached in
# _LOOM_DENY_ENABLED_CACHE for the life of this process; later calls skip the
# file read entirely.
loom_deny_enabled() {
    if [[ -n "${_LOOM_DENY_ENABLED_CACHE:-}" ]]; then
        [[ "$_LOOM_DENY_ENABLED_CACHE" == "1" ]]
        return
    fi

    _LOOM_DENY_ENABLED_CACHE=0

    local work_dir="${LOOM_WORK_DIR:-}"
    if [[ -n "$work_dir" ]]; then
        local config="${work_dir}/config.toml"
        if [[ -r "$config" ]]; then
            local found
            found=$(awk '
                {
                    t = $0
                    sub(/^[[:space:]]+/, "", t)
                    sub(/[[:space:]]+$/, "", t)
                }
                t == "[hooks]" { in_hooks = 1; next }
                t ~ /^\[/ { in_hooks = 0; next }
                in_hooks && t ~ /^deny_enabled[[:space:]]*=[[:space:]]*true$/ {
                    print "1"
                    exit
                }
            ' "$config" 2>/dev/null || true)
            if [[ "$found" == "1" ]]; then
                _LOOM_DENY_ENABLED_CACHE=1
            fi
        fi
    fi

    [[ "$_LOOM_DENY_ENABLED_CACHE" == "1" ]]
}

# loom_current_worktree - Echo the loom worktree root this session is operating
# in, or return non-zero if this is NOT a loom worktree session.
#
# A loom worktree lives at `<repo>/.worktrees/<stage-id>/`. Membership is decided
# by LOCATION, never by LOOM_STAGE_ID: that variable leaks into plain Claude Code
# sessions (e.g. a prior loom run exported it into the shell), so gating on it
# alone makes the isolation hooks wrongly fire on ordinary branches like main.
#
# A session counts as inside a worktree when either:
#   (a) the current working directory is inside `.worktrees/<stage>/`, or
#   (b) LOOM_WORKTREE_PATH names a worktree root - it ENDS at
#       `.worktrees/<stage>`, which is what the wrapper exports, so a repo that
#       itself lives under an outer worktree does not count - AND that
#       directory still exists on disk (the on-disk check rejects a stale,
#       leaked value).
#
# Returns the worktree root on stdout.
loom_current_worktree() {
    local dir
    dir=$(pwd 2>/dev/null) || dir=""
    if [[ "$dir" =~ ^(.*/\.worktrees/[^/]+) ]]; then
        printf '%s' "${BASH_REMATCH[1]}"
        return 0
    fi

    local wt="${LOOM_WORKTREE_PATH:-}"
    if [[ -n "$wt" && -d "$wt" && "$wt" =~ /\.worktrees/[^/]+/?$ ]]; then
        printf '%s' "$wt"
        return 0
    fi

    return 1
}
