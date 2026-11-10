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
# Exception: subagent-verify-guard.sh blocks runner calls (exit 2) but calls
# loom_warn_no_jq, because it is a deterrent whose own failure must not stop work.
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

# _loom_utf8_cut <text> <max-bytes> - print at most <max-bytes> bytes of
# <text>, dropping the final UTF-8 sequence when the cut left it incomplete,
# so valid UTF-8 in stays valid UTF-8 out. Byte arithmetic needs LC_ALL=C.
_loom_utf8_cut() {
    local LC_ALL=C
    local s="$1" max="$2" len k=0 lead need=1
    if ((${#s} <= max)); then printf '%s' "$s"; return 0; fi
    s="${s:0:max}"
    len=${#s}
    while ((k < 3 && k < len)); do
        lead=$(printf '%d' "'${s:len-1-k:1}")
        ((lead < 0)) && lead=$((lead + 256))
        ((lead >= 128 && lead < 192)) || break
        k=$((k + 1))
    done
    if ((k < len)); then
        lead=$(printf '%d' "'${s:len-1-k:1}")
        ((lead < 0)) && lead=$((lead + 256))
        if ((lead >= 240)); then need=4
        elif ((lead >= 224)); then need=3
        elif ((lead >= 192)); then need=2
        fi
        if ((k + 1 < need)); then s="${s:0:len-1-k}"; fi
    fi
    printf '%s' "$s"
}

# loom_record_block <guard> <rule> <tool> <subject> - append one JSON line to
# "$LOOM_WORK_DIR/heartbeat/$LOOM_STAGE_ID.blocks.jsonl", the stage's block
# ledger (diagnostics only; no orchestrator reader watches that directory for
# *.jsonl). Writes nothing unless LOOM_WORK_DIR is an absolute existing
# directory, LOOM_STAGE_ID matches ^[A-Za-z0-9_-]+$, LOOM_SESSION_ID is set,
# LOOM_CODEX_FORWARD is empty (the codex lane's sandbox denies <repo>/.loom),
# jq is present, the heartbeat directory is a real directory (created mode
# 700 when missing), and the ledger is absent or a regular non-symlink file of
# at most 4 MiB. A FIFO, device or symlink at the ledger path writes nothing,
# so a planted FIFO cannot hang the guard before its exit 2. No lock: one
# printf of at most about 2 KiB to an O_APPEND file lands whole. Never prints;
# always returns 0. role comes from loom_payload_agent_verdict on the caller's
# global INPUT_JSON; subject keeps its first 300 bytes (_loom_utf8_cut).
loom_record_block() {
    local guard="${1:-}" rule="${2:-}" tool="${3:-}" subject="${4:-}"
    local work="${LOOM_WORK_DIR:-}" stage="${LOOM_STAGE_ID:-}" session="${LOOM_SESSION_ID:-}"
    local dir file bytes role=main verdict ts line
    [[ -z "${LOOM_CODEX_FORWARD:-}" && -n "$session" ]] || return 0
    [[ "$work" == /* && -d "$work" && "$stage" =~ ^[A-Za-z0-9_-]+$ ]] || return 0
    command -v jq >/dev/null 2>&1 || return 0
    dir="${work%/}/heartbeat"
    if [[ ! -e "$dir" && ! -L "$dir" ]]; then
        (umask 077 && mkdir -m 700 "$dir") >/dev/null 2>&1 || true
    fi
    [[ -d "$dir" && ! -L "$dir" ]] || return 0
    file="$dir/$stage.blocks.jsonl"
    verdict=$(loom_payload_agent_verdict "${INPUT_JSON:-}" 2>/dev/null) || verdict=""
    [[ "$verdict" != subagent ]] || role=subagent
    ts=$(date -u +%Y-%m-%dT%H:%M:%SZ 2>/dev/null) || ts=""
    subject=$(_loom_utf8_cut "$subject" 300)
    line=$(jq -nc --arg ts "$ts" --arg stage "$stage" --arg session "$session" \
        --arg role "$role" --arg guard "$guard" --arg rule "$rule" --arg tool "$tool" \
        --arg subject "$subject" \
        '{ts:$ts,stage:$stage,session:$session,role:$role,guard:$guard,rule:$rule,tool:$tool,subject:$subject}' \
        2>/dev/null) || return 0
    [[ -n "$line" ]] || return 0
    # File safety, checked last before the append. stat reads the size
    # without opening the file, so a FIFO swapped in cannot block here.
    # Accepted residual: a FIFO swapped in after this check hangs the >> open (bash has no O_NONBLOCK).
    if [[ -e "$file" || -L "$file" ]]; then
        [[ ! -L "$file" && -f "$file" ]] || return 0
        bytes=$(stat -c '%s' "$file" 2>/dev/null || stat -f '%z' "$file" 2>/dev/null || true)
        [[ "$bytes" =~ ^[0-9]+$ ]] && ((bytes <= 4194304)) || return 0
    fi
    (umask 077 && printf '%s\n' "$line" >>"$file") >/dev/null 2>&1 || true
    return 0
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


source "$(dirname "${BASH_SOURCE[0]}")/_parser.sh" || return 1
source "$(dirname "${BASH_SOURCE[0]}")/_parser_expand.sh" || return 1
source "$(dirname "${BASH_SOURCE[0]}")/_parser_scan.sh" || return 1
source "$(dirname "${BASH_SOURCE[0]}")/_identity.sh" || return 1
source "$(dirname "${BASH_SOURCE[0]}")/_stage_lookup.sh" || return 1
source "$(dirname "${BASH_SOURCE[0]}")/_containment.sh" || return 1
source "$(dirname "${BASH_SOURCE[0]}")/_heartbeat.sh" || return 1
_LOOM_COMMON_LOADED=1
