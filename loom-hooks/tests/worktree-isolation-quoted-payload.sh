#!/usr/bin/env bash
# worktree-isolation.sh regex-matched the RAW Bash command string, so a
# relative-import specifier sitting inside a QUOTED PROSE argument (a
# codex-forward task brief, a `loom memory note` body) was scanned as if it
# were a real path argument and blocked as "path traversal" even though
# nothing was ever opened outside the worktree. The fix scans word-shaped
# argv TOKENS instead of the raw string: a quoted prose payload tokenizes to
# one token carrying embedded whitespace and is excluded, while a quoted but
# genuinely word-shaped path argument (`cat "../../foo"`) is still a single
# whitespace-free token and still blocks. This test pins both directions,
# plus the sibling git-override and cross-worktree patterns that moved to the
# same token-scan approach.
set -euo pipefail

HOOK="$(cd "$(dirname "$0")/.." && pwd)/worktree-isolation.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

STAGE="build-api"
OTHER_STAGE="other-stage"
WORKTREE="$TMP/repo/.worktrees/$STAGE"
mkdir -p "$WORKTREE/src" "$TMP/repo/.worktrees/$OTHER_STAGE"

run_hook() {
    local cmd="$1"
    local input
    input=$(jq -nc --arg c "$cmd" '{tool_name:"Bash",tool_input:{command:$c}}')
    (cd "$WORKTREE" && printf '%s' "$input" | bash "$HOOK" 2>&1)
}

expect_allow() {
    local desc="$1" cmd="$2"
    local output code
    set +e
    output=$(run_hook "$cmd")
    code=$?
    set -e
    if [[ $code -ne 0 ]]; then
        echo "FAIL: expected exit 0 (allow) for: $desc" >&2
        echo "  command: $cmd" >&2
        echo "  output: $output" >&2
        exit 1
    fi
}

expect_block() {
    local desc="$1" cmd="$2"
    local output code
    set +e
    output=$(run_hook "$cmd")
    code=$?
    set -e
    if [[ $code -ne 2 ]]; then
        echo "FAIL: expected exit 2 (block) for: $desc" >&2
        echo "  command: $cmd" >&2
        echo "  output: $output" >&2
        exit 1
    fi
}

# --- MUST BE ALLOWED (exit 0) -----------------------------------------------
# The reported false positives: a relative-import specifier sitting inside a
# quoted PROSE payload, never a real path argument.

PROSE_IMPORT_CMD="echo 'import x from \"../../../src/y\"'"
expect_allow "single-quoted prose containing a relative import" "$PROSE_IMPORT_CMD"

CODEX_CMD="~/.claude/hooks/loom/codex-forward.sh task \"Update the chart loaders.
import { computeQuantiles } from '../../../src/data/atlas/quantiles';
import { decodeAdmin1Overlay } from '../../../src/data/admin1';
Wire both into the loader module.\" --model gpt-5.6-terra --effort xhigh --write"
expect_allow "the full codex-forward command with a multi-line quoted brief" "$CODEX_CMD"

MEMORY_NOTE_CMD="loom memory note \"a relative import like ../../../src/data/atlas/foo is bad\""
expect_allow "loom memory note documenting the traversal trap" "$MEMORY_NOTE_CMD"

expect_allow "current stage's own worktree path is not cross-worktree access" \
    "cat .worktrees/$STAGE/src/main.rs"

# --- MUST BE BLOCKED (exit 2), no regressions -------------------------------

expect_block "bare path traversal" "cat ../../foo"

# Quoting changes how bash PARSES the command, never the argument's VALUE - a
# quoted but genuinely word-shaped path argument must still block. This is
# what proves quoting alone is not the discriminator; whitespace inside the
# token is.
expect_block "quoted real traversal path argument" 'cat "../../foo"'

expect_block "git -C directory override" "git -C /somewhere status"
expect_block "GIT_DIR= env assignment" "GIT_DIR=/x git status"
expect_block "eval-reached git" 'eval "git status"'
expect_block "git --work-tree override" "git --work-tree=/somewhere status"
expect_block "git --git-dir override" "git --git-dir=/somewhere/.git log"
expect_block "GIT_WORK_TREE= env assignment" "GIT_WORK_TREE=/x git status"

expect_block "another stage's worktree path" \
    "cat .worktrees/$OTHER_STAGE/src/main.rs"

# --- FALLBACK PATH: unterminated quote -> loom_tokenize_command returns 1,
# so validate_bash_command must fall through to validate_bash_command_regex_fallback.
# Nothing else in this file exercises that branch, so it would rot unnoticed.
expect_block "unterminated quote falls back to the regex scan and still blocks traversal" \
    'cat "../../foo'

echo "PASS"
