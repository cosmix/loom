#!/usr/bin/env bash
# PreToolUse hook: Block plan files written to .claude/plans paths
#
# Claude Code's plan mode suggests saving plans under ~/.claude/plans/ or
# ~/.claude/projects/<project>/plans/. Files there are invisible to loom and
# to git, so the plan is effectively lost. This hook intercepts Write/Edit
# calls targeting any such path and redirects to doc/plans/PLAN-<description>.md.
#
# Unlike worktree-file-guard.sh this hook is unconditional: it fires in
# interactive sessions too, because that is where plan mode runs.
#
# Blocked (matched as path segments, relative or absolute):
#   .claude/plans/...
#   .claude/projects/<anything>/plans/...
#
# Exit codes:
#   0 - Allow the operation
#   2 - Block with guidance message

set -euo pipefail

# Read stdin JSON (Claude Code provides tool input)
# Cross-platform timeout: gtimeout (macOS+coreutils), timeout (Linux), or plain cat
if command -v gtimeout &>/dev/null; then
    INPUT_JSON=$(gtimeout 1 cat 2>/dev/null || true)
elif command -v timeout &>/dev/null; then
    INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)
else
    INPUT_JSON=$(cat 2>/dev/null || true)
fi

TOOL_NAME=$(echo "$INPUT_JSON" | jq -r '.tool_name // empty' 2>/dev/null || true)
FILE_PATH=$(echo "$INPUT_JSON" | jq -r '.tool_input.file_path // empty' 2>/dev/null || true)

# Only Write/Edit carry a file_path we need to validate
if [[ "$TOOL_NAME" != "Write" && "$TOOL_NAME" != "Edit" ]] || [[ -z "$FILE_PATH" ]]; then
    exit 0
fi

# Match .claude/plans and .claude/projects/*/plans as path segments so that
# names like .claude/plans-archive or my.claude/plans do not false-positive.
if [[ "$FILE_PATH" =~ (^|/)\.claude/plans(/|$) ]] ||
    [[ "$FILE_PATH" =~ (^|/)\.claude/projects/[^/]+/plans(/|$) ]]; then
    cat >&2 <<'EOF'

============================================================
  LOOM: BLOCKED - Plan targeted at a .claude/plans path
============================================================

Plans belong in the repository: ./doc/plans/PLAN-<description>.md

Files under ~/.claude/plans/ or ~/.claude/projects/*/plans/ are
invisible to loom and to git - the plan would be lost.

Plan mode suggests these paths by default. Override it: retry this
Write/Edit with the same content at doc/plans/PLAN-<description>.md
(relative to the repository root).

============================================================

EOF
    exit 2
fi

exit 0
