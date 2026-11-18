#!/usr/bin/env bash
# worktree-isolation.sh - PreToolUse hook to enforce worktree boundaries
#
# This hook intercepts tool calls and blocks operations that would violate
# worktree isolation boundaries:
#
# For Bash tool:
#   - Block global `git -C`, `--work-tree`, `--git-dir` unless every target
#     resolves into this worktree or the session scratch directory
#   - Block GIT_DIR= / GIT_WORK_TREE= env assignments that retarget git
#   - Block `eval`-reached git (the regex cannot see inside eval'd strings)
#   - Block `..` path traversal that RESOLVES outside the worktree (against
#     the directory each token runs in, loom_track_cwd); warn instead when a
#     `../..` token's directory cannot be known
#   - Block `.worktrees/` access (except current worktree)
#
# SECURITY NOTE (best-effort): this is regex/token-scanning, not a parser. It
# cannot catch every evasion — variable indirection (g=-C; git $g ..), $IFS
# tricks, command substitution, or a cd into a sibling repo followed by a
# plain `git`. The DURABLE boundary is the OS sandbox `Write` deny on parent
# paths; this hook is defense-in-depth that raises the cost of the obvious
# bypasses.
#
# TOKEN SCANNING: each pattern checks argv tokens from loom_tokenize_command,
# not the raw string. A quoted PROSE payload (a codex brief, a `loom memory
# note` body) is ONE token with embedded whitespace, which loom_token_is_word
# excludes; a real path, flag or env assignment is a whitespace-free word, so
# `cat "../../foo"` is still checked. Exception: a whitespace-bearing token
# that STARTS WITH `../..` or `.worktrees/` is checked too
# (`cat "../../doc/my notes.md"`); prose almost never begins that way. When
# the tokenizer fails (an unterminated quote), the raw-string regex fallback
# decides, so protection is never weaker than before tokenizing existed.
#
# Input: JSON from stdin (Claude Code passes tool info via stdin)
#   {"tool_name": "Bash", "tool_input": {...}, "cwd": "...", ...}
#
# Exit codes:
#   0 - Allow the operation (stdout may carry a LOOM_HOOK_WARN additionalContext)
#   2 - Block with guidance message; each block appends one line to the stage
#       block ledger (loom_record_block, rule id in BLOCK_RULE)
#   2 - jq not installed (fail closed)
#
# Environment:
#   LOOM_STAGE_ID - Current stage ID (set by loom)
#   LOOM_WORKTREE_PATH - Path to current worktree (if set)

set -euo pipefail
source "$(dirname "$0")/_common.sh"
loom_require_jq "worktree-isolation.sh"

debug() {
    [[ "${WORKTREE_ISOLATION_DEBUG:-}" == "1" ]] || return 0
    echo "$@" >&2
}

# Read JSON input from stdin
source "$(dirname "${BASH_SOURCE[0]}")/_bounded.sh"
INPUT_JSON=$(loom_read_input 1 2>/dev/null || true)

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
BLOCK_RULE=""
POLICY_TAG="[worktree-isolation/policy-load] "

# block_policy_load - record and exit for a policy that cannot be loaded.
block_policy_load() {
    loom_record_block worktree-isolation policy-load "${TOOL_NAME:-unknown}" \
        "$(jq -r '.tool_input.command // empty' <<<"$INPUT_JSON" 2>/dev/null || true)"
    exit 2
}
if ! source "$(dirname "${BASH_SOURCE[0]}")/_worktree_policy.sh"; then
    echo "${POLICY_TAG}LOOM: BLOCKED - _worktree_policy.sh cannot be loaded" >&2
    block_policy_load
fi
loom_load_worktree_policy "$POLICY_TAG" || block_policy_load

# === BLOCK MESSAGES ===
# Factored out so both the token-scan path and the legacy regex fallback path
# print identical guidance text. Each opens with the `[worktree-isolation/<rule>]`
# tag from BLOCK_RULE on its first non-empty line.

print_git_override_block() {
    cat >&2 <<EOF

[worktree-isolation/${BLOCK_RULE}] LOOM: BLOCKED - Git directory override detected
============================================================

You tried to: Retarget git at another directory (-C / --work-tree /
--git-dir / GIT_DIR= / GIT_WORK_TREE=) or reach git via eval

This is FORBIDDEN in loom worktrees because:
  - Each worktree has its own isolated git state
  - Cross-worktree git operations corrupt state
  - eval hides the real command from isolation checks

Instead, you should:
  - For eval: run the command directly, or loop in plain shell without eval
  - For -C: run git from inside this worktree, or point -C at a scratch repo
    under \$LOOM_SCRATCH_DIR, written as "\$LOOM_SCRATCH_DIR/<repo>" or as
    the literal path
  - Stay confined to your assigned worktree

Git commands should operate on the current directory.
============================================================

EOF
}

print_path_traversal_block() {
    cat >&2 <<EOF

[worktree-isolation/${BLOCK_RULE}] LOOM: BLOCKED - Path traversal detected
============================================================

You tried to: Reach ${1}, outside this worktree

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

[worktree-isolation/${BLOCK_RULE}] LOOM: BLOCKED - Cross-worktree access detected
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

# Pattern 1 (token path): git directory overrides that leave the worktree,
# GIT_DIR=/GIT_WORK_TREE= assignments and eval-reached git, matched only in a
# REAL command segment (loom_policy_git_tokens). Returns 0 when present.
check_git_override_tokens() {
    loom_policy_git_tokens
}

# Pattern 2 (token path): `..` traversal, decided by RESOLUTION against the
# directory each token runs in (LOOM_TOKEN_CWD, loom_track_cwd), so
# `cd web/src && cat ../../doc/x.md` is allowed and `cat ../sibling/x` is not.
# A PAIR candidate is what the lexical check matched before: a word-shaped
# token containing the policy's parent_pair, or ANY token starting with it
# (`cat "../../doc/my notes.md"`). A quoted prose payload is neither, so it
# stays unchecked. A WIDENED candidate is any other word-shaped token whose
# path part has a parent component (`../x`, `a/../b`, `..`).
#
# traversal_path_part <token> - the text after the first `=` when the text
# before it holds no `/` (`--opt=../x`), else the whole token: in `../a=b`
# the `=` sits inside a path component.
traversal_path_part() {
    local tok="$1"
    if [[ "$tok" == *=* && "${tok%%=*}" != */* ]]; then
        printf '%s' "${tok#*=}"
    else
        printf '%s' "$tok"
    fi
}

# traversal_kind <token> - print pair, widened, or nothing.
traversal_kind() {
    local tok="$1" prefix="^$LOOM_POLICY_PARENT_PAIR"
    if [[ "$tok" =~ $prefix ]] ||
        { loom_token_is_word "$tok" && [[ "$tok" =~ $LOOM_POLICY_PARENT_PAIR ]]; }; then
        printf pair
    elif loom_token_is_word "$tok" &&
        [[ "$(traversal_path_part "$tok")" =~ $LOOM_POLICY_PARENT_COMPONENT ]]; then
        printf widened
    fi
}

# check_path_traversal_resolved - 0 (block) when a candidate resolves to a
# sibling worktree, the shared state or outside, or a PAIR candidate cannot be
# normalized (`$X/../../y`, or a glob/brace component before a `..`: bash
# expands `.loom/w*` to the `work` symlink, and `..` then leaves it physically);
# TRAVERSAL_PATH names the target (a literal ${LOOM_SCRATCH_DIR} prefix is no
# brace). A relative PAIR candidate whose directory is unknown goes to
# TRAVERSAL_WARN; a WIDENED one that cannot be resolved stays allowed.
check_path_traversal_resolved() {
    TRAVERSAL_WARN=() TRAVERSAL_PATH=""
    local n=${#LOOM_TOKENS[@]} i tok kind part cwd abs glob_re='[*?[{\\][^/]*/(.*/)?\.\.(/|$)'
    local scr='${LOOM_SCRATCH_DIR}'
    for ((i = 0; i < n; i++)); do
        tok="${LOOM_TOKENS[$i]}"
        [[ "$tok" != "%%SEP%%" ]] || continue
        kind=$(traversal_kind "$tok")
        [[ -n "$kind" ]] || continue
        part=$(traversal_path_part "$tok")
        cwd="${LOOM_TOKEN_CWD[$i]:-}"
        if [[ -z "$cwd" && "$part" != [/~\$]* ]]; then
            if [[ "$kind" == pair ]]; then TRAVERSAL_WARN+=("$tok"); fi
            continue
        fi
        # Residual: a WIDENED glob before one `..` (`cat .loom/w*/../x`) passes; it reaches only main/.loom.
        if [[ "${part#"$scr"}" =~ $glob_re ]] || ! abs=$(loom_normalize_path "$cwd" "$part"); then
            [[ "$kind" == pair ]] || continue
            TRAVERSAL_PATH="$tok (which cannot be resolved)"
            return 0
        fi
        case "$(loom_path_class "$abs")" in worktree | scratch) continue ;; esac
        TRAVERSAL_PATH="$abs (from $tok)"
        return 0
    done
    return 1
}

# print_traversal_warning - emit one LOOM_HOOK_WARN additionalContext naming
# every TRAVERSAL_WARN token, or nothing when there is none.
print_traversal_warning() {
    ((${#TRAVERSAL_WARN[@]} > 0)) || return 0
    local list="" tok
    for tok in "${TRAVERSAL_WARN[@]}"; do list="${list:+$list, }$tok"; done
    jq -nc --arg ctx "LOOM_HOOK_WARN: worktree-isolation cannot tell which directory runs $list (a subshell, pipe, sh -c or a cd that may fail comes first), so its traversal was not checked. Use an absolute path inside your worktree instead." \
        '{hookSpecificOutput:{hookEventName:"PreToolUse",additionalContext:$ctx}}'
}

# Pattern 3 (token path): returns 0 when a token names another stage's
# worktree. A word token counts when it CONTAINS ".worktrees/"; a
# whitespace-bearing one (quoted prose) only when it STARTS with it, so
# `cat ".worktrees/other/some notes.md"` blocks while a brief mentioning the
# path mid-sentence does not. Exempt: tokens naming CURRENT_STAGE's worktree,
# and an absolute token with a known cwd resolving to class worktree or
# scratch (a scratch dir whose parent path holds a .worktrees/ component).
# The empty-array guard keeps bash 3.2 `set -u` quiet.
check_cross_worktree_tokens() {
    local n=${#LOOM_TOKENS[@]}
    ((n > 0)) || return 1
    local own_re="${LOOM_POLICY_WORKTREE_MARKER}${CURRENT_STAGE}(/|\$)"
    local i tok abs cwd prefix="^$LOOM_POLICY_WORKTREE_MARKER"
    for ((i = 0; i < n; i++)); do
        tok="${LOOM_TOKENS[$i]}"
        if loom_token_is_word "$tok"; then
            [[ "$tok" =~ $LOOM_POLICY_WORKTREE_MARKER ]] || continue
        else
            [[ "$tok" =~ $prefix ]] || continue
        fi
        [[ "$tok" =~ $own_re ]] && continue
        cwd="${LOOM_TOKEN_CWD[$i]:-}"
        if [[ "$tok" == /* && -n "$cwd" ]] && abs=$(loom_normalize_word "$cwd" "$tok"); then
            case "$(loom_path_class "$abs")" in worktree | scratch) continue ;; esac
        fi
        return 0
    done
    return 1
}

# A block from any pattern wins over a traversal warning.
validate_bash_command_token_path() {
    loom_track_cwd "$(jq -r '.cwd // empty' <<<"$INPUT_JSON" 2>/dev/null || true)"
    if check_git_override_tokens; then
        BLOCK_RULE=git-override
        print_git_override_block
        return 1
    fi

    if check_path_traversal_resolved; then
        BLOCK_RULE=path-traversal
        print_path_traversal_block "$TRAVERSAL_PATH"
        return 1
    fi

    if check_cross_worktree_tokens; then
        BLOCK_RULE=cross-worktree
        print_cross_worktree_block
        return 1
    fi

    print_traversal_warning
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
    if loom_policy_git_fallback "$stripped"; then
        BLOCK_RULE=git-override
        print_git_override_block
        return 1
    fi

    # Pattern 2: Block ../../ path traversal (escaping worktree)
    if [[ "$stripped" =~ $LOOM_POLICY_PARENT_PAIR ]]; then
        BLOCK_RULE=path-traversal
        print_path_traversal_block "a ../../ path"
        return 1
    fi

    # Pattern 3: Block .worktrees/ access (except current worktree)
    # Allow references to current stage, block others
    if echo "$stripped" | grep -qE "$LOOM_POLICY_WORKTREE_MARKER" && \
       ! echo "$stripped" | grep -qE "${LOOM_POLICY_WORKTREE_MARKER}${CURRENT_STAGE}[/[:space:]]|${LOOM_POLICY_WORKTREE_MARKER}${CURRENT_STAGE}\$"; then
        BLOCK_RULE=cross-worktree
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
                loom_record_block worktree-isolation "${BLOCK_RULE:-unknown}" Bash "$COMMAND"
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
