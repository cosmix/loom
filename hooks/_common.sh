#!/usr/bin/env bash
# _common.sh - Shared utilities for loom hooks
#
# Source guard prevents double-sourcing.
# Provides strip_embedded_content() to remove heredoc bodies and
# -m/--message quoted content before pattern matching, preventing
# false positives from text inside commit messages or heredocs.
#
# Bash 3.2+ compatible (macOS default). No perl dependency.
# Uses POSIX awk (no gawk extensions).
#
# Usage:
#   source "$(dirname "$0")/_common.sh"
#   local stripped
#   stripped=$(strip_embedded_content "$command")
#   # Use $stripped for pattern checks, $command for error messages

# Source guard
if [[ "${_LOOM_COMMON_LOADED:-}" == "1" ]]; then
    return 0
fi
_LOOM_COMMON_LOADED=1

# strip_embedded_content - Remove heredoc bodies and -m/--message content
#
# Phase 1: awk strips heredoc bodies (state machine tracking <<MARKER to ^MARKER$)
# Phase 2: sed strips -m "..." / -m '...' / --message="..." / --message '...'
#
# Arguments:
#   $1 - The command string to strip
#
# Output:
#   Stripped command on stdout
strip_embedded_content() {
    local input="$1"

    # Phase 1: Strip heredoc bodies using awk state machine
    # POSIX awk compatible (no gawk array captures)
    local phase1
    phase1=$(printf '%s\n' "$input" | awk '
BEGIN { inside = 0; marker = "" }
{
    if (inside) {
        if ($0 == marker) {
            inside = 0
        }
        next
    }
    # Detect heredoc: <<[-]?[ ]*[quote]?MARKER[quote]?
    if (match($0, /<<-?[[:space:]]*["\x27]?[A-Za-z_][A-Za-z0-9_]*["\x27]?/)) {
        s = substr($0, RSTART, RLENGTH)
        # Remove << prefix, optional dash, whitespace, quotes
        sub(/^<<-?[[:space:]]*["\x27]?/, "", s)
        sub(/["\x27]?$/, "", s)
        if (s != "") {
            marker = s
            inside = 1
            print
            next
        }
    }
    print
}')

    # Phase 2: Strip -m / --message quoted content
    # Replace -m "..." with -m ""
    # Replace -m '...' with -m ''
    # Replace --message="..." with --message=""
    # Replace --message='...' with --message=''
    # Replace --message "..." with --message ""
    # Replace --message '...' with --message ''
    local phase2
    phase2=$(printf '%s' "$phase1" | sed \
        -e 's/-m[[:space:]]*"[^"]*"/-m ""/g' \
        -e "s/-m[[:space:]]*'[^']*'/-m ''/g" \
        -e 's/--message=[[:space:]]*"[^"]*"/--message=""/g' \
        -e "s/--message=[[:space:]]*'[^']*'/--message=''/g" \
        -e 's/--message[[:space:]]*"[^"]*"/--message ""/g' \
        -e "s/--message[[:space:]]*'[^']*'/--message ''/g")

    printf '%s' "$phase2"
}

# loom_debug - Emit a debug line to stderr when LOOM_HOOK_DEBUG=1 (or the
# legacy COMMIT_FILTER_DEBUG=1). Defined here rather than relying on the
# caller's own `debug`, so every hook that sources this file can call it.
loom_debug() {
    if [[ "${LOOM_HOOK_DEBUG:-}" == "1" || "${COMMIT_FILTER_DEBUG:-}" == "1" ]]; then
        echo "$@" >&2
    fi
    return 0
}

# --- Subagent detection ------------------------------------------------------
#
# The main agent's wrapper exports LOOM_MAIN_AGENT_PID and then `exec claude`,
# so for the MAIN agent that PID *is* the Claude process. A subagent runs as a
# separate Claude process below it, so there is at least one extra Claude
# process between us and LOOM_MAIN_AGENT_PID.
#
# is_ancestor / find_nearest_claude_ancestor / count_claude_processes_between
# are internal helpers for loom_is_subagent - hooks should call
# loom_is_subagent, not these.

# is_ancestor - Check if a PID is in our ancestor chain
# Returns 0 if found, 1 if not
is_ancestor() {
    local target_pid="$1"
    local current_pid="$$"

    while [[ "$current_pid" != "1" && "$current_pid" != "0" && -n "$current_pid" ]]; do
        if [[ "$current_pid" == "$target_pid" ]]; then
            return 0
        fi

        # Get parent PID
        if [[ -r "/proc/$current_pid/stat" ]]; then
            current_pid=$(awk '{print $4}' "/proc/$current_pid/stat" 2>/dev/null || true)
        else
            current_pid=$(ps -o ppid= -p "$current_pid" 2>/dev/null | tr -d ' ' || true)
        fi
    done

    return 1
}

# find_nearest_claude_ancestor - Find the nearest Claude Code process ancestor
# Returns its PID if found, empty string if not found
find_nearest_claude_ancestor() {
    local current_pid="$$"

    while [[ "$current_pid" != "1" && "$current_pid" != "0" && -n "$current_pid" ]]; do
        # Check if this process is Claude Code
        local cmdline=""
        if [[ -r "/proc/$current_pid/cmdline" ]]; then
            # Linux: read cmdline (null-separated)
            cmdline=$(tr '\0' ' ' <"/proc/$current_pid/cmdline" 2>/dev/null || true)
        else
            # macOS: use ps
            cmdline=$(ps -o command= -p "$current_pid" 2>/dev/null || true)
        fi

        # Claude Code runs as node with "claude" in the binary/args
        # Exclude matches that are just hook scripts (paths containing .claude/hooks)
        if echo "$cmdline" | grep -qi "claude"; then
            if echo "$cmdline" | grep -q "\.claude/hooks"; then
                # This is a hook script, not Claude Code - skip it
                loom_debug "DEBUG: Skipping PID $current_pid - hook script: $cmdline"
            else
                echo "$current_pid"
                return 0
            fi
        fi

        # Get parent PID
        if [[ -r "/proc/$current_pid/stat" ]]; then
            current_pid=$(awk '{print $4}' "/proc/$current_pid/stat" 2>/dev/null || true)
        else
            current_pid=$(ps -o ppid= -p "$current_pid" 2>/dev/null | tr -d ' ' || true)
        fi
    done

    echo ""
    return 1
}

# count_claude_processes_between - Count Claude processes between two PIDs
# (exclusive of start, inclusive of end).
# Returns the count on stdout. If end PID is not found, returns 999.
count_claude_processes_between() {
    local start_pid="$1"
    local end_pid="$2"
    local count=0

    local current_pid="$start_pid"
    # Move to parent first (start is exclusive)
    if [[ -r "/proc/$current_pid/stat" ]]; then
        current_pid=$(awk '{print $4}' "/proc/$current_pid/stat" 2>/dev/null || true)
    else
        current_pid=$(ps -o ppid= -p "$current_pid" 2>/dev/null | tr -d ' ' || true)
    fi

    while [[ "$current_pid" != "1" && "$current_pid" != "0" && -n "$current_pid" ]]; do
        if [[ "$current_pid" == "$end_pid" ]]; then
            echo "$count"
            return 0
        fi

        # Check if this process is Claude Code (not a hook script)
        local cmdline=""
        if [[ -r "/proc/$current_pid/cmdline" ]]; then
            cmdline=$(tr '\0' ' ' <"/proc/$current_pid/cmdline" 2>/dev/null || true)
        else
            cmdline=$(ps -o command= -p "$current_pid" 2>/dev/null || true)
        fi

        if echo "$cmdline" | grep -qi "claude" && ! echo "$cmdline" | grep -q "\.claude/hooks"; then
            # Not ((count++)): that returns 1 when count is 0, which trips
            # errexit inside the caller's command substitution
            count=$((count + 1))
        fi

        # Get parent PID
        if [[ -r "/proc/$current_pid/stat" ]]; then
            current_pid=$(awk '{print $4}' "/proc/$current_pid/stat" 2>/dev/null || true)
        else
            current_pid=$(ps -o ppid= -p "$current_pid" 2>/dev/null | tr -d ' ' || true)
        fi
    done

    echo "999" # End PID not found
    return 0  # Don't return 1 - it triggers set -e in the sourcing hook
}

# loom_is_subagent - Return 0 when this hook runs under a SUBAGENT, non-zero
# otherwise (main agent, or no live loom session at all).
#
# Depth-agnostic: any number of Claude processes between us and the main agent
# means subagent. LOOM_MAIN_AGENT_PID must be a LIVE ancestor - a leaked value
# from a previous session names a PID that is not in our chain, and is ignored.
loom_is_subagent() {
    local main_pid="${LOOM_MAIN_AGENT_PID:-}"
    if [[ -z "$main_pid" ]]; then
        return 1
    fi

    # Stale value (from a previous session) - not in our ancestor chain
    if ! is_ancestor "$main_pid"; then
        loom_debug "DEBUG: LOOM_MAIN_AGENT_PID=$main_pid is NOT in ancestor chain - stale value, ignoring"
        return 1
    fi

    local nearest
    nearest=$(find_nearest_claude_ancestor || true)
    loom_debug "DEBUG: LOOM_MAIN_AGENT_PID=$main_pid, PPID=$PPID, NEAREST_CLAUDE=$nearest"
    if [[ -z "$nearest" ]]; then
        return 1
    fi

    local claude_count
    if [[ "$nearest" == "$main_pid" ]]; then
        # Fast path: the wrapper used `exec claude`, so the main agent's Claude
        # process IS LOOM_MAIN_AGENT_PID
        claude_count=0
        loom_debug "DEBUG: Fast path - NEAREST_CLAUDE == LOOM_MAIN_AGENT_PID (same process after exec)"
    else
        claude_count=$(count_claude_processes_between "$nearest" "$main_pid")
        loom_debug "DEBUG: Claude processes between NEAREST_CLAUDE and LOOM_MAIN_AGENT_PID: $claude_count"
    fi

    if [[ "$claude_count" == "0" ]]; then
        loom_debug "DEBUG: Main agent detected - no intermediate Claude processes"
        return 1
    fi

    loom_debug "DEBUG: Subagent detected - $claude_count intermediate Claude process(es)"
    return 0
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
#   (b) LOOM_WORKTREE_PATH points into `.worktrees/` AND that directory still
#       exists on disk (the on-disk check rejects a stale, leaked value).
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
    if [[ -n "$wt" && -d "$wt" && "$wt" =~ /\.worktrees/[^/]+ ]]; then
        printf '%s' "$wt"
        return 0
    fi

    return 1
}
