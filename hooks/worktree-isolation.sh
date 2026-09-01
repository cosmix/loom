#!/usr/bin/env bash
# worktree-isolation.sh - PreToolUse hook to enforce worktree boundaries
#
# This hook intercepts tool calls and blocks operations that would violate
# worktree isolation boundaries:
#
# For Bash tool:
#   - Block `git -C`, `git --work-tree`, `git --git-dir` (accessing other git dirs)
#   - Block GIT_DIR= / GIT_WORK_TREE= env assignments that retarget git
#   - Block `eval`-reached git (the regex cannot see inside eval'd strings)
#   - Block `../../` path traversal (escaping worktree)
#   - Block `.worktrees/` access (except current worktree)
#
# SECURITY NOTE (best-effort): this is regex/token-scanning, not a parser. It
# cannot catch every evasion — variable indirection (g=-C; git $g ..), $IFS
# tricks, command substitution, or a cd into a sibling repo followed by a
# plain `git`. The DURABLE boundary is the OS sandbox `Write` deny on parent
# paths; this hook is defense-in-depth that raises the cost of the obvious
# bypasses.
#
# TOKEN SCANNING: each pattern below is checked against word-shaped argv
# tokens produced by loom_tokenize_command (hooks/_common.sh), not the raw
# command string. A quoted PROSE payload — a codex-forward task brief, a
# `loom memory note` body — tokenizes to ONE token carrying embedded
# whitespace, and loom_token_is_word excludes it from every check; a real
# path, flag, or env assignment is always a whitespace-free word and stays
# covered. This is what lets `echo 'import x from "../../../src/y"'` and a
# multi-line codex brief quoting relative-import specifiers through, while
# `cat "../../foo"` (a quoted but genuinely word-shaped path argument) still
# blocks — quoting alone was never the right discriminator; whitespace
# inside the token is. When loom_tokenize_command cannot produce a
# trustworthy token list (an unterminated quote), every pattern falls back
# to the original raw-string regex scan, so protection is never weaker than
# it was before tokenizing existed.
#
# NARROWER EXCEPTION for the path-traversal and cross-worktree checks: a
# whitespace-bearing token that STARTS WITH the traversal sequence
# (`../..`/`..\..`) or with `.worktrees/` is still checked, even though it
# fails loom_token_is_word. A prose payload effectively never BEGINS with
# either sequence, so this closes a real escape whose path contains a space
# (`cat "../../doc/my notes.md"`) without reintroducing the false positive
# above. The discriminator stays "does this token LOOK like a path being
# operated on" — a token that STARTS WITH the sequence does, whatever
# follows it.
#
# Input: JSON from stdin (Claude Code passes tool info via stdin)
#   {"tool_name": "Bash", "tool_input": {...}, ...}
#
# Exit codes:
#   0 - Allow the operation
#   2 - Block with guidance message
#
# Environment:
#   LOOM_STAGE_ID - Current stage ID (set by loom)
#   LOOM_WORKTREE_PATH - Path to current worktree (if set)

set -euo pipefail
source "$(dirname "$0")/_common.sh"

debug() {
    [[ "${WORKTREE_ISOLATION_DEBUG:-}" == "1" ]] || return 0
    echo "$@" >&2
}

# Read JSON input from stdin
if command -v gtimeout &>/dev/null; then
    INPUT_JSON=$(gtimeout 1 cat 2>/dev/null || true)
elif command -v timeout &>/dev/null; then
    INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)
else
    INPUT_JSON=$(cat 2>/dev/null || true)
fi

debug "=== $(date) worktree-isolation ==="
debug "INPUT_JSON: $INPUT_JSON"
debug "LOOM_STAGE_ID: ${LOOM_STAGE_ID:-unset}"
debug "PWD: $(pwd)"

# Parse tool_name and tool_input from JSON
TOOL_NAME=$(echo "$INPUT_JSON" | jq -r '.tool_name // empty' 2>/dev/null || true)
TOOL_INPUT=$(echo "$INPUT_JSON" | jq -r '.tool_input // empty' 2>/dev/null || true)

debug "TOOL_NAME: $TOOL_NAME"
debug "TOOL_INPUT: $TOOL_INPUT"

# Only enforce inside a genuine loom worktree. Membership is decided by the
# working directory, NOT by LOOM_STAGE_ID: that variable leaks into plain Claude
# Code sessions (e.g. a prior loom run exported it), so gating on it alone made
# this hook wrongly fire on ordinary branches like main. If we are not inside a
# loom worktree, stay inert.
CURRENT_WORKTREE=$(loom_current_worktree) || {
    debug "Not inside a loom worktree; allowing"
    exit 0
}

# Derive the stage from the worktree path itself — authoritative for which
# worktree this session owns, and immune to a stale LOOM_STAGE_ID.
CURRENT_STAGE=$(basename "$CURRENT_WORKTREE")

# === BLOCK MESSAGES ===
# Factored out so both the token-scan path and the legacy regex fallback path
# print identical guidance text.

print_git_override_block() {
    cat >&2 <<'EOF'

============================================================
  LOOM: BLOCKED - Git directory override detected
============================================================

You tried to: Retarget git at another directory (-C / --work-tree /
--git-dir / GIT_DIR= / GIT_WORK_TREE=) or reach git via eval

This is FORBIDDEN in loom worktrees because:
  - Each worktree has its own isolated git state
  - Cross-worktree git operations corrupt state
  - eval hides the real command from isolation checks

Instead, you should:
  - Run git commands in the CURRENT worktree only
  - Use relative paths within this worktree
  - Stay confined to your assigned worktree
  - Do not wrap git in eval

Git commands should operate on the current directory.
============================================================

EOF
}

print_path_traversal_block() {
    cat >&2 <<'EOF'

============================================================
  LOOM: BLOCKED - Path traversal detected
============================================================

You tried to: Use ../../ to escape the worktree

This is FORBIDDEN in loom worktrees because:
  - You are CONFINED to this worktree
  - Accessing parent directories breaks isolation
  - Other worktrees/stages may be affected

Instead, you should:
  - Use relative paths WITHIN this worktree
  - All files you need are in the worktree
  - Context is in your signal file (.loom/work/signals/, or the legacy .work/signals/)

Stay within your worktree boundaries.
============================================================

EOF
}

print_cross_worktree_block() {
    cat >&2 <<EOF

============================================================
  LOOM: BLOCKED - Cross-worktree access detected
============================================================

You tried to: Access .worktrees/ directory (another stage's worktree)

This is FORBIDDEN because:
  - Each stage has its own isolated worktree
  - Accessing other stages' worktrees breaks isolation
  - You may corrupt another stage's work

Instead, you should:
  - Stay in YOUR worktree: .worktrees/${CURRENT_STAGE}/
  - Your files and context are all here
  - Communicate via .loom/work/ (shared state symlink; legacy workspaces use .work/)

You can only access your own worktree.
============================================================

EOF
}

# === TOKEN-SCAN PATTERN CHECKS (LOOM_TOKENS already populated by a prior
# successful loom_tokenize_command call) ===

# Pattern 1 (token path): git directory/work-tree overrides and eval-reached
# git. loom_tokens_cmd_has_arg/loom_tokens_invoke only match a REAL command
# segment (the effective command word, after wrapper-unwrapping), so `eval`
# or `GIT_DIR=` merely mentioned inside a quoted brief no longer arms these -
# and the GIT_DIR=/GIT_WORK_TREE= checks are both ANCHORED (^...=) AND
# word-shaped, so a prose sentence that happens to contain that substring
# never matches either. Returns 0 when this pattern is present.
check_git_override_tokens() {
    loom_tokens_cmd_has_arg 'git' '-C' ||
        loom_tokens_cmd_has_arg 'git' '--work-tree(=.*)?' ||
        loom_tokens_cmd_has_arg 'git' '--git-dir(=.*)?' ||
        loom_tokens_word_matches '^GIT_DIR=' ||
        loom_tokens_word_matches '^GIT_WORK_TREE=' ||
        loom_tokens_invoke 'eval'
}

# Pattern 2 (token path): ../../ path traversal. THE HEADLINE FIX -
# loom_tokens_word_matches only considers word-shaped (whitespace-free)
# tokens, so a quoted PROSE payload (a codex-forward brief, a `loom memory
# note` body) that merely CONTAINS "../../../src/..." tokenizes to one
# multi-word token and is skipped, while a quoted but genuinely word-shaped
# path argument (`cat "../../foo"`) is still a single whitespace-free token
# and still matches. Do not "simplify" this back to scanning every token -
# that reintroduces the exact false positive this rewrite exists to fix.
#
# check_path_traversal_prefix_tokens below covers the remaining gap: a
# genuine traversal path whose argument contains a space
# (`cat "../../doc/my notes.md"`) tokenizes to ONE whitespace-bearing token
# and loom_tokens_word_matches would miss it entirely. It is checked
# separately (STARTS WITH, not CONTAINS) so a prose payload that merely
# mentions "../.." mid-sentence still does not match.
#
# Returns 0 when this pattern is present.
check_path_traversal_tokens() {
    loom_tokens_word_matches '\.\./\.\.' ||
        loom_tokens_word_matches '\.\.[\\/]\.\.' ||
        check_path_traversal_prefix_tokens
}

# Pattern 2b (token path): the whitespace-bearing half of the prefix
# exception described above the TOKEN SCANNING header comment. Walks
# LOOM_TOKENS directly (bash 3.2 `set -u` empty-array guard, same as
# check_cross_worktree_tokens) because no loom_tokens_* helper in _common.sh
# checks a PREFIX rather than a full-token or unanchored match. Returns 0
# when some token (word-shaped or not, skipping only the "%%SEP%%"
# sentinel) starts with `../..` or `..\..`.
check_path_traversal_prefix_tokens() {
    local n=${#LOOM_TOKENS[@]}
    ((n > 0)) || return 1
    local i tok
    for ((i = 0; i < n; i++)); do
        tok="${LOOM_TOKENS[$i]}"
        [[ "$tok" == "%%SEP%%" ]] && continue
        [[ "$tok" =~ ^\.\.[\\/]\.\. ]] && return 0
    done
    return 1
}

# Pattern 3 (token path): .worktrees/ access outside the current stage. No
# single helper call covers "block unless it names MY worktree", so this
# walks LOOM_TOKENS directly - guarded for bash 3.2 `set -u` on an empty
# array the same way loom-control-complete.sh's is_completion_command does.
# A word-shaped token counts if it CONTAINS ".worktrees/" anywhere (same
# reasoning as Pattern 2's original check): a prose mention of another
# stage's directory inside a quoted brief is not a real access attempt, and
# a real path/flag argument is always whitespace-free. A whitespace-bearing
# token additionally counts, but only if it STARTS WITH ".worktrees/" - the
# same narrower prefix exception Pattern 2 uses to close the analogous hole
# for a real path argument that happens to contain a space
# (`cat ".worktrees/other-stage/some notes.md"`), without reintroducing the
# false positive for prose that merely mentions ".worktrees/" mid-sentence.
# Returns 0 when this pattern is present (a token names some OTHER stage's
# worktree; a token naming CURRENT_STAGE's own worktree, or a path under it,
# is exempt and does not trigger a block on its own).
check_cross_worktree_tokens() {
    local n=${#LOOM_TOKENS[@]}
    ((n > 0)) || return 1
    local own_re="\\.worktrees/${CURRENT_STAGE}(/|\$)"
    local i tok
    for ((i = 0; i < n; i++)); do
        tok="${LOOM_TOKENS[$i]}"
        if loom_token_is_word "$tok"; then
            [[ "$tok" =~ \.worktrees/ ]] || continue
        else
            [[ "$tok" == .worktrees/* ]] || continue
        fi
        [[ "$tok" =~ $own_re ]] && continue
        return 0
    done
    return 1
}

validate_bash_command_token_path() {
    if check_git_override_tokens; then
        print_git_override_block
        return 1
    fi

    if check_path_traversal_tokens; then
        print_path_traversal_block
        return 1
    fi

    if check_cross_worktree_tokens; then
        print_cross_worktree_block
        return 1
    fi

    return 0
}

# === LEGACY REGEX FALLBACK ===
#
# Exercised only when loom_tokenize_command reports an unterminated quote (in
# which case bash itself would refuse to run the command anyway, so this path
# is rarely hit in practice). Preserved verbatim from before tokenizing
# existed, so today's protection is never weaker than it was.
validate_bash_command_regex_fallback() {
    local stripped="$1"

    # Pattern 1: Block git directory/work-tree overrides and eval-reached git.
    #   - `git -C <dir>` / `git --work-tree[=| ]` / `git --git-dir[=| ]`
    #   - `GIT_DIR=...` / `GIT_WORK_TREE=...` env assignments (retarget any git)
    #   - `eval ... git ...` — the regex cannot see inside an eval'd string, so we
    #     refuse the whole command rather than let it through unparsed.
    if echo "$stripped" | grep -qE 'git[[:space:]]+-C[[:space:]]' || \
       echo "$stripped" | grep -qE 'git[[:space:]]+--work-tree([=[:space:]]|$)' || \
       echo "$stripped" | grep -qE 'git[[:space:]]+--git-dir([=[:space:]]|$)' || \
       echo "$stripped" | grep -qE '(^|[[:space:];&|(])GIT_DIR=' || \
       echo "$stripped" | grep -qE '(^|[[:space:];&|(])GIT_WORK_TREE=' || \
       echo "$stripped" | grep -qE '(^|[[:space:];&|(])eval([[:space:]]|$)'; then
        print_git_override_block
        return 1
    fi

    # Pattern 2: Block ../../ path traversal (escaping worktree)
    if echo "$stripped" | grep -qE '\.\./\.\.' || echo "$stripped" | grep -qE '\.\.[\\/]\.\.'; then
        print_path_traversal_block
        return 1
    fi

    # Pattern 3: Block .worktrees/ access (except current worktree)
    # Allow references to current stage, block others
    if echo "$stripped" | grep -qE '\.worktrees/' && \
       ! echo "$stripped" | grep -qE "\.worktrees/${CURRENT_STAGE}[/[:space:]]|\.worktrees/${CURRENT_STAGE}\$"; then
        print_cross_worktree_block
        return 1
    fi

    return 0
}

# === BASH VALIDATION ===
validate_bash_command() {
    local cmd="$1"
    local stripped
    stripped=$(strip_embedded_content "$cmd")

    if loom_tokenize_command "$stripped"; then
        validate_bash_command_token_path
        return $?
    fi

    debug "Tokenizer reported an unterminated quote - falling back to the regex scan"
    validate_bash_command_regex_fallback "$stripped"
    return $?
}

# === MAIN DISPATCH ===
case "$TOOL_NAME" in
    Bash)
        COMMAND=$(echo "$TOOL_INPUT" | jq -r '.command // empty' 2>/dev/null || echo "$TOOL_INPUT")
        if [[ -n "$COMMAND" ]]; then
            if ! validate_bash_command "$COMMAND"; then
                debug "BLOCKED: Bash command failed validation"
                exit 2
            fi
        fi
        ;;

    *)
        # Not a tool we validate
        ;;
esac

debug "Allowing operation"
exit 0
