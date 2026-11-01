#!/usr/bin/env bash
# Shared hook helpers; Bash 3.2 compatible.
if [[ "${_LOOM_PARSER_LOADED:-}" == 1 ]]; then return 0; fi
_LOOM_PARSER_LOADED=1

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
