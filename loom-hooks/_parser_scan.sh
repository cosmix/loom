#!/usr/bin/env bash
if [[ "${_LOOM_PARSER_SCAN_LOADED:-}" == 1 ]]; then return 0; fi
_LOOM_PARSER_SCAN_LOADED=1

# --- Token-scanning helpers over LOOM_TOKENS --------------------------------
#
# The seven helpers below all read the global LOOM_TOKENS array populated by
# a prior SUCCESSFUL loom_tokenize_command call (real argv tokens plus the
# literal "%%SEP%%" command-boundary sentinel). They exist so a hook can ask
# "does this command actually INVOKE git/find/grep" or "does some real argv
# VALUE look like a path traversal", instead of regex-matching the raw
# command string - which also matches prose sitting inside one quoted
# argument (a codex-forward task prompt, a `loom memory note` body, a commit
# message) even though no such command was ever invoked there. This mirrors
# git-add-guard.sh's own token walk (scan_git_add_tokens) but generalised
# for reuse across hooks instead of being specific to `git add`.

# loom_token_is_word <token> - Return 0 when <token> is a real "word-shaped"
# argv token: it is NOT the "%%SEP%%" sentinel, and it contains no whitespace.
# The test is the POSIX class `[[:space:]]`, deliberately not a bracket
# expression built from a quoted `$' \t\r\n'`: bash 3.2 treats the quoted
# portions of an `=~` pattern literally and can pull a stray backslash into
# the bracket set, which would misjudge an ordinary word such as `a\b` as
# whitespace-bearing. This is the discriminator that makes the
# path/traversal checks safe: a genuine path, flag, or env-var argument is
# always whitespace-free, while a prose payload passed as one quoted
# argument (loom_tokenize_command strips the quotes but keeps the embedded
# whitespace) is not. Returns 1 for the sentinel or any whitespace-bearing
# token.
loom_token_is_word() {
    local tok="$1"
    [[ "$tok" == "%%SEP%%" ]] && return 1
    [[ "$tok" =~ [[:space:]] ]] && return 1
    return 0
}

# _loom_wrapper_flag_takes_arg <wrapper> <flag> - (internal helper) Return 0
# when <flag>, as spelled by <wrapper>, consumes the FOLLOWING word as its
# value, so command-word resolution has to step over both. Without this,
# `nice -n 10 git commit` resolves to `10` and every git guard misses it.
# Only spellings that occur in real commands are listed; an unlisted flag is
# treated as self-contained, which at worst stops the unwrap one word early -
# the conservative direction for a resolver.
_loom_wrapper_flag_takes_arg() {
    case "$1:$2" in
    env:-u | nice:-n | exec:-a | doas:-u | doas:-C | \
        xargs:-n | xargs:-I | xargs:-P | xargs:-L | xargs:-s | \
        timeout:-s | timeout:-k | gtimeout:-s | gtimeout:-k | \
        stdbuf:-i | stdbuf:-o | stdbuf:-e)
        return 0
        ;;
    esac
    return 1
}

# loom_tokens_command_word_index <start-index> - (internal helper) Echo the
# index into LOOM_TOKENS of the EFFECTIVE command word for the command
# segment beginning at <start-index>, which must sit at a COMMAND POSITION
# (index 0, or immediately after a "%%SEP%%" sentinel). Returns 1 (nothing
# echoed) when that segment has no command word before its "%%SEP%%" or the
# array ends.
#
# Resolution walks forward, skipping everything that is not yet the command:
#   1. VAR=value environment assignments (^[A-Za-z_][A-Za-z0-9_]*=), the same
#      env-skip scan_git_add_tokens does at git-add-guard.sh:115-117.
#   2. TRANSPARENT shell keywords and grouping words - if then elif else do
#      while until ! { } fi done. These occupy argv[0] without being the
#      command: `if git commit -m x; then :; fi` puts `if` first, and a
#      resolver that stopped there would report the segment as invoking `if`
#      and wave every git guard through.
#   3. Wrapper commands, matched on the BASENAME (everything after the last
#      "/") - sudo doas env xargs time nohup command exec builtin setsid nice
#      stdbuf timeout gtimeout. These are COMMAND PREFIXES: each runs the rest
#      of the words as a command, so the real command sits behind them.
#      `exec git commit` genuinely runs git - the shell is REPLACED by it - so
#      an `exec` the resolver stops at hides the invocation from every guard.
#      Step past the wrapper, then past that wrapper's own option words
#      (tokens starting with "-", and VAR=value tokens), consuming the
#      following word too for the flags that take one
#      (_loom_wrapper_flag_takes_arg: `env -u NAME`, `nice -n 10`,
#      `exec -a NAME`, `doas -u NAME`, `xargs -I {}`, ...), then repeat the
#      whole check on whatever word is left. This unwinds chained wrappers
#      such as `sudo env FOO=bar timeout 5 git commit` down to `git`.
#      `timeout`/`gtimeout` additionally consume one further non-option word
#      for the DURATION, so `timeout 60 git commit` resolves to `git`, not to
#      `60`.
#
#      `eval` is deliberately ABSENT from this set and from the keyword set
#      above. It is not resolved through at all: commit-filter.sh and
#      worktree-isolation.sh detect `eval` as its own risk signal (its
#      argument is a string the guard cannot see into), and making it
#      transparent would silently disable those checks.
#   4. Stop at "%%SEP%%" or the end of the array. No skip may step OVER a
#      "%%SEP%%": an arg-taking flag at a segment boundary (`env -u; git
#      commit`) has to end this segment with no command word, rather than
#      reach into the next command and report ITS command word as this
#      segment's.
#
# Still deliberately modest, not a full shell parser: an arg-taking wrapper
# flag that _loom_wrapper_flag_takes_arg does not list stops the unwrap at
# that flag's value - the same limitation scan_git_add_tokens already accepts
# for git's own "-C <dir>" handling.
loom_tokens_command_word_index() {
    local i=$1
    local n=${#LOOM_TOKENS[@]}
    local tok base opt wrapper

    while ((i < n)) && [[ "${LOOM_TOKENS[$i]}" != "%%SEP%%" ]]; do
        tok="${LOOM_TOKENS[$i]}"

        if [[ "$tok" =~ ^[A-Za-z_][A-Za-z0-9_]*= ]]; then
            i=$((i + 1))
            continue
        fi

        case "$tok" in
        if | then | elif | else | do | while | until | '!' | '{' | '}' | fi | done)
            i=$((i + 1))
            continue
            ;;
        esac

        base="${tok##*/}"
        case "$base" in
        sudo | doas | env | xargs | time | nohup | command | exec | builtin | setsid | nice | stdbuf | timeout | gtimeout)
            wrapper="$base"
            i=$((i + 1))
            while ((i < n)); do
                opt="${LOOM_TOKENS[$i]}"
                [[ "$opt" == "%%SEP%%" ]] && break
                if _loom_wrapper_flag_takes_arg "$wrapper" "$opt"; then
                    i=$((i + 1))
                    if ((i < n)) && [[ "${LOOM_TOKENS[$i]}" != "%%SEP%%" ]]; then
                        i=$((i + 1))
                    fi
                    continue
                fi
                if [[ "$opt" == -* || "$opt" =~ ^[A-Za-z_][A-Za-z0-9_]*= ]]; then
                    i=$((i + 1))
                    continue
                fi
                break
            done
            if [[ "$wrapper" == "timeout" || "$wrapper" == "gtimeout" ]]; then
                if ((i < n)) && [[ "${LOOM_TOKENS[$i]}" != "%%SEP%%" ]]; then
                    i=$((i + 1))
                fi
            fi
            continue
            ;;
        *)
            break
            ;;
        esac
    done

    if ((i >= n)) || [[ "${LOOM_TOKENS[$i]}" == "%%SEP%%" ]]; then
        return 1
    fi

    echo "$i"
    return 0
}

# loom_tokens_invoke <basename-ere> - Return 0 when some command segment's
# EFFECTIVE command word (loom_tokens_command_word_index, after wrapper
# unwrapping) has a BASENAME matching <basename-ere>. The pattern is
# ANCHORED here as "^(<basename-ere>)$" - callers pass "git", not "^git$".
# Returns 1 when no segment's command word matches.
loom_tokens_invoke() {
    local pattern="$1"
    local re="^(${pattern})$"
    local n=${#LOOM_TOKENS[@]}
    local i=0
    local at_cmd_pos=1
    local j base

    while ((i < n)); do
        if [[ "${LOOM_TOKENS[$i]}" == "%%SEP%%" ]]; then
            at_cmd_pos=1
            i=$((i + 1))
            continue
        fi

        if [[ $at_cmd_pos -eq 1 ]]; then
            if j=$(loom_tokens_command_word_index "$i"); then
                base="${LOOM_TOKENS[$j]##*/}"
                [[ "$base" =~ $re ]] && return 0
            fi
        fi

        at_cmd_pos=0
        i=$((i + 1))
    done

    return 1
}

# loom_tokens_cmd_has_arg <basename-ere> <arg-ere> - Return 0 when some
# segment invoking <basename-ere> (per loom_tokens_invoke's matching rule)
# has a LATER argv word - after the effective command word, before that
# segment's next "%%SEP%%" - fully matching <arg-ere>, anchored the same way
# as the basename pattern. Returns 1 otherwise.
loom_tokens_cmd_has_arg() {
    local cmd_pattern="$1" arg_pattern="$2"
    local cmd_re="^(${cmd_pattern})$" arg_re="^(${arg_pattern})$"
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
                if [[ "$base" =~ $cmd_re ]]; then
                    k=$((j + 1))
                    while ((k < n)) && [[ "${LOOM_TOKENS[$k]}" != "%%SEP%%" ]]; do
                        tok="${LOOM_TOKENS[$k]}"
                        [[ "$tok" =~ $arg_re ]] && return 0
                        k=$((k + 1))
                    done
                fi
            fi
        fi

        at_cmd_pos=0
        i=$((i + 1))
    done

    return 1
}

# loom_tokens_cmd_has_arg_pair <basename-ere> <first-ere> <second-ere> - As
# loom_tokens_cmd_has_arg, but requires two ADJACENT argv words - both still
# before the invoking segment's next "%%SEP%%" - matching <first-ere> then
# <second-ere> in that order. Returns 1 when no segment has such a pair.
loom_tokens_cmd_has_arg_pair() {
    local cmd_pattern="$1" first_pattern="$2" second_pattern="$3"
    local cmd_re="^(${cmd_pattern})$"
    local first_re="^(${first_pattern})$"
    local second_re="^(${second_pattern})$"
    local n=${#LOOM_TOKENS[@]}
    local i=0
    local at_cmd_pos=1
    local j k base

    while ((i < n)); do
        if [[ "${LOOM_TOKENS[$i]}" == "%%SEP%%" ]]; then
            at_cmd_pos=1
            i=$((i + 1))
            continue
        fi

        if [[ $at_cmd_pos -eq 1 ]]; then
            if j=$(loom_tokens_command_word_index "$i"); then
                base="${LOOM_TOKENS[$j]##*/}"
                if [[ "$base" =~ $cmd_re ]]; then
                    k=$((j + 1))
                    while ((k + 1 < n)) &&
                        [[ "${LOOM_TOKENS[$k]}" != "%%SEP%%" ]] &&
                        [[ "${LOOM_TOKENS[$((k + 1))]}" != "%%SEP%%" ]]; do
                        if [[ "${LOOM_TOKENS[$k]}" =~ $first_re && "${LOOM_TOKENS[$((k + 1))]}" =~ $second_re ]]; then
                            return 0
                        fi
                        k=$((k + 1))
                    done
                fi
            fi
        fi

        at_cmd_pos=0
        i=$((i + 1))
    done

    return 1
}

# loom_tokens_cmd_argv <basename-ere> <n> <arg-ere> - Return 0 when some
# segment invoking <basename-ere> has argv[<n>] fully matching <arg-ere>,
# anchored the same way as the basename pattern, counting the EFFECTIVE
# command word itself (post wrapper-unwrapping) as argv[0]. Every token from
# argv[0] through argv[<n>] must exist and stay inside the same segment (no
# "%%SEP%%" crossed) or the segment does not count. Returns 1 when no
# segment matches.
loom_tokens_cmd_argv() {
    local cmd_pattern="$1" argv_index="$2" arg_pattern="$3"
    local cmd_re="^(${cmd_pattern})$" arg_re="^(${arg_pattern})$"
    local n=${#LOOM_TOKENS[@]}
    local i=0
    local at_cmd_pos=1
    local j k m base crossed

    while ((i < n)); do
        if [[ "${LOOM_TOKENS[$i]}" == "%%SEP%%" ]]; then
            at_cmd_pos=1
            i=$((i + 1))
            continue
        fi

        if [[ $at_cmd_pos -eq 1 ]]; then
            if j=$(loom_tokens_command_word_index "$i"); then
                base="${LOOM_TOKENS[$j]##*/}"
                if [[ "$base" =~ $cmd_re ]]; then
                    k=$((j + argv_index))
                    crossed=0
                    for ((m = j + 1; m <= k; m++)); do
                        if ((m >= n)) || [[ "${LOOM_TOKENS[$m]}" == "%%SEP%%" ]]; then
                            crossed=1
                            break
                        fi
                    done
                    if [[ $crossed -eq 0 ]] && [[ "${LOOM_TOKENS[$k]}" =~ $arg_re ]]; then
                        return 0
                    fi
                fi
            fi
        fi

        at_cmd_pos=0
        i=$((i + 1))
    done

    return 1
}

# loom_tokens_word_matches <ere> - Return 0 when some token satisfying
# loom_token_is_word matches <ere> UNANCHORED (callers anchor with ^/$
# themselves when they want to). Returns 1 when no word-shaped token
# matches - in particular, a token that came from one quoted argument
# containing whitespace (a task prompt, a memory note body) never counts,
# no matter what substring it contains.
loom_tokens_word_matches() {
    local pattern="$1"
    local n=${#LOOM_TOKENS[@]}
    local i tok

    for ((i = 0; i < n; i++)); do
        tok="${LOOM_TOKENS[$i]}"
        loom_token_is_word "$tok" || continue
        [[ "$tok" =~ $pattern ]] && return 0
    done

    return 1
}
