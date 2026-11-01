#!/usr/bin/env bash
# Shared hook helpers; Bash 3.2 compatible.
if [[ "${_LOOM_IDENTITY_LOADED:-}" == 1 ]]; then return 0; fi
_LOOM_IDENTITY_LOADED=1

# loom_codex_forwarder_verdict <agent_type> <transcript_path> - Decide whether
# a hook payload came from a codex forwarding shim, by the rule
# codex-forward-guard.sh enforces: an authoritative agent_type first, then the
# LOOM-CODEX-FORWARD-ONLY sentinel in the first 200000 bytes of a SUBAGENT
# transcript (a main-session transcript also carries the sentinel inside Agent
# tool payloads, so it never qualifies). Returns 0 for a forwarder, 1 for
# anything else, and 2 when a subagent transcript that would decide it is
# unreadable or a symlink.
loom_codex_forwarder_verdict() {
    local agent_type="$1" transcript_path="$2"
    case "$agent_type" in
    loom-codex-forwarder | codex:codex-rescue) return 0 ;;
    esac
    [[ -n "$agent_type" ]] && return 1
    case "$transcript_path" in
    */subagents/agent-*.jsonl) ;;
    *) return 1 ;;
    esac
    [[ -f "$transcript_path" && -r "$transcript_path" && ! -L "$transcript_path" ]] || return 2
    if LC_ALL=C dd if="$transcript_path" bs=200000 count=1 2>/dev/null | grep -qF 'LOOM-CODEX-FORWARD-ONLY'; then
        return 0
    fi
    return 1
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
# loom_cmdline_is_claude / loom_proc_tree_available are internal helpers -
# hooks should call loom_is_subagent, not these.

# loom_proc_tree_available - Return 0 when a parent-pid lookup can actually be
# performed here, 1 when the environment withholds process information.
#
# Every walk below reads a parent pid from /proc (Linux) or `ps` (macOS). A
# sandbox that withholds process information denies `ps` outright
# ("/bin/ps: Operation not permitted"), so the very first lookup yields
# nothing and the walk stops - which is indistinguishable, at the call site,
# from having walked the whole chain and not found the target. Probing our OWN
# pid separates the two: $$ is always a live process, so an empty answer for it
# means the mechanism is unavailable rather than the chain being short. The
# answer is computed once and cached for the life of the process.
loom_proc_tree_available() {
    if [[ -z "${_LOOM_PROC_TREE_AVAILABLE_CACHE:-}" ]]; then
        local probe=""
        if [[ -r "/proc/$$/stat" ]]; then
            probe=$(awk '{print $4}' "/proc/$$/stat" 2>/dev/null || true)
        else
            probe=$(ps -o ppid= -p "$$" 2>/dev/null | tr -d ' ' || true)
        fi
        if [[ -n "$probe" ]]; then
            _LOOM_PROC_TREE_AVAILABLE_CACHE=1
        else
            _LOOM_PROC_TREE_AVAILABLE_CACHE=0
        fi
    fi
    [[ "$_LOOM_PROC_TREE_AVAILABLE_CACHE" == "1" ]]
}

# is_ancestor - Check if a PID is in our ancestor chain
# Returns 0 if found, 1 if not, 2 if the answer is UNKNOWABLE here (no process
# information available at all - see loom_proc_tree_available). Callers that
# only test truth keep their old behaviour, since 2 is non-zero like 1;
# loom_is_subagent distinguishes them.
is_ancestor() {
    local target_pid="$1"
    local current_pid="$$"

    if ! loom_proc_tree_available; then
        return 2
    fi

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
# part of a loom session at all). Where no process information exists at all
# (`ps` denied by a sandbox, no /proc) ancestry is unknowable, and the gate
# degrades to "the pid must still be signalable" rather than to "no loom
# session" - see the branch on `ancestry -eq 2` below.
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

    local ancestry=0
    is_ancestor "$main_pid" || ancestry=$?

    # Stale value (from a previous session) - not in our ancestor chain
    if [[ $ancestry -eq 1 ]]; then
        loom_debug "DEBUG: LOOM_MAIN_AGENT_PID=$main_pid is NOT in ancestor chain - stale value, ignoring"
        return 1
    fi

    # No process information in this environment, so ancestry is unknowable.
    # Fall back to the strongest proof left - the pid must name a process we
    # can still signal - and treat that as satisfying the gate. A guard that
    # cannot walk the tree must fail CLOSED and keep guarding: returning 1
    # here would silently turn commit-filter.sh and subagent-verify-guard.sh
    # into no-ops for every session whose sandbox withholds `ps`, which is the
    # common case rather than an exotic one. A dead pid is still ignored, so a
    # leaked value from an exited session cannot re-arm the gate.
    if [[ $ancestry -eq 2 ]]; then
        if ! kill -0 "$main_pid" 2>/dev/null; then
            loom_debug "DEBUG: no process information available and LOOM_MAIN_AGENT_PID=$main_pid is not signalable - ignoring"
            return 1
        fi
        loom_debug "DEBUG: no process information available - LOOM_MAIN_AGENT_PID=$main_pid is live, loom-session gate satisfied"
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
