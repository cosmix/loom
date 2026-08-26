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

# loom_tokenize_command - Permissively tokenize a shell command string into
# argv-shaped words, populating the global array LOOM_TOKENS.
#
# Unlike the strict parser in codex-forward-guard.sh (which must REJECT any
# command containing an operator), this walks the string with a plain /
# single / double quote state machine and tolerates arbitrary shell text -
# it reports structure, it does not validate or reject shell syntax.
#
# Alongside real argv words, LOOM_TOKENS carries the literal sentinel token
# "%%SEP%%" at every point a new command position begins: after `;` `&` `|`
# a newline, `<`, `>`, `(`, `)`, a backtick, or the two-character opener
# `$(` (in `double` state only the substitution openers - backtick and `$(`
# - count; the rest are literal there, as in real bash). Runs of separators
# collapse to ONE sentinel, so `&&` does not yield two. A token that came
# from quoted text is stored with its quotes removed and its escapes
# resolved, exactly as bash would pass it in argv - the sentinel is never
# tagged onto a real token, and a real token is never mistaken for one
# because "%%SEP%%" is pushed only from the separator-handling branches
# below, never appended to `token`.
#
# Arguments:
#   $1 - The command string to tokenize
#
# Output:
#   Populates the global array LOOM_TOKENS on stdout as a side effect (no
#   stdout output of its own). Returns 0 on a clean parse (ended back in the
#   `plain` state). Returns 1 if the string ends inside an unterminated
#   quote - callers should treat that as "could not tokenize" and fall back
#   to a more conservative check rather than trust a partial LOOM_TOKENS.
#
# Bash 3.2+ compatible: no associative arrays, no `${arr[-1]}` negative
# indexing, no `declare -n` namerefs.
loom_tokenize_command() {
    local input="$1"
    local state=plain
    local token=""
    local started=0
    local last_was_sep=0
    local length=${#input}
    local i char next

    LOOM_TOKENS=()

    for ((i = 0; i < length; i++)); do
        char="${input:$i:1}"
        case "$state" in
        plain)
            case "$char" in
            ' ' | $'\t')
                if [[ $started -eq 1 ]]; then
                    LOOM_TOKENS+=("$token")
                    token=""
                    started=0
                    last_was_sep=0
                fi
                ;;
            $'\n' | ';' | '&' | '|' | '<' | '>' | '(' | ')' | '`')
                if [[ $started -eq 1 ]]; then
                    LOOM_TOKENS+=("$token")
                    token=""
                    started=0
                    last_was_sep=0
                fi
                if [[ $last_was_sep -eq 0 ]]; then
                    LOOM_TOKENS+=("%%SEP%%")
                    last_was_sep=1
                fi
                ;;
            "'")
                state=single
                started=1
                ;;
            '"')
                state=double
                started=1
                ;;
            '\')
                if ((i + 1 < length)); then
                    i=$((i + 1))
                    token+="${input:$i:1}"
                else
                    token+='\'
                fi
                started=1
                ;;
            '$')
                if [[ "${input:$((i + 1)):1}" == "(" ]]; then
                    if [[ $started -eq 1 ]]; then
                        LOOM_TOKENS+=("$token")
                        token=""
                        started=0
                        last_was_sep=0
                    fi
                    if [[ $last_was_sep -eq 0 ]]; then
                        LOOM_TOKENS+=("%%SEP%%")
                        last_was_sep=1
                    fi
                    i=$((i + 1))
                else
                    token+="$char"
                    started=1
                fi
                ;;
            *)
                token+="$char"
                started=1
                ;;
            esac
            ;;
        single)
            if [[ "$char" == "'" ]]; then
                state=plain
            else
                token+="$char"
            fi
            ;;
        double)
            case "$char" in
            '"')
                state=plain
                ;;
            '\')
                if ((i + 1 < length)); then
                    next="${input:$((i + 1)):1}"
                    case "$next" in
                    '"' | '\' | '$' | '`')
                        token+="$next"
                        i=$((i + 1))
                        ;;
                    *)
                        token+='\'
                        ;;
                    esac
                else
                    token+='\'
                fi
                ;;
            '`')
                if [[ $started -eq 1 ]]; then
                    LOOM_TOKENS+=("$token")
                    token=""
                    started=0
                    last_was_sep=0
                fi
                if [[ $last_was_sep -eq 0 ]]; then
                    LOOM_TOKENS+=("%%SEP%%")
                    last_was_sep=1
                fi
                state=plain
                ;;
            '$')
                if [[ "${input:$((i + 1)):1}" == "(" ]]; then
                    if [[ $started -eq 1 ]]; then
                        LOOM_TOKENS+=("$token")
                        token=""
                        started=0
                        last_was_sep=0
                    fi
                    if [[ $last_was_sep -eq 0 ]]; then
                        LOOM_TOKENS+=("%%SEP%%")
                        last_was_sep=1
                    fi
                    i=$((i + 1))
                    state=plain
                else
                    token+="$char"
                fi
                ;;
            *)
                token+="$char"
                ;;
            esac
            ;;
        esac
    done

    if [[ $started -eq 1 ]]; then
        LOOM_TOKENS+=("$token")
    fi

    [[ "$state" == plain ]]
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
# loom_is_subagent gates on a LIVE loom session FIRST - LOOM_MAIN_AGENT_PID
# must be set and a live process-tree ancestor - because these hooks install
# globally at ~/.claude/hooks/loom/ and that precondition is the only thing
# scoping them to a loom stage session rather than every Claude Code session
# on the machine. Only once that gate passes does it classify the caller
# PAYLOAD-FIRST: Claude Code writes the hook's JSON payload to stdin, which
# the agent cannot forge, and `loom_payload_agent_verdict` reads its
# `.agent_type` / `.transcript_path` fields (the same fields
# codex-forward-guard.sh already trusts) to decide "subagent" or "main"
# outright. The process-tree walk below
# (is_ancestor / find_nearest_claude_ancestor / count_claude_processes_between)
# is kept as a fallback for a payload-less caller, because it is wrong in both
# directions on its own: a Bash-tool shell's cmdline often mentions a
# ~/.claude/ path (e.g. sourcing a shell-snapshot file) and gets counted as a
# spurious "Claude process" between the caller and LOOM_MAIN_AGENT_PID, while a
# genuine Task-tool subagent runs IN-PROCESS (same claude process as the main
# agent) and the walk finds no intervening process at all. The main agent's
# wrapper exports LOOM_MAIN_AGENT_PID and then `exec claude`, so for the MAIN
# agent that PID *is* the Claude process; that assumption only holds once the
# payload verdict is "unknown".
#
# is_ancestor / find_nearest_claude_ancestor / count_claude_processes_between /
# loom_cmdline_is_claude are internal helpers - hooks should call
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

# loom_cmdline_is_claude - Return 0 if `cmdline` names a real Claude Code
# process, 1 otherwise.
#
# Only the FIRST TWO whitespace-separated words of the cmdline may establish a
# match (the interpreter and its script/binary - `claude ...`,
# `node /path/@anthropic-ai/claude-code/cli.js ...`). A path argument further
# down the argv list no longer counts, which is deliberate: a Bash-tool shell
# spawned to run a command has a cmdline like
#   /bin/zsh -c source /home/<user>/.claude/shell-snapshots/snapshot-....sh ...
# and that ~/.claude/ mention sits well past word two, so it is no longer
# mistaken for Claude Code itself. Words containing `.claude/hooks` are
# excluded either way - that identifies a hook script, not Claude Code.
loom_cmdline_is_claude() {
    local cmdline="$1"
    local head
    head=$(printf '%s' "$cmdline" | awk '{print $1, $2}')

    if printf '%s' "$head" | grep -qi '\.claude/hooks'; then
        return 1
    fi
    printf '%s' "$head" | grep -qi "claude"
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

        if loom_cmdline_is_claude "$cmdline"; then
            echo "$current_pid"
            return 0
        else
            loom_debug "DEBUG: Skipping PID $current_pid - not Claude Code: $cmdline"
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

        if loom_cmdline_is_claude "$cmdline"; then
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

# loom_payload_agent_verdict <payload-json> - echo "subagent", "main", or
# "unknown" for the hook's raw stdin JSON payload. Precedence:
#   1. no argument, empty payload, or jq unavailable         -> unknown
#   2. `.agent_type` non-empty                                -> subagent
#      (a Task-spawned subagent always carries its agent type name; the main
#      agent's payload never does)
#   3. `.transcript_path` matches `*/subagents/agent-*.jsonl` -> subagent
#      (a subagent's transcript lives under a `subagents/` dir; only that
#      shape counts, since main-session Agent-tool payloads can also mention
#      the sentinel elsewhere in the path)
#   4. `.transcript_path` is MAIN-SHAPED                      -> main, where
#      main-shaped means ALL of: `.session_id` is non-empty, the transcript
#      path has no `/subagents/` path component, AND the transcript basename
#      is exactly `<session_id>.jsonl`. This is a POSITIVE identification,
#      not "any other shape" - verified against a real main-session
#      transcript (`<project-dir>/<session-uuid>.jsonl` with that same uuid
#      as `.session_id`).
#   5. otherwise                                              -> unknown, and
#      a debug line names the unrecognized transcript shape. This case
#      DELIBERATELY does not default to "main": an unrecognized SUBAGENT
#      transcript layout (e.g. a future `agents/agent-*.jsonl` rename, a
#      relative path, or a mid-rotation `.jsonl.tmp`) must fall through to
#      the process-tree fallback rather than being waved through by
#      elimination - granting "main" by ELIMINATION silently turns both
#      guards into no-ops the moment Claude Code's transcript layout changes
#      in a way this function does not yet recognize. The asymmetry (an
#      unrecognized MAIN shape only costs a fallback to the process walk,
#      an unrecognized SUBAGENT shape must never cost the whole guard)
#      mirrors codex-forward-guard.sh's fail-closed posture on ambiguous
#      metadata (see its header) - do not "simplify" rule 5 back to "main".
#
# Every jq call tolerates malformed JSON (`2>/dev/null || true`) so a bad
# payload degrades to "unknown" rather than tripping the sourcing hook's
# `set -e`.
loom_payload_agent_verdict() {
    local payload="${1:-}"
    if [[ -z "$payload" ]] || ! command -v jq &>/dev/null; then
        echo "unknown"
        return 0
    fi

    local agent_type transcript_path session_id
    agent_type=$(printf '%s' "$payload" | jq -r '.agent_type // empty' 2>/dev/null || true)
    transcript_path=$(printf '%s' "$payload" | jq -r '.transcript_path // empty' 2>/dev/null || true)
    session_id=$(printf '%s' "$payload" | jq -r '.session_id // empty' 2>/dev/null || true)

    if [[ -n "$agent_type" ]]; then
        echo "subagent"
        return 0
    fi

    case "$transcript_path" in
    */subagents/agent-*.jsonl)
        echo "subagent"
        return 0
        ;;
    esac

    if [[ -n "$transcript_path" && -n "$session_id" ]]; then
        case "$transcript_path" in
        */subagents/*) ;; # a subagents/ path is never main-shaped
        *)
            if [[ "${transcript_path##*/}" == "${session_id}.jsonl" ]]; then
                echo "main"
                return 0
            fi
            ;;
        esac
    fi

    if [[ -n "$transcript_path" ]]; then
        loom_debug "DEBUG: Unrecognized transcript_path shape (session_id=$session_id) - not classified as main: $transcript_path"
    fi

    echo "unknown"
    return 0
}

# loom_is_subagent [<payload-json>] - Return 0 when this hook runs under a
# SUBAGENT, non-zero otherwise (main agent, or no live loom session at all).
#
# LOOM-SESSION GATE FIRST, ALWAYS: LOOM_MAIN_AGENT_PID must be set AND a LIVE
# ancestor (a leaked value from a previous session names a PID that is not in
# our chain, and is ignored) before anything else runs - this scopes BOTH
# hooks to a loom stage session. They install globally at
# ~/.claude/hooks/loom/, so without this precondition the payload check below
# would fire for every Claude Code session on the machine: a Task subagent in
# an unrelated, non-loom repo would get `cargo build` / `git commit`
# hard-blocked with no escape hatch. An agent-team teammate is NOT in the main
# agent's process tree, so it correctly keeps returning 1 here too (it is not
# part of a loom session at all).
#
# PAYLOAD-FIRST CLASSIFICATION, ONCE THE GATE PASSES: with a live
# LOOM_MAIN_AGENT_PID ancestor established, a payload argument lets
# `loom_payload_agent_verdict` decide main-vs-subagent outright - "subagent"
# returns 0 immediately (no further process-tree check needed; an in-process
# Task subagent is a subagent regardless of the process tree - it runs inside
# the very claude process LOOM_MAIN_AGENT_PID names, so the gate above is
# trivially satisfied for it too), "main" returns 1 immediately. Only an
# "unknown" verdict (or no payload argument at all - back-compat for callers
# not yet updated) falls through to the process-tree heuristic below.
loom_is_subagent() {
    local payload="${1:-}"

    local main_pid="${LOOM_MAIN_AGENT_PID:-}"
    if [[ -z "$main_pid" ]]; then
        return 1
    fi

    # Stale value (from a previous session) - not in our ancestor chain
    if ! is_ancestor "$main_pid"; then
        loom_debug "DEBUG: LOOM_MAIN_AGENT_PID=$main_pid is NOT in ancestor chain - stale value, ignoring"
        return 1
    fi

    if [[ -n "$payload" ]]; then
        local verdict
        verdict=$(loom_payload_agent_verdict "$payload")
        case "$verdict" in
        subagent)
            loom_debug "DEBUG: Subagent detected via payload (agent_type/transcript_path)"
            return 0
            ;;
        main)
            loom_debug "DEBUG: Main agent detected via payload (transcript_path names the session file)"
            return 1
            ;;
        esac
        loom_debug "DEBUG: Payload verdict unknown - falling back to process-tree heuristic"
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
