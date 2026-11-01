#!/usr/bin/env bash
# Compatibility loader for shared hook helpers.
if [[ "${_LOOM_COMMON_LOADED:-}" == 1 ]]; then return 0; fi
# loom_debug - Emit a debug line to stderr when LOOM_HOOK_DEBUG=1 (or the
# legacy COMMIT_FILTER_DEBUG=1). Defined here rather than relying on the
# caller's own `debug`, so every hook that sources this file can call it.
loom_debug() {
    if [[ "${LOOM_HOOK_DEBUG:-}" == "1" || "${COMMIT_FILTER_DEBUG:-}" == "1" ]]; then
        echo "$@" >&2
    fi
    return 0
}

# loom_jq_missing_message <hook-name>
#
# The one sentence every hook prints when jq is absent, so the user sees the
# same diagnosis whichever hook fires first. Printed to stdout; callers
# redirect to stderr.
loom_jq_missing_message() {
    printf 'LOOM_HOOK_ERROR: jq is not installed, so %s cannot read the Claude Code hook payload. jq is a required loom dependency: install it (apt install jq / brew install jq / https://jqlang.github.io/jq/download/) and start a new session.\n' "$1"
}

# loom_require_jq <hook-name>
#
# For BLOCKING guards (any hook whose header documents exit 2). A guard that
# cannot parse its payload must not silently allow, so when jq is absent this
# prints the shared message on stderr and exits 2 (deny). Returns 0 when jq
# is present. Call it right after sourcing this file, before reading stdin.
loom_require_jq() {
    command -v jq &>/dev/null && return 0
    loom_jq_missing_message "$1" >&2
    exit 2
}

# loom_warn_no_jq <hook-name>
#
# For ADVISORY hooks (header says exit 0 always, or deny-or-warn). Exit 1 is
# Claude Code's non-blocking hook error: stderr is shown to the user and the
# tool call proceeds. Returns 0 when jq is present.
loom_warn_no_jq() {
    command -v jq &>/dev/null && return 0
    loom_jq_missing_message "$1" >&2
    exit 1
}

# loom_saved_output_path <text>
#
# Print the path from the FIRST line of <text> carrying the literal
# "Full output saved to: " notice - everything after the marker's last
# occurrence on that line, the same extraction
# `sed -n 's/^.*Full output saved to: //p' | head -n1` performed. Prints
# nothing when no line carries the notice. Always returns 0.
#
# Deliberately no `| head` here: under `set -o pipefail`, a `sed` still
# writing lines after `head -n1` has read its one line and exited takes
# SIGPIPE and dies with 141, which `errexit` then turns into an unclassified
# hook abort instead of the caller's normal fail-closed path - the same bug
# class commit ec721be5 fixed for commit-guard.sh's git-status pipeline.
# Walking the here-string with a plain read loop never opens that pipe.
loom_saved_output_path() {
    local line
    while IFS= read -r line; do
        case "$line" in
        *'Full output saved to: '*)
            printf '%s' "${line##*'Full output saved to: '}"
            return 0
            ;;
        esac
    done <<<"$1"
    return 0
}


source "$(dirname "${BASH_SOURCE[0]}")/_parser.sh" || return 1
source "$(dirname "${BASH_SOURCE[0]}")/_parser_expand.sh" || return 1
source "$(dirname "${BASH_SOURCE[0]}")/_parser_scan.sh" || return 1
source "$(dirname "${BASH_SOURCE[0]}")/_identity.sh" || return 1
source "$(dirname "${BASH_SOURCE[0]}")/_stage_lookup.sh" || return 1
source "$(dirname "${BASH_SOURCE[0]}")/_containment.sh" || return 1
source "$(dirname "${BASH_SOURCE[0]}")/_heartbeat.sh" || return 1
_LOOM_COMMON_LOADED=1
