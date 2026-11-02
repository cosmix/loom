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
    if [[ -z "${_LOOM_DENY_ENABLED_CACHE:-}" ]]; then
        _LOOM_DENY_ENABLED_CACHE=0
        if _loom_hooks_setting_is deny_enabled true; then
            _LOOM_DENY_ENABLED_CACHE=1
        fi
    fi
    [[ "$_LOOM_DENY_ENABLED_CACHE" == "1" ]]
}

# loom_read_gate_enabled - Return 0 unless a `[hooks]` table in
# "$LOOM_WORK_DIR/config.toml" holds exactly `read_gate = false`. The default
# is the inverse of loom_deny_enabled's: this answers "disabled" only on an
# unambiguous `false`, and any doubt leaves the gate on. Only the deny-once
# read gate (loom_read_deny_once, _read_ledger.sh) consults it, and only after
# proving a live stage, so it cannot block a session with no loom above it.
loom_read_gate_enabled() {
    ! _loom_hooks_setting_is read_gate false
}

# _loom_hooks_setting_is <key> <value> - Return 0 when a `[hooks]` table in
# "$LOOM_WORK_DIR/config.toml" holds `<key> = <value>` exactly (surrounding
# whitespace ignored). LOOM_WORK_DIR unset, the file absent or unreadable, no
# `[hooks]` table, the key outside that table, or any other value all fail.
_loom_hooks_setting_is() {
    local work_dir="${LOOM_WORK_DIR:-}" found
    [[ -n "$work_dir" && -r "${work_dir}/config.toml" ]] || return 1
    found=$(awk -v key="$1" -v value="$2" '
        {
            t = $0
            sub(/^[[:space:]]+/, "", t)
            sub(/[[:space:]]+$/, "", t)
        }
        t == "[hooks]" { in_hooks = 1; next }
        t ~ /^\[/ { in_hooks = 0; next }
        in_hooks && match(t, /^[A-Za-z0-9_-]+[[:space:]]*=[[:space:]]*/) {
            k = t
            sub(/[[:space:]]*=.*$/, "", k)
            if (k == key && substr(t, RLENGTH + 1) == value) {
                print "1"
                exit
            }
        }
    ' "${work_dir}/config.toml" 2>/dev/null || true)
    [[ "$found" == "1" ]]
}

# loom_cwd_worktree - Echo the worktree root holding the current directory, or
# return non-zero outside one. No environment variable can answer yes, so a
# globally registered guard that must leave unrelated sessions alone uses this.
loom_cwd_worktree() {
    local dir
    dir=$(pwd 2>/dev/null) || dir=""
    if [[ "$dir" =~ ^(.*/\.worktrees/[^/]+) ]]; then
        printf '%s' "${BASH_REMATCH[1]}"
        return 0
    fi
    return 1
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
#       itself lives under an outer worktree does not count - that directory
#       still exists on disk, AND the variable is this session's own
#       (loom_in_live_session).
#
# (b) is what keeps a stage session contained after it moves out of its worktree,
# since native file tools run outside the OS sandbox. The value also leaks into
# any shell a loom session starts, and it can name a live worktree, so its
# presence alone would block an unrelated session of a globally registered guard.
# An agent cannot unset its own agent's variables, so leaving gains nothing.
# Known limit: an agent-team teammate is outside the main agent's process tree,
# so only (a) binds it. Revisit if the harness exposes teammate ancestry.
#
# Returns the worktree root on stdout.
loom_current_worktree() {
    loom_cwd_worktree && return 0

    local wt="${LOOM_WORKTREE_PATH:-}"
    if [[ -n "$wt" && -d "$wt" && "$wt" =~ /\.worktrees/[^/]+/?$ ]] && loom_in_live_session; then
        printf '%s' "$wt"
        return 0
    fi

    return 1
}
