#!/usr/bin/env bash
# common-token-helpers.sh - The seven loom_tokens_* helpers in _common.sh
# scan argv VALUES from loom_tokenize_command, not the raw command string.
# That is what fixes three reported false positives: a codex-forward task
# prompt carried as one single-quoted argument containing prose like
# "no git add, no git commit", a relative TypeScript import, and an
# ARR.find((c) => ...) call all used to trip regex-on-raw-string guards even
# though none of those words sit at a real command or argument position.
# This file exercises the helpers directly against LOOM_TOKENS, without going
# through any hook script.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# shellcheck source=../_common.sh
source "$SCRIPT_DIR/../_common.sh"

FAILED=0

fail() {
    echo "FAIL: $1"
    FAILED=1
}

# assert_true <label> <command-string> <helper...> - tokenize <command-string>
# then assert the helper call (remaining args) returns 0 (true).
assert_true() {
    local label="$1" cmd="$2"
    shift 2
    if ! loom_tokenize_command "$cmd"; then
        fail "$label: tokenizer reported an unterminated quote"
        return
    fi
    if ! "$@"; then
        fail "$label: expected true, got false (tokens: ${LOOM_TOKENS[*]})"
    fi
}

# assert_false <label> <command-string> <helper...> - same, but asserts false.
assert_false() {
    local label="$1" cmd="$2"
    shift 2
    if ! loom_tokenize_command "$cmd"; then
        fail "$label: tokenizer reported an unterminated quote"
        return
    fi
    if "$@"; then
        fail "$label: expected false, got true (tokens: ${LOOM_TOKENS[*]})"
    fi
}

# assert_tokenizes <label> <command-string> - assert the parse itself is clean.
# A failed tokenize is never cosmetic: every caller then falls back to its raw
# regex, which resurrects the exact false positives these helpers exist to
# remove, so the return code is worth pinning on its own.
assert_tokenizes() {
    local label="$1" cmd="$2"
    if ! loom_tokenize_command "$cmd"; then
        fail "$label: expected a clean parse, got an unterminated-quote report"
    fi
}

# assert_tokenize_fails <label> <command-string> - the mirror of the above, for
# the FAIL-CLOSED cases. Returning 0 for a token list the tokenizer knows is
# incomplete is worse than returning 1: the caller trusts it and no fallback
# runs, so a guard that the raw regex would have caught is silently bypassed.
assert_tokenize_fails() {
    local label="$1" cmd="$2"
    if loom_tokenize_command "$cmd"; then
        fail "$label: expected a failed parse (rc 1), got a clean one (tokens: ${LOOM_TOKENS[*]})"
    fi
}

# --- The reported false positives -------------------------------------------
# A codex-forward invocation carrying an entire task prompt as ONE
# single-quoted positional argument. The prompt itself contains prose about
# git, a relative import, and an ARR.find call - none of it is a real
# command or argument position.
CODEX_FORWARD_CMD=$'~/.claude/hooks/loom/codex-forward.sh task \'Fix the bug.\nDo NOT run git at all (no git add, no git commit).\nimport { helper } from "../../../src/y";\nARR.find((c) => c.k === k)\n\' --model gpt-5.6-terra --effort xhigh --write'

assert_false "codex-forward prompt: git is not invoked" "$CODEX_FORWARD_CMD" \
    loom_tokens_invoke 'git'
assert_false "codex-forward prompt: find is not invoked" "$CODEX_FORWARD_CMD" \
    loom_tokens_invoke 'find'
assert_false "codex-forward prompt: git commit is not an argument pair" "$CODEX_FORWARD_CMD" \
    loom_tokens_cmd_has_arg 'git' 'commit'
assert_false "codex-forward prompt: relative import is not a standalone word (has whitespace)" "$CODEX_FORWARD_CMD" \
    loom_tokens_word_matches '\.\./\.\.'

assert_false "prose about git commit inside echo is not an invocation" \
    "echo 'do not run git commit'" \
    loom_tokens_cmd_has_arg 'git' 'commit'

assert_false "a loom memory note quoting a relative path is not a standalone word" \
    'loom memory note "a path like ../../../src/y is bad"' \
    loom_tokens_word_matches '\.\./\.\.'

# --- Must not regress: real invocations still block -------------------------

assert_true "git commit -m is a real invocation" \
    'git commit -m "msg"' \
    loom_tokens_cmd_has_arg 'git' 'commit'

assert_true "git commit reached through a && separator" \
    'cargo build && git commit -m x' \
    loom_tokens_cmd_has_arg 'git' 'commit'

assert_true "git commit inside a command substitution" \
    'echo $(git commit)' \
    loom_tokens_cmd_has_arg 'git' 'commit'

assert_true "git add -A is an adjacent argument pair" \
    'git add -A' \
    loom_tokens_cmd_has_arg_pair 'git' 'add' '-A'

assert_true "sudo git commit unwraps the sudo wrapper" \
    'sudo git commit' \
    loom_tokens_invoke 'git'

assert_true "env -u NAME git commit unwraps env and its -u argument" \
    'env -u LOOM_MAIN_AGENT_PID git commit' \
    loom_tokens_invoke 'git'

assert_true "cat ../../foo is a real word-shaped traversal" \
    'cat ../../foo' \
    loom_tokens_word_matches '\.\./\.\.'

assert_true "xargs grep foo unwraps the xargs wrapper" \
    'xargs grep foo' \
    loom_tokens_invoke 'grep'

assert_true "GIT_DIR=/x is a standalone assignment word" \
    'GIT_DIR=/x git status' \
    loom_tokens_word_matches '^GIT_DIR='
assert_true "GIT_DIR=/x git status still invokes git after the leading assignment" \
    'GIT_DIR=/x git status' \
    loom_tokens_invoke 'git'

assert_true "loom stage complete x: argv[1] is stage" \
    'loom stage complete x' \
    loom_tokens_cmd_argv 'loom' 1 'stage'
assert_true "loom stage complete x: argv[2] is complete" \
    'loom stage complete x' \
    loom_tokens_cmd_argv 'loom' 2 'complete'

assert_true "/usr/bin/grep -n x invokes grep by basename" \
    '/usr/bin/grep -n x' \
    loom_tokens_invoke 'grep'

# --- Bypasses the argv narrowing opened, and now closes ----------------------
# Every case below was BLOCKED by the raw-regex checks these helpers replaced
# and was ALLOWED once the hooks started scanning tokens instead. Each asserts
# the post-fix behaviour, so a regression here is a live guard bypass.

# `sh -c <payload>`: the payload is one whitespace-bearing word, so nothing
# inside it sits at a command or argument position until it is re-tokenized
# and spliced back in.
assert_true "bash -c payload: git commit is seen inside the script" \
    "bash -c 'git commit -m wip'" \
    loom_tokens_cmd_has_arg 'git' 'commit'
assert_true "sh -c payload: a relative traversal is seen inside the script" \
    "sh -c 'cat ../../foo'" \
    loom_tokens_word_matches '\.\./\.\.'
assert_true "sh -c payload: git -C is seen inside the script" \
    "sh -c 'git -C /other status'" \
    loom_tokens_cmd_has_arg 'git' '-C'
assert_true "nested sh -c inside bash -c is spliced two levels deep" \
    "bash -c \"sh -c 'git commit'\"" \
    loom_tokens_cmd_has_arg 'git' 'commit'
assert_false "a -c flag on a NON-shell command is not a script payload" \
    "notashell -c 'git commit'" \
    loom_tokens_cmd_has_arg 'git' 'commit'

# The recursion budget must FAIL CLOSED, not silently allow. Within the bound
# the payload is expanded and the parse is clean; PAST the bound the innermost
# payload is still one opaque word, so reporting a clean parse would leave every
# hook trusting a token list that hides `git commit` - and the raw regex this
# replaced did match that string. Raising the depth number is not the fix: any
# fixed bound has a next level.
SHELL_C_DEPTH1='bash -c '"'"'git commit'"'"''
SHELL_C_DEPTH2="bash -c \"bash -c 'git commit'\""
SHELL_C_DEPTH3="bash -c \"bash -c \\\"bash -c 'git commit'\\\"\""

assert_true "sh -c depth 1 parses cleanly and git commit is detected" \
    "$SHELL_C_DEPTH1" \
    loom_tokens_cmd_has_arg 'git' 'commit'
assert_true "sh -c depth 2 parses cleanly and git commit is detected" \
    "$SHELL_C_DEPTH2" \
    loom_tokens_cmd_has_arg 'git' 'commit'
assert_tokenize_fails "sh -c depth 3 exhausts the budget with a payload unexpanded" \
    "$SHELL_C_DEPTH3"

# Shell keywords occupy argv[0] without being the command.
assert_true "if git commit ...; then ...; fi resolves past the if keyword" \
    'if git commit -m x; then :; fi' \
    loom_tokens_cmd_has_arg 'git' 'commit'
assert_true "a { } group resolves past the brace" \
    '{ git commit -m x; }' \
    loom_tokens_cmd_has_arg 'git' 'commit'
assert_true "a do-loop body resolves past the do keyword" \
    'while read -r f; do grep x "$f"; done' \
    loom_tokens_invoke 'grep'

# Arg-taking wrapper flags: the wrapper's VALUE word is not the command.
assert_true "timeout N cmd steps over the duration" \
    'timeout 60 git commit -m x' \
    loom_tokens_cmd_has_arg 'git' 'commit'
assert_true "nice -n N cmd steps over the niceness value" \
    'nice -n 10 git commit' \
    loom_tokens_invoke 'git'
assert_true "xargs -n 1 cmd steps over the batch size" \
    'xargs -n 1 grep foo' \
    loom_tokens_invoke 'grep'
assert_true "xargs -I {} cmd steps over the replacement string" \
    'xargs -I {} grep x {}' \
    loom_tokens_invoke 'grep'
assert_true "chained wrappers still unwind to the real command" \
    'sudo env FOO=bar timeout 5 git commit' \
    loom_tokens_invoke 'git'

# Command-prefix builtins. `exec git commit` REPLACES the shell with git, so it
# really does run it; a resolver that stops at the prefix hides the invocation
# from every guard.
assert_true "exec git commit resolves past the exec prefix" \
    'exec git commit -m x' \
    loom_tokens_cmd_has_arg 'git' 'commit'
assert_true "exec -a NAME consumes the argv[0] override" \
    'exec -a mygit git commit -m x' \
    loom_tokens_cmd_has_arg 'git' 'commit'
assert_true "builtin resolves past the builtin prefix" \
    'builtin git commit -m x' \
    loom_tokens_cmd_has_arg 'git' 'commit'
assert_true "doas resolves past the doas prefix" \
    'doas git commit -m x' \
    loom_tokens_cmd_has_arg 'git' 'commit'
assert_true "doas -u NAME consumes the user argument" \
    'doas -u root git commit -m x' \
    loom_tokens_cmd_has_arg 'git' 'commit'
assert_true "setsid resolves past the setsid prefix" \
    'setsid -f git commit -m x' \
    loom_tokens_cmd_has_arg 'git' 'commit'
assert_true "command-prefix builtins chain with the other wrappers" \
    'exec setsid sudo git commit -m x' \
    loom_tokens_cmd_has_arg 'git' 'commit'
assert_true "exec bash -c payload is still spliced" \
    "exec bash -c 'git commit'" \
    loom_tokens_cmd_has_arg 'git' 'commit'

# eval must NOT become transparent. commit-filter.sh and worktree-isolation.sh
# detect it as its own risk signal - its argument is a string the guard cannot
# see into - so resolving through it would silently disable those checks.
assert_true "eval is still reported as invoking eval, not resolved through" \
    'eval "git commit"' \
    loom_tokens_invoke 'eval'
assert_false "eval does not resolve through to its string argument" \
    'eval "git commit"' \
    loom_tokens_invoke 'git'

# ANSI-C quoting: bash drops the `$`, so the VALUE is `commit`, not `$commit`.
assert_true "git \$'commit' is a real git commit invocation" \
    "git \$'commit' -m x" \
    loom_tokens_cmd_has_arg 'git' 'commit'

# Substitutions inside DOUBLE quotes must restore the quote when they close.
# Before the fix the walk ran off the end of the string, the parse failed, and
# every caller silently reverted to its raw-regex fallback.
assert_tokenizes 'a $( ) substitution inside double quotes parses cleanly' \
    'echo "today is $(date)"'
assert_tokenizes 'a backtick substitution inside double quotes parses cleanly' \
    'echo "today is `date` ok"'
assert_true 'a $( ) inside double quotes still opens a command position' \
    'echo "today is $(date)"' \
    loom_tokens_invoke 'date'
assert_false "a double-quoted brief with a backtick keeps its prose out of word checks" \
    '~/.claude/hooks/loom/codex-forward.sh task "Fix it. Use `pwd`; import from ../../../src/y" --model gpt-5.6-terra' \
    loom_tokens_word_matches '\.\./\.\.'
assert_false "a double-quoted brief with a \$( ) keeps its prose out of word checks" \
    '~/.claude/hooks/loom/codex-forward.sh task "See $(pwd) and import from ../../../src/y" --model gpt-5.6-terra' \
    loom_tokens_word_matches '\.\./\.\.'

# An arg-taking flag at a segment boundary must not reach into the NEXT
# command and report its command word as this segment's. `env -u; git commit`
# has no command word in segment 0 at all.
if loom_tokenize_command 'env -u; git commit'; then
    if BOUNDARY_IDX=$(loom_tokens_command_word_index 0); then
        fail "env -u at a segment boundary resolved across %%SEP%% to index $BOUNDARY_IDX (${LOOM_TOKENS[$BOUNDARY_IDX]})"
    fi
else
    fail "env -u; git commit: tokenizer reported an unterminated quote"
fi

# loom_token_is_word must key on whitespace only. Bash 3.2 treats the quoted
# parts of an `=~` pattern literally and can pull a backslash into a bracket
# expression built from $' \t\r\n', which would misjudge `a\b` as whitespace.
loom_token_is_word 'a\b' ||
    fail "loom_token_is_word: a backslash-bearing word must still be word-shaped"
loom_token_is_word '../../x' ||
    fail "loom_token_is_word: a plain traversal path must be word-shaped"
if loom_token_is_word 'a b'; then
    fail "loom_token_is_word: a space-bearing token must not be word-shaped"
fi
if loom_token_is_word "$(printf 'a\tb')"; then
    fail "loom_token_is_word: a tab-bearing token must not be word-shaped"
fi
if loom_token_is_word '%%SEP%%'; then
    fail "loom_token_is_word: the sentinel must not be word-shaped"
fi

# The headline fix must survive the sh -c splicing: the codex forward has no
# shell at argv[0] and no -c, so its whole brief stays exactly ONE token.
if loom_tokenize_command "$CODEX_FORWARD_CMD"; then
    if [[ ${#LOOM_TOKENS[@]} -ne 8 ]]; then
        fail "codex-forward: expected 8 argv tokens, got ${#LOOM_TOKENS[@]} (${LOOM_TOKENS[*]})"
    fi
    case "${LOOM_TOKENS[2]}" in
    *"no git add, no git commit"*"../../../src/y"*) ;;
    *)
        fail "codex-forward: the brief must stay ONE token, got <${LOOM_TOKENS[2]}>"
        ;;
    esac
else
    fail "codex-forward: tokenizer reported an unterminated quote"
fi

if [[ $FAILED -ne 0 ]]; then
    exit 1
fi

echo "PASS"
