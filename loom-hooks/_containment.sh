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

# _loom_owned_by_uid <path> - <path> exists, is not a symlink, and belongs to
# the current uid. A copy of worktree-file-guard.sh's owned_by_uid: helpers
# never source a guard.
_loom_owned_by_uid() {
    local owner
    [[ -e "$1" && ! -L "$1" ]] || return 1
    owner=$(stat -c '%u' "$1" 2>/dev/null || stat -f '%u' "$1" 2>/dev/null || true)
    [[ -n "$owner" && "$owner" == "$(id -u)" ]]
}

# _loom_physical_dir <dir> - print the physical form of the existing directory
# <dir>, or return 1 when `cd -P` fails. The trailing `x` keeps a directory
# name that ends in a newline intact through the command substitution.
_loom_physical_dir() {
    local out
    out=$(cd -P -- "$1" 2>/dev/null && pwd -P && printf x) || return 1
    out="${out%x}"
    out="${out%?}"
    [[ "$out" == /* ]] || return 1
    printf '%s' "$out"
}

# _loom_physical_prefix <abs> - make the longest existing directory prefix of
# the absolute path <abs> physical and keep the missing tail lexical.
_loom_physical_prefix() {
    local head="$1" tail="" phys
    while [[ -n "$head" && ! -d "$head" ]]; do
        tail="/${head##*/}${tail}"
        head="${head%/*}"
    done
    phys=$(_loom_physical_dir "${head:-/}") || return 1
    phys="${phys%/}${tail}"
    printf '%s' "${phys:-/}"
}

# _loom_under <path> <root> - <path> is <root> or below it, at a component
# boundary, so /a/wt-x is not under /a/wt.
_loom_under() {
    [[ -n "$2" ]] || return 1
    [[ "$2" == / || "$1" == "$2" || "$1" == "$2"/* ]]
}

# _loom_scratch_root - print the physical LOOM_SCRATCH_DIR when it is
# absolute, an existing directory, not a symlink, and owned by `id -u`; else
# return 1. TMPDIR is never a scratch root (see the plan's common.md).
_loom_scratch_root() {
    local dir="${LOOM_SCRATCH_DIR:-}" phys
    while [[ "$dir" == */ && "$dir" != / ]]; do dir="${dir%/}"; done
    [[ "$dir" == /* && "$dir" != / && -d "$dir" && ! -L "$dir" ]] || return 1
    _loom_owned_by_uid "$dir" || return 1
    phys=$(_loom_physical_dir "$dir") || return 1
    [[ "$phys" != / ]] || return 1
    printf '%s' "$phys"
}

# loom_normalize_path <base> <path> - print the absolute physical path <path>
# names when run from <base>. A relative <path> joins <base>; `~` and `~/x`
# expand to $HOME; a leading `$LOOM_SCRATCH_DIR` or `${LOOM_SCRATCH_DIR}`
# expands to this hook's LOOM_SCRATCH_DIR only while LOOM_SCRATCH_EXPAND=1
# (unset counts as 0) and _loom_scratch_root accepts it. Empty and `.`
# components drop; before a `..` pops a component the prefix built so far is
# made physical when it is an existing directory, so `<wt>/.loom/work/../x`
# (work is a symlink into the main checkout) is `<repo>/.loom/x`. Afterwards
# the longest existing directory prefix is made physical; a missing tail stays
# lexical. Returns 1 for `~user`, any other `$` or a backtick, a relative
# <path> with an empty or relative <base>, and a failing `cd -P`.
loom_normalize_path() {
    local base="$1" path="$2" head="" tail="$2" full built="" rest comp phys
    local v1='$LOOM_SCRATCH_DIR' v2='${LOOM_SCRATCH_DIR}'
    case "$path" in *'`'*) return 1 ;; esac
    case "$path" in
    '~' | '~/'*) head="${HOME:-}" tail="${path:1}" ;;
    '~'*) return 1 ;;
    "$v1" | "$v1"/*) tail="${path#"$v1"}" ;;
    "$v2" | "$v2"/*) tail="${path#"$v2"}" ;;
    esac
    if [[ "$tail" != "$path" && "$path" != '~'* ]]; then
        [[ "${LOOM_SCRATCH_EXPAND:-0}" == 1 ]] || return 1
        head=$(_loom_scratch_root) || return 1
    fi
    case "$tail" in *'$'*) return 1 ;; esac
    if [[ "$tail" != "$path" ]]; then
        [[ "$head" == /* ]] || return 1
        full="${head}${tail}"
    elif [[ "$path" == /* ]]; then
        full="$path"
    else
        [[ "$base" == /* ]] || return 1
        full="${base}/${path}"
    fi
    rest="$full"
    while [[ -n "$rest" ]]; do
        comp="${rest%%/*}"
        if [[ "$rest" == */* ]]; then rest="${rest#*/}"; else rest=""; fi
        case "$comp" in
        '' | .) ;;
        ..)
            if [[ -d "${built:-/}" ]]; then
                phys=$(_loom_physical_dir "${built:-/}") || return 1
                built="${phys%/}"
            fi
            built="${built%/*}"
            ;;
        *) built="${built}/${comp}" ;;
        esac
    done
    _loom_physical_prefix "${built:-/}"
}

# loom_normalize_word <base> <word> - loom_normalize_path for a token naming a
# git target, scratch exemption or cd: the tokenizer keeps glob, brace and escape
# text that bash expands, so `\ * ? [ {` (past ${LOOM_SCRATCH_DIR}) returns 1.
loom_normalize_word() {
    local v='${LOOM_SCRATCH_DIR}'
    case "${2#"$v"}" in *'\'* | *'*'* | *'?'* | *'['* | *'{'*) return 1 ;; esac
    loom_normalize_path "$1" "$2"
}

# _loom_physical_or_lexical <abs> - _loom_physical_prefix, else <abs> as is.
_loom_physical_or_lexical() {
    _loom_physical_prefix "$1" 2>/dev/null || printf '%s' "$1"
}

# loom_path_class <abs-physical-path> - print exactly one of state, sibling,
# worktree, scratch, outside; the first match in that order wins:
#   state     under the main checkout's .loom/work, .work, or .git
#   sibling   under <repo>/.worktrees/<other-id>
#   worktree  under the current worktree root (loom_current_worktree)
#   scratch   under _loom_scratch_root, unless that root lies inside the repo
#   outside   anything else, the main checkout's own files included
# The repo is the parent of the current worktree's `.worktrees/`; with no
# current worktree only scratch and outside are possible. A relative argument
# prints outside.
loom_path_class() {
    local abs="$1" wt="" wt_phys="" repo="" root rel id scratch
    if [[ "$abs" != /* ]]; then printf 'outside'; return 0; fi
    if wt=$(loom_current_worktree 2>/dev/null); then
        while [[ "$wt" == */ && "$wt" != / ]]; do wt="${wt%/}"; done
        repo=$(_loom_physical_or_lexical "${wt%/.worktrees/*}")
        wt_phys=$(_loom_physical_or_lexical "$wt")
        for root in "$repo/.loom/work" "$repo/.work" "$repo/.git"; do
            if _loom_under "$abs" "$root" ||
                _loom_under "$abs" "$(_loom_physical_or_lexical "$root")"; then
                printf 'state'; return 0
            fi
        done
        root=$(_loom_physical_or_lexical "$repo/.worktrees")
        if [[ "$abs" == "$root"/* ]]; then
            rel="${abs#"$root"/}"
            id="${rel%%/*}"
            if [[ -n "$id" && "$id" != "${wt##*/}" && "$root/$id" != "$wt_phys" ]]; then
                printf 'sibling'; return 0
            fi
        fi
        if _loom_under "$abs" "$wt_phys"; then printf 'worktree'; return 0; fi
    fi
    if scratch=$(_loom_scratch_root) && _loom_under "$abs" "$scratch" &&
        { [[ -z "$repo" ]] || ! _loom_under "$scratch" "$repo"; }; then
        printf 'scratch'; return 0
    fi
    printf 'outside'
}

# _loom_scratch_mention_unsafe <token> - 0 when <token> names LOOM_SCRATCH_DIR
# other than as a plain `$LOOM_SCRATCH_DIR` (next character not a name
# character) or `${LOOM_SCRATCH_DIR}` read. Stricter than "not preceded by $
# or ${": `${LOOM_SCRATCH_DIR:=x}` assigns, so it counts as unsafe too.
_loom_scratch_mention_unsafe() {
    local rest="$1" before after
    while [[ "$rest" == *LOOM_SCRATCH_DIR* ]]; do
        before="${rest%%LOOM_SCRATCH_DIR*}"
        after="${rest#*LOOM_SCRATCH_DIR}"
        case "$before" in
        *'${') [[ "$after" == '}'* ]] || return 0 ;;
        *'$') [[ "$after" != [A-Za-z0-9_]* ]] || return 0 ;;
        *) return 0 ;;
        esac
        rest="$after"
    done
    return 1
}

# _loom_scratch_expand_allowed <n> - 0 unless a token of LOOM_TOKENS[0..n)
# could reassign LOOM_SCRATCH_DIR (common.md, rule for LOOM_SCRATCH_EXPAND).
_loom_scratch_expand_allowed() {
    local n="$1" i tok
    for ((i = 0; i < n; i++)); do
        tok="${LOOM_TOKENS[$i]}"
        case "$tok" in
        eval | env | declare | typeset | export | local | readonly | read | unset | set | source | . | printf | mapfile | readarray) return 1 ;;
        esac
        if _loom_scratch_mention_unsafe "$tok"; then return 1; fi
    done
    return 0
}

# _loom_cwd_segment <start> <end> <prev-seps> <next-seps> - apply the segment
# LOOM_TOKENS[start..end) to loom_track_cwd's locals _tc_cwd, _tc_dead and
# _tc_unverified (bash dynamic scope). <prev-seps> is `^` at the command start
# and <next-seps> is `$` at its end. A segment holding a cd, pushd or popd
# word anywhere (`{ cd x`, `if cd x`), or starting with eval, source or `.`,
# changes directory; only `cd <word>` has a known result. A cd behind `&&`
# may not run, so it counts as unverified; behind `||`, `|`, `&` or a
# redirection its effect is unknown, as after any earlier token naming CDPATH
# (_tc_cdpath). Known limit: a CDPATH inherited from the shell is not seen.
_loom_cwd_segment() {
    local s="$1" e="$2" prev="$3" next="$4" k target nl=$'\n' changes=0
    for ((k = s; k < e; k++)); do
        case "${LOOM_TOKENS[$k]}" in cd | pushd | popd) changes=1 ;; esac
    done
    if ((e > s)); then
        case "${LOOM_TOKENS[$s]}" in eval | source | .) changes=1 ;; esac
    fi
    if ((changes == 0 || _tc_dead)); then return 0; fi
    _tc_dead=1
    ((_tc_taint == 0 && _tc_cdpath == 0 && e - s == 2)) || return 0
    [[ "${LOOM_TOKENS[$s]}" == cd ]] || return 0
    target="${LOOM_TOKENS[$((s + 1))]}"
    loom_token_is_word "$target" || return 0
    [[ "$target" != -* ]] || return 0
    target=$(loom_normalize_word "$_tc_cwd" "$target") || return 0
    prev="${prev//;/}" prev="${prev//$nl/}"
    case "$prev" in
    '' | '^') ;;
    '&&') _tc_unverified=1 ;;
    *) return 0 ;;
    esac
    if [[ "$next" == '&&' ]]; then
        [[ -d "$target" ]] || _tc_unverified=1
    elif [[ "$next" != '$' ]]; then
        next="${next//;/}" next="${next//$nl/}"
        [[ -z "$next" && "$4" != '' && -d "$target" ]] || return 0
    fi
    _tc_cwd="$target"
    _tc_dead=0
}

# loom_track_cwd [<start-dir>] - after a successful loom_tokenize_command,
# fill LOOM_TOKEN_CWD so LOOM_TOKEN_CWD[i] is the absolute physical working
# directory token i runs in, or "" when it cannot be known (sentinels always
# get ""). Starts from <start-dir> made physical when it is an absolute
# existing directory (callers pass the payload's .cwd), else from `pwd -P`.
# Also sets LOOM_SCRATCH_EXPAND to 1 or 0. Rules: common.md, interface 1.
# Taint: a sentinel holding `(`, `)` or a backtick, or empty (`sh -c` splice,
# bare `$(`), makes every segment after the first directory change unknown.
loom_track_cwd() {
    LOOM_TOKEN_CWD=()
    LOOM_SCRATCH_EXPAND=0
    local start="${1:-}" n=0 i seg=0 seps prev='^' next
    local _tc_cwd="" _tc_dead=0 _tc_unverified=0 _tc_taint=0 _tc_cdpath=0
    if [[ -n "${LOOM_TOKENS[*]+x}" ]]; then n=${#LOOM_TOKENS[@]}; fi
    if _loom_scratch_expand_allowed "$n"; then LOOM_SCRATCH_EXPAND=1; fi
    if [[ "$start" == /* && -d "$start" ]]; then
        _tc_cwd=$(_loom_physical_dir "$start") || _tc_cwd=""
    else
        _tc_cwd=$(pwd -P 2>/dev/null) || _tc_cwd=""
    fi
    for ((i = 0; i < n; i++)); do
        [[ "${LOOM_TOKENS[$i]}" == "%%SEP%%" ]] || continue
        case "${LOOM_TOKEN_SEPS[$i]:-}" in '' | *'('* | *')'* | *'`'*) _tc_taint=1 ;; esac
    done
    for ((i = 0; i <= n; i++)); do
        if ((i < n)) && [[ "${LOOM_TOKENS[$i]}" != "%%SEP%%" ]]; then
            [[ "${LOOM_TOKENS[$i]}" != *CDPATH* ]] || _tc_cdpath=1
            if ((_tc_dead)); then LOOM_TOKEN_CWD[$i]=""; else LOOM_TOKEN_CWD[$i]="$_tc_cwd"; fi
            continue
        fi
        next='$'
        if ((i < n)); then next="${LOOM_TOKEN_SEPS[$i]:-}"; fi
        _loom_cwd_segment "$seg" "$i" "$prev" "$next"
        if ((i < n)); then
            LOOM_TOKEN_CWD[$i]=""
            if ((_tc_unverified)) && [[ "$next" != '&&' ]]; then _tc_dead=1; fi
        fi
        prev="$next"
        seg=$((i + 1))
    done
    return 0
}
