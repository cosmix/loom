#!/usr/bin/env bash
if [[ "${_LOOM_PARSER_EXPAND_LOADED:-}" == 1 ]]; then return 0; fi
_LOOM_PARSER_EXPAND_LOADED=1

# _LOOM_SHELL_C_MAX_PAYLOAD - Longest `sh -c` payload worth re-walking. The
# outer walk already costs one bash-level step per character; the bound keeps
# a pathological single argument from multiplying that by the recursion depth.
_LOOM_SHELL_C_MAX_PAYLOAD=16384

# _loom_shell_c_payload_indices - (internal) Echo, one per line, the index into
# LOOM_TOKENS of every `-c` PAYLOAD word whose command segment actually invokes
# a shell. Uses loom_tokens_command_word_index (defined below) so the shell is
# still found behind wrappers and keywords - `timeout 60 bash -c '...'` counts.
# Only the FIRST `-c` of a segment is reported: that is the one bash executes.
_loom_shell_c_payload_indices() {
    local n=${#LOOM_TOKENS[@]}
    local i=0
    local at_cmd_pos=1
    local j k base tok

    while ((i < n)); do
        if [[ "${LOOM_TOKENS[$i]}" == "%%SEP%%" ]]; then
            at_cmd_pos=1
            i=$((i + 1))
            continue
        fi

        if [[ $at_cmd_pos -eq 1 ]]; then
            if j=$(loom_tokens_command_word_index "$i"); then
                base="${LOOM_TOKENS[$j]##*/}"
                case "$base" in
                sh | bash | zsh | dash | ksh)
                    k=$((j + 1))
                    while ((k + 1 < n)) && [[ "${LOOM_TOKENS[$k]}" != "%%SEP%%" ]]; do
                        tok="${LOOM_TOKENS[$k]}"
                        # `-c` and the combined spellings ending in it (`-lc`,
                        # `-xc`) all take the next word as the script to run
                        if [[ "$tok" =~ ^-[A-Za-z]*c$ ]] &&
                            [[ "${LOOM_TOKENS[$((k + 1))]}" != "%%SEP%%" ]]; then
                            echo "$((k + 1))"
                            break
                        fi
                        k=$((k + 1))
                    done
                    ;;
                esac
            fi
        fi

        at_cmd_pos=0
        i=$((i + 1))
    done

    return 0
}

# _loom_expand_shell_c <depth> - (internal) Rewrite LOOM_TOKENS so every
# `sh -c <payload>` payload word is followed by "%%SEP%%" plus the payload's
# own tokens, recursing at most <depth> levels.
#
# Returns 1 whenever a payload is left UNEXPANDED, for either reason: it ended
# inside an unterminated quote, or the budget (recursion depth, payload length)
# ran out with a shell `-c` payload still to expand. loom_tokenize_command
# propagates that, so callers fall back to their conservative raw-regex check
# instead of trusting a half-walked splice.
#
# The budget MUST fail this way rather than returning 0. Returning 0 with a
# payload still opaque both hides the nested command from every loom_tokens_*
# helper AND tells the caller the token list is trustworthy, so no fallback
# runs - a silent bypass of the guard, since the raw regex these helpers
# replaced did match the nested string. Raising the depth number would only
# move the cliff; exceeding whatever bound exists has to fail toward the block.
# The asymmetry is what settles it: rc=1 costs at worst a false positive from a
# stricter regex on a pathologically nested command, rc=0 costs a real bypass.
_loom_expand_shell_c() {
    local depth="$1"

    local indices
    indices=$(_loom_shell_c_payload_indices)
    if [[ -z "$indices" ]]; then
        return 0
    fi

    if ((depth <= 0)); then
        return 1
    fi

    local -a src
    src=()
    if ((${#LOOM_TOKENS[@]} > 0)); then
        src=("${LOOM_TOKENS[@]}")
    fi
    local n=${#src[@]}
    if ((n == 0)); then
        return 0
    fi

    # " 2 5 " - a space-delimited set, since bash 3.2 has no associative arrays
    local marks=" " idx
    for idx in $indices; do
        marks="${marks}${idx} "
    done

    local -a out
    out=()
    local i payload rc=0
    for ((i = 0; i < n; i++)); do
        out+=("${src[$i]}")
        if [[ "$marks" != *" $i "* ]]; then
            continue
        fi
        payload="${src[$i]}"
        if ((${#payload} > _LOOM_SHELL_C_MAX_PAYLOAD)); then
            # The other budget, and the same rule: an unexpanded payload is
            # never reported as a clean parse
            rc=1
            continue
        fi
        LOOM_TOKENS=()
        _loom_tokenize_walk "$payload" || rc=1
        _loom_expand_shell_c $((depth - 1)) || rc=1
        if ((${#LOOM_TOKENS[@]} > 0)); then
            out+=("%%SEP%%")
            out+=("${LOOM_TOKENS[@]}")
            if ((i + 1 < n)) && [[ "${src[$((i + 1))]}" != "%%SEP%%" ]]; then
                out+=("%%SEP%%")
            fi
        fi
    done

    LOOM_TOKENS=()
    if ((${#out[@]} > 0)); then
        LOOM_TOKENS=("${out[@]}")
    fi
    return "$rc"
}
