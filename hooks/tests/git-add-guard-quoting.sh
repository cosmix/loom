#!/usr/bin/env bash
# git-add-guard-quoting.sh - the guard must ignore prose that merely MENTIONS
# a dangerous git-add form inside a quoted argument, while still blocking a
# real invocation whose argument VALUE is dangerous regardless of quoting
# (quoting changes how bash parses a command, never the argument's value).
set -euo pipefail
HOOK="$(dirname "$0")/../git-add-guard.sh"

expect_allow() {
    local desc="$1" cmd="$2"
    local input code
    input=$(jq -nc --arg c "$cmd" '{tool_name:"Bash",tool_input:{command:$c}}')
    set +e
    echo "$input" | bash "$HOOK" >/dev/null 2>&1
    code=$?
    set -e
    if [[ $code -ne 0 ]]; then
        echo "FAIL: expected exit 0 (allow) for: $desc"
        echo "  command: $cmd"
        exit 1
    fi
}

expect_block() {
    local desc="$1" cmd="$2"
    local input code
    input=$(jq -nc --arg c "$cmd" '{tool_name:"Bash",tool_input:{command:$c}}')
    set +e
    echo "$input" | bash "$HOOK" >/dev/null 2>&1
    code=$?
    set -e
    if [[ $code -ne 2 ]]; then
        echo "FAIL: expected exit 2 (block) for: $desc"
        echo "  command: $cmd"
        exit 1
    fi
}

# --- MUST BE ALLOWED (exit 0) -----------------------------------------------

expect_allow "prose mentioning the forbidden forms, single-quoted" \
    "echo 'Never run git add -A or git add . because it stages .work'"

expect_allow "the same prose, double-quoted" \
    'echo "Never run git add -A or git add . because it stages .work"'

# The old regression this guard already fixed once: an unbounded regex let a
# LATER line of a multi-line commit message (here, "Co-Authored-By" contains
# "-A") satisfy the git-add danger patterns even though the message is one
# quoted argument to -m, never a git-add argument.
expect_allow "multi-line commit -m body containing Co-Authored-By" \
    $'git add hooks/commit-filter.sh\ngit commit -q -m "fix(hooks): tighten the guard\n\nExplain that a Co-Authored-By trailer must never be added."'

expect_allow "staging specific files" "git add src/main.rs src/lib.rs"
expect_allow ".workspace is not .work" "git add .workspace"
expect_allow ".working is not .work" "git add .working"
expect_allow ".workdir is not .work" "git add .workdir"
expect_allow "./file is not ." "git add ./file"
expect_allow "doc path with no .work at all" "git add doc/foo.md"
expect_allow "unrelated filename with no .work at all" "git add network.md"
expect_allow "cargo add is not git add" "cargo add serde"

# --- MUST BE BLOCKED (exit 2) -----------------------------------------------

expect_block "git add -A" "git add -A"
expect_block "git add --all" "git add --all"
expect_block "git add ." "git add ."
expect_block "git add .work" "git add .work"
expect_block "git add .work/" "git add .work/"
expect_block "git add .work/foo" "git add .work/foo"
expect_block "git add foo .work bar" "git add foo .work bar"
expect_block "git add .work other" "git add .work other"

# Quoted real arguments: quoting changes how bash PARSES the command, never
# the argument's VALUE - a naive fix that just blanked quoted interiors
# before scanning would let these two through, which is exactly the gap
# tokenizing (rather than blanking) closes.
expect_block "quoted real -A argument" "git add '-A'"
expect_block "quoted real .work argument" 'git add ".work"'

expect_block "git add reached through a && separator" 'cargo build && git add -A'
expect_block "git add -A inside a command substitution" 'echo $(git add -A)'

# --- A3 regression: wrapper commands must not bypass the guard -------------
#
# scan_git_add_tokens used to walk the token stream itself, only recognizing
# a bare `git`/`*/git` token as "the git invocation" - it never unwrapped a
# leading wrapper command, so `sudo git add -A` and `env FOO=1 git add .`
# slipped past entirely. It now resolves the effective command word via
# _common.sh's loom_tokens_command_word_index (the same helper every other
# hook's checks use), which already unwraps sudo/env/xargs/time/nohup/
# command/nice/stdbuf/timeout/gtimeout and their own option words.
expect_block "sudo git add -A is still blocked" "sudo git add -A"
expect_block "env FOO=1 git add . is still blocked" "env FOO=1 git add ."

# --- A5: raw-regex FALLBACK branch coverage ---------------------------------
#
# Nothing previously exercised the path loom_tokenize_command falls back to
# on an unterminated quote. An UNTERMINATED QUOTE forces the pre-tokenizing
# regex fallback in check_dangerous_patterns - a real dangerous `git add -A`
# invocation inside that malformed command must still be blocked.
expect_block "unterminated quote wrapping a real 'git add -A' is still blocked" \
    'git add -A "oops'

echo "PASS"
