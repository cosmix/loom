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
