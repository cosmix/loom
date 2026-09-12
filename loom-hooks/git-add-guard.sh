#!/usr/bin/env bash
# PreToolUse hook: Block dangerous git add patterns that would stage loom's
# state directory - the legacy `.work` or the current `.loom/work`.
#
# This hook intercepts Bash tool calls and blocks:
# - git add -A / git add --all (stages everything including the state dir)
# - git add . (stages current directory including the state dir)
# - git add .work / git add .loom/work (explicitly staging the state dir)
#
# Exit codes:
#   0 - Allow the command
#   2 - Block with guidance message
#   2 - jq not installed (fail closed)
#
# Debug mode:
#   Set GIT_ADD_GUARD_DEBUG=1 to see what patterns are being checked
#
# Test cases for Pattern 3 (state directory detection):
#   SHOULD BLOCK:
#     git add .work            (direct legacy .work)
#     git add .work/           (directory)
#     git add .work/foo        (subpath)
#     git add foo .work bar    (.work as middle argument)
#     git add .work other      (.work followed by other files)
#     git add .loom/work       (direct current .loom/work)
#     git add .loom/work/      (directory)
#     git add .loom/work/foo   (subpath)
#   SHOULD ALLOW:
#     git add .workspace     (.work is substring, not standalone)
#     git add .working       (.work is substring)
#     git add .workdir       (.work is substring)
#     git add .loom/cache    (.loom state that isn't the shared work symlink)
#     git add doc/foo.md     (no state directory at all)
#     git add network.md     (no state directory at all)
#
# Test cases for quoting (the command is tokenized with loom_tokenize_command
# before scanning, so quoting changes ARGUMENT VALUES, not what gets matched):
#   SHOULD BLOCK:
#     git add '-A'            (quoted real argument - value is still -A)
#     git add ".work"         (quoted real argument - value is still .work)
#     cargo build && git add -A   (git add reached through a separator)
#     echo $(git add -A)          (git add inside a command substitution)
#   SHOULD ALLOW:
#     echo 'Never run git add -A or git add . because it stages .work'
#         (prose INSIDE a quoted argument - no git invocation exists here)
#     the same prose inside double quotes
#     a multi-line `git commit -m "...Co-Authored-By: ..."` body (the
#         message text is one quoted argument to -m, never a git-add argument)

set -euo pipefail

# Source shared utilities for strip_embedded_content()
source "$(dirname "$0")/_common.sh"
loom_require_jq "git-add-guard.sh"

# Debug helper
debug() {
    if [[ "${GIT_ADD_GUARD_DEBUG:-}" == "1" ]]; then
        echo "[git-add-guard DEBUG] $*" >&2
    fi
}

# Read stdin JSON (Claude Code provides tool input)
# Cross-platform timeout: gtimeout (macOS+coreutils), timeout (Linux), or plain cat
if command -v gtimeout &>/dev/null; then
    INPUT_JSON=$(gtimeout 1 cat 2>/dev/null || true)
elif command -v timeout &>/dev/null; then
    INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)
else
    INPUT_JSON=$(cat 2>/dev/null || true)
fi

# Extract tool name and command
TOOL_NAME=$(echo "$INPUT_JSON" | jq -r '.tool_name // empty' 2>/dev/null || true)
COMMAND=$(echo "$INPUT_JSON" | jq -r '.tool_input.command // empty' 2>/dev/null || true)
debug "Tool: $TOOL_NAME, Command: $COMMAND"

# Only process Bash tool calls
if [[ "$TOOL_NAME" != "Bash" ]] || [[ -z "$COMMAND" ]]; then
    debug "Skipping: not a Bash tool call"
    exit 0
fi

# scan_git_add_tokens - Walk the global LOOM_TOKENS array (as produced by a
# prior loom_tokenize_command call: real argv tokens plus "%%SEP%%"
# command-boundary sentinels) looking for a `git add` invocation whose
# arguments would stage everything or stage .work.
#
# Command-word resolution reuses loom_tokens_command_word_index from
# _common.sh - the same helper commit-filter.sh's checks use - instead of a
# bespoke walk: it already skips leading VAR=value environment assignments
# AND unwraps wrapper commands (sudo, env, xargs, time, nohup, command,
# nice, stdbuf, timeout, gtimeout - including each wrapper's OWN option
# words, e.g. `env -u NAME` or `env FOO=1`), so `sudo git add -A` and
# `env FOO=1 git add .` now resolve to `git` exactly like every other hook's
# checks. The previous bespoke check here (`tok == "git" || tok == */git`
# after only an env-assignment skip) had no wrapper awareness at all, so
# both of those slipped through entirely.
#
# Everything AFTER the effective `git` command word - git's own global
# options (-C <dir>, -c <cfg> skipped with their value; any other -flag
# skipped bare), locating the `add` subcommand, then add's own arguments
# (--all, a combined short flag containing A, `.`, `.work`/`.work/*`, with
# `--` only suppressing the -A/--all flag checks, matching git's own
# semantics that `--` still leaves `.`/`.work` as positional arguments) is
# still a bespoke, git-add-specific walk: none of that generalizes into
# _common.sh's shared helpers, which know how to find an invoking command
# and its argv, not how to walk a specific subcommand's own option grammar.
#
# The state-directory check itself matches both the legacy `.work` symlink
# and the current `.loom/work` symlink - a workspace created before this
# migration keeps `.work` forever (see doc/plans, "Back-compat"), so both
# names must stay blocked.
#
# Returns 1 (block) if a dangerous `git add` invocation is found, 0 (allow)
# otherwise.
scan_git_add_tokens() {
    local n=${#LOOM_TOKENS[@]}
    local i=0
    local at_cmd_pos=1
    local j gt at
    local found_add seen_dashdash

    while ((i < n)); do
        if [[ "${LOOM_TOKENS[$i]}" == "%%SEP%%" ]]; then
            at_cmd_pos=1
            i=$((i + 1))
            continue
        fi

        if [[ $at_cmd_pos -eq 1 ]] && j=$(loom_tokens_command_word_index "$i") && [[ "${LOOM_TOKENS[$j]##*/}" == "git" ]]; then
            i=$((j + 1))
            found_add=0
            while ((i < n)); do
                gt="${LOOM_TOKENS[$i]}"
                [[ "$gt" == "%%SEP%%" ]] && break
                if [[ "$gt" == "-C" || "$gt" == "-c" ]]; then
                    i=$((i + 2))
                    continue
                fi
                if [[ "$gt" == "add" ]]; then
                    found_add=1
                    i=$((i + 1))
                    break
                fi
                if [[ "$gt" == -* ]]; then
                    i=$((i + 1))
                    continue
                fi
                # Some other git subcommand, not `add` - leave i pointing
                # at it and stop looking.
                break
            done

            if [[ $found_add -eq 1 ]]; then
                seen_dashdash=0
                while ((i < n)); do
                    at="${LOOM_TOKENS[$i]}"
                    [[ "$at" == "%%SEP%%" ]] && break
                    if [[ $seen_dashdash -eq 0 && "$at" == "--" ]]; then
                        seen_dashdash=1
                        i=$((i + 1))
                        continue
                    fi
                    if [[ $seen_dashdash -eq 0 ]]; then
                        if [[ "$at" == "--all" ]]; then
                            debug "BLOCKED by token scan: git add --all"
                            return 1
                        fi
                        if [[ "$at" =~ ^-[a-zA-Z]*A[a-zA-Z]*$ ]]; then
                            debug "BLOCKED by token scan: git add $at"
                            return 1
                        fi
                    fi
                    if [[ "$at" == "." ]]; then
                        debug "BLOCKED by token scan: git add ."
                        return 1
                    fi
                    if [[ "$at" == ".work" || "$at" == .work/* || "$at" == ".loom/work" || "$at" == .loom/work/* ]]; then
                        debug "BLOCKED by token scan: git add $at"
                        return 1
                    fi
                    i=$((i + 1))
                done
                at_cmd_pos=0
                continue
            fi
        fi

        at_cmd_pos=0
        i=$((i + 1))
    done

    return 0
}

# Check for dangerous git add patterns
check_dangerous_patterns() {
    local cmd="$1"

    # Strip heredoc bodies and -m/--message content to avoid false positives.
    # A heredoc body is not quoted, so its words would otherwise tokenize as
    # real command tokens.
    local stripped
    stripped=$(strip_embedded_content "$cmd")

    if loom_tokenize_command "$stripped"; then
        # ${LOOM_TOKENS[*]} is only safe to expand when the array is
        # non-empty: bash 3.2 (this file's header targets macOS) errors on
        # "${arr[*]}" for an EMPTY array under `set -u`, and the argument is
        # expanded at the call site regardless of whether debug() actually
        # prints it. ${#LOOM_TOKENS[@]} alone is always safe.
        if ((${#LOOM_TOKENS[@]} > 0)); then
            debug "Tokenized into ${#LOOM_TOKENS[@]} token(s): ${LOOM_TOKENS[*]}"
        else
            debug "Tokenized into 0 token(s)"
        fi
        if ! scan_git_add_tokens; then
            return 1
        fi
        debug "ALLOWED: token scan found no dangerous git add pattern"
        return 0
    fi

    # Fallback: the command has an unterminated quote, so it is not valid
    # bash anyway and loom_tokenize_command could not produce a trustworthy
    # token list. Fall back to the regex patterns this hook used before
    # tokenizing existed, so today's protection is never weaker than it was.
    debug "Tokenizer reported an unterminated quote - falling back to the regex scan"

    # Normalize the stripped version: remove extra whitespace
    local normalized
    normalized=$(echo "$stripped" | tr -s ' ')
    debug "Checking command: $normalized"

    # Arguments belonging to ONE `git add` invocation: everything up to a command
    # separator or end of line.
    #
    # This bound is load-bearing. In bash's =~ a `.` matches newlines, so an
    # unbounded `.*` lets text from a LATER line satisfy these patterns - and
    # strip_embedded_content only removes single-line -m bodies, so a multi-line
    # commit message survives into the scan. The result was that staging specific
    # files was blocked whenever the message body happened to contain "-A" (as in
    # "Co-Authored-By") or the string ".work", with a diagnostic naming patterns
    # the command never used.
    local args=$'[^;&|\n]*'

    # Pattern 1: git add -A or git add --all (flag must be its own token, so a
    # path like src/-Analysis.rs or a word like Co-Authored does not match)
    if [[ "$normalized" =~ git[[:space:]]+add${args}[[:space:]](-A|--all)([[:space:]]|[;\&\|]|$) ]]; then
        debug "BLOCKED by Pattern 1: git add -A/--all"
        return 1
    fi

    # Pattern 2: git add . (stages current directory)
    # Match "git add ." but not "git add ./file" or "git add .gitignore"
    if [[ "$normalized" =~ git[[:space:]]+add[[:space:]]+\.[[:space:]]*$ ]] || \
       [[ "$normalized" =~ git[[:space:]]+add[[:space:]]+\.[[:space:]]+[^/] ]] || \
       [[ "$normalized" =~ git[[:space:]]+add[[:space:]]+\.[[:space:]]*\&\& ]]; then
        debug "BLOCKED by Pattern 2: git add ."
        return 1
    fi

    # Pattern 3: Explicitly staging the state directory (legacy .work or
    # current .loom/work).
    # Match .work ONLY as a standalone argument (not as substring of longer name)
    # .work must be followed by: space, forward slash, or end of string
    # This prevents false positives for: .workspace, .working, .workdir, etc.
    if [[ "$normalized" =~ git[[:space:]]+add${args}\.work([[:space:]]|/|[;\&\|]|$) ]]; then
        debug "BLOCKED by Pattern 3: .work directory"
        return 1
    fi

    # Pattern 4: Explicitly staging .loom/work (the current state directory)
    if [[ "$normalized" =~ git[[:space:]]+add${args}\.loom/work([[:space:]]|/|[;\&\|]|$) ]]; then
        debug "BLOCKED by Pattern 4: .loom/work directory"
        return 1
    fi

    debug "ALLOWED: No dangerous patterns detected"
    return 0
}

# Check the command
if ! check_dangerous_patterns "$COMMAND"; then
    # Block with guidance
    cat >&2 <<'EOF'

============================================================
  LOOM: BLOCKED - Dangerous git add pattern detected
============================================================

Your command would stage loom's state directory (.work or .loom/work)
which MUST NOT be committed.

BLOCKED PATTERNS:
  - git add -A / git add --all  (stages everything)
  - git add .                    (stages current directory)
  - git add .work                (explicitly stages the legacy state dir)
  - git add .loom/work           (explicitly stages the current state dir)

CORRECT PATTERN:
  git add <specific-files>

Example:
  git add src/main.rs src/lib.rs

WHY: In worktrees, .work / .loom/work is a symlink to shared state.
     Committing it corrupts the main repository for all parallel stages.

============================================================

EOF
    exit 2
fi

# Allow the command
exit 0
