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
    if (match($0, /<<-?[[:space:]]*["\047]?[A-Za-z_][A-Za-z0-9_]*["\047]?/)) {
        s = substr($0, RSTART, RLENGTH)
        # Remove << prefix, optional dash, whitespace, quotes
        sub(/^<<-?[[:space:]]*["\047]?/, "", s)
        sub(/["\047]?$/, "", s)
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
# resolved, exactly as bash would pass it in argv, and `$'...'` / `$"..."`
# quoting drops its leading `$` the way bash does (`git $'commit'` really
# does run `git commit`).
#
# A substitution opened from INSIDE a double-quoted string RESTORES the
# quote when it closes: `echo "today is $(date)"` tokenizes cleanly, because
# the walk stacks the pre-substitution state and pops it at the matching `)`
# or closing backtick. Getting that wrong is not a cosmetic parse bug - a
# failed tokenize sends every caller back to its raw-regex fallback, which
# is exactly the false-positive behaviour these helpers exist to remove.
#
# `sh -c <payload>` payloads are tokenized RECURSIVELY (bounded at two
# levels) and spliced into LOOM_TOKENS immediately after the payload word,
# preceded by a "%%SEP%%" so the spliced content starts at a command
# position. Without that, `bash -c 'git commit -m wip'` hands every helper
# one opaque whitespace-bearing word and nothing can see the `git commit`
# inside it. The payload word itself is KEPT, so anything that already
# matched on it keeps matching. Only a segment whose EFFECTIVE command word
# is a shell (sh/bash/zsh/dash/ksh) is expanded, so a task brief passed to
# any other command - `codex-forward.sh task '<brief>' --model ...` - stays
# exactly ONE token. Nesting DEEPER than the bound returns 1 rather than a
# half-expanded token list; see _loom_expand_shell_c for why the budget has
# to fail toward the block.
#
# CAVEAT on the sentinel: "%%SEP%%" is pushed only from the separator
# branches below and is never appended to `token`, so a sentinel is never
# tagged onto a real token. It is NOT unforgeable in the other direction,
# though: an argv word that is literally `%%SEP%%` (`echo %%SEP%%`) is
# indistinguishable from a genuine boundary and will be read as one. That
# residual is accepted - a caller that must not be fooled by a self-chosen
# argument value cannot rely on segment boundaries alone.
#
# Arguments:
#   $1 - The command string to tokenize
#
# Output:
#   Populates the global array LOOM_TOKENS as a side effect (no stdout
#   output of its own). Returns 0 on a clean parse - the walk, and every
#   `sh -c` payload walk it spliced, ended back in the `plain` state with no
#   payload left unexpanded. Returns 1 if any of them ends inside an
#   unterminated quote, or if a payload was left unexpanded because a budget
#   ran out; callers should treat either as "could not tokenize" and fall
#   back to a more conservative check rather than trust a partial
#   LOOM_TOKENS.
#
# Bash 3.2+ compatible: no associative arrays, no `${arr[-1]}` negative
# indexing, no `declare -n` namerefs.
loom_tokenize_command() {
    local rc=0
    _loom_tokenize_walk "$1" || rc=1
    _loom_expand_shell_c 2 || rc=1
    return "$rc"
}

# _loom_tokenize_walk <command-string> - (internal) The quote/separator state
# machine behind loom_tokenize_command. Repopulates LOOM_TOKENS from scratch
# and returns 1 when the string ends inside an unterminated quote. Does NOT
# expand `sh -c` payloads; loom_tokenize_command layers that on top.
_loom_tokenize_walk() {
    local input="$1"
    local state=plain
    local token=""
    local started=0
    local last_was_sep=0
    local length=${#input}
    local i char next entry
    # Substitutions currently open, innermost last. Each entry is
    # "<kind>:<state>": the kind the close must match (`p` for `(` / `$(`,
    # `b` for a backtick) and the state to restore when it does. Bash 3.2 has
    # no negative indexing, so the top lives at subst_stack[subst_depth - 1].
    local -a subst_stack
    subst_stack=()
    local subst_depth=0

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
                case "$char" in
                '(')
                    subst_stack[$subst_depth]="p:plain"
                    subst_depth=$((subst_depth + 1))
                    ;;
                ')')
                    if ((subst_depth > 0)); then
                        entry="${subst_stack[$((subst_depth - 1))]}"
                        if [[ "${entry%%:*}" == "p" ]]; then
                            subst_depth=$((subst_depth - 1))
                            state="${entry#*:}"
                        fi
                    fi
                    ;;
                '`')
                    entry=""
                    if ((subst_depth > 0)); then
                        entry="${subst_stack[$((subst_depth - 1))]}"
                    fi
                    if [[ "${entry%%:*}" == "b" ]]; then
                        subst_depth=$((subst_depth - 1))
                        state="${entry#*:}"
                    else
                        subst_stack[$subst_depth]="b:plain"
                        subst_depth=$((subst_depth + 1))
                    fi
                    ;;
                esac
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
                next="${input:$((i + 1)):1}"
                case "$next" in
                '(')
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
                    subst_stack[$subst_depth]="p:plain"
                    subst_depth=$((subst_depth + 1))
                    i=$((i + 1))
                    ;;
                "'" | '"')
                    # ANSI-C ($'...') and locale ($"...") quoting: bash drops
                    # the `$` and passes the quoted body through as the value.
                    # Consume the `$` WITHOUT appending it and let the quote
                    # char open its own state on the next iteration, so
                    # `git $'commit'` yields the word `commit`, not `$commit`.
                    :
                    ;;
                *)
                    token+="$char"
                    started=1
                    ;;
                esac
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
                started=1
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
                subst_stack[$subst_depth]="b:double"
                subst_depth=$((subst_depth + 1))
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
                    subst_stack[$subst_depth]="p:double"
                    subst_depth=$((subst_depth + 1))
                    i=$((i + 1))
                    state=plain
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
        esac
    done

    if [[ $started -eq 1 ]]; then
        LOOM_TOKENS+=("$token")
    fi

    [[ "$state" == plain ]]
}

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
    if [[ -n "${_LOOM_DENY_ENABLED_CACHE:-}" ]]; then
        [[ "$_LOOM_DENY_ENABLED_CACHE" == "1" ]]
        return
    fi

    _LOOM_DENY_ENABLED_CACHE=0

    local work_dir="${LOOM_WORK_DIR:-}"
    if [[ -n "$work_dir" ]]; then
        local config="${work_dir}/config.toml"
        if [[ -r "$config" ]]; then
            local found
            found=$(awk '
                {
                    t = $0
                    sub(/^[[:space:]]+/, "", t)
                    sub(/[[:space:]]+$/, "", t)
                }
                t == "[hooks]" { in_hooks = 1; next }
                t ~ /^\[/ { in_hooks = 0; next }
                in_hooks && t ~ /^deny_enabled[[:space:]]*=[[:space:]]*true$/ {
                    print "1"
                    exit
                }
            ' "$config" 2>/dev/null || true)
            if [[ "$found" == "1" ]]; then
                _LOOM_DENY_ENABLED_CACHE=1
            fi
        fi
    fi

    [[ "$_LOOM_DENY_ENABLED_CACHE" == "1" ]]
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
