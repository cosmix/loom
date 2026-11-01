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
