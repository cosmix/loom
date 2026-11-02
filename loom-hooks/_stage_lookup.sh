#!/usr/bin/env bash
if [[ "${_LOOM_STAGE_LOOKUP_LOADED:-}" == 1 ]]; then return 0; fi
_LOOM_STAGE_LOOKUP_LOADED=1

# loom_find_stage_file <work-dir> <stage-id>
#
# Echo the one canonical stage document matching Rust's `{depth}-{id}.md` or
# legacy `{id}.md` naming. Return 0 when exactly one regular, non-symlink file
# matches; 1 when none matches; 2 for unsafe input, a matching symlink, or
# ambiguity. Callers that make ownership decisions must distinguish 2 from a
# genuinely absent record and fail closed.
loom_find_stage_file() {
    local work_dir="$1" stage_id="$2"
    local stages_dir="${work_dir}/stages"
    local exact="" candidate="" basename="" prefix="" unsafe=0
    local matches=()

    case "$stage_id" in
    *[!A-Za-z0-9_-]* | "")
        loom_debug "stage lookup: unsafe stage id '$stage_id'"
        return 2
        ;;
    esac
    [[ -d "$stages_dir" && ! -L "$stages_dir" ]] || return 1

    exact="${stages_dir}/${stage_id}.md"
    if [[ -L "$exact" ]]; then
        unsafe=1
    elif [[ -f "$exact" ]]; then
        matches+=("$exact")
    fi

    for candidate in "$stages_dir"/[0-9]*-"$stage_id".md; do
        [[ -e "$candidate" || -L "$candidate" ]] || continue
        basename="${candidate##*/}"
        prefix="${basename%-${stage_id}.md}"
        case "$prefix" in
        "" | *[!0-9]*) continue ;;
        esac
        if [[ -L "$candidate" ]]; then
            unsafe=1
        elif [[ -f "$candidate" ]]; then
            matches+=("$candidate")
        fi
    done

    if [[ "$unsafe" -ne 0 || "${#matches[@]}" -gt 1 ]]; then
        loom_debug "stage lookup: unsafe or ambiguous match for '$stage_id'"
        return 2
    fi
    if [[ "${#matches[@]}" -eq 1 ]]; then
        printf '%s\n' "${matches[0]}"
        return 0
    fi
    return 1
}

# Resolve the nearest workspace state directory from a given directory.
loom_stage_work_dir() {
    local dir
    dir="${1:-$PWD}"
    # A relative path never reaches `/`, so the walk below would not end.
    [[ "$dir" == /* ]] || return 1
    dir=${dir%/}; [[ -n "$dir" ]] || dir=/
    while :; do
        if [[ -d "$dir/.loom/work" ]]; then printf '%s\n' "$dir/.loom/work"; return 0; fi
        if [[ -d "$dir/.work" ]]; then printf '%s\n' "$dir/.work"; return 0; fi
        [[ "$dir" != / ]] || return 1
        dir=${dir%/*}; [[ -n "$dir" ]] || dir=/
    done
}

# Read exactly one top-level frontmatter field. Bodies and incomplete or
# duplicate fields cannot supply stage metadata to any hook.
loom_stage_frontmatter_field() {
    local file="$1" field="$2" line="" value="" count=0 closed=0 first=1
    [[ -f "$file" && ! -L "$file" ]] || return 1
    [[ "$field" =~ ^[a-z][a-z0-9_-]*$ ]] || return 1
    while IFS= read -r line || [[ -n "$line" ]]; do
        if ((first)); then
            [[ "$line" == --- ]] || return 1
            first=0; continue
        fi
        if [[ "$line" == --- ]]; then closed=1; break; fi
        case "$line" in
        "$field":*)
            count=$((count + 1)); value=${line#*:}
            value="${value#"${value%%[![:space:]]*}"}"
            value="${value%"${value##*[![:space:]]}"}"
            ;;
        esac
    done <"$file"
    [[ $closed -eq 1 && $count -eq 1 ]] || return 1
    printf '%s\n' "$value"
}

# Reject incomplete frontmatter and duplicate statuses, including body prose.
loom_stage_status() {
    local status=""
    status=$(loom_stage_frontmatter_field "$1" status) || return 1
    status=${status%%#*}
    status="${status%"${status##*[![:space:]]}"}"
    case "$status" in
    \"*\") status=${status#\"}; status=${status%\"} ;;
    \'*\') status=${status#\'}; status=${status%\'} ;;
    esac
    [[ "$status" =~ ^[A-Za-z][A-Za-z0-9-]*$ ]] || return 1
    printf '%s\n' "$status"
}
