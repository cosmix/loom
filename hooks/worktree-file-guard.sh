#!/usr/bin/env bash
# worktree-file-guard.sh - canonical PreToolUse guard for every file tool
#
# Read, Write, Edit, MultiEdit, Glob, Grep, and NotebookEdit all pass through
# this guard. Paths are resolved component-by-component against the canonical
# worktree root so an absolute host path, a symlink leaf, or a sibling
# sharing the worktree's string prefix cannot cross the boundary.
#
# Input: JSON from stdin (Claude Code passes tool info via stdin)
# Exit codes: 0 = allow, 2 = block

set -euo pipefail
source "$(dirname "$0")/_common.sh"

if command -v gtimeout &>/dev/null; then
	INPUT_JSON=$(gtimeout 5 cat 2>/dev/null || true)
elif command -v timeout &>/dev/null; then
	INPUT_JSON=$(timeout 5 cat 2>/dev/null || true)
else
	INPUT_JSON=$(cat 2>/dev/null || true)
fi

TOOL_NAME=$(printf '%s' "$INPUT_JSON" | jq -r '.tool_name // empty' 2>/dev/null || true)

if [[ -n "$INPUT_JSON" && -z "$TOOL_NAME" ]]; then
	cat >&2 <<EOF

============================================================
  LOOM: BLOCKED - File operation metadata could not be parsed
============================================================

Reason: received non-empty tool input that did not contain a valid tool name.

Retry the file operation so the worktree policy can validate its path.
============================================================

EOF
	exit 2
fi

case "$TOOL_NAME" in
Read | Write | Edit | MultiEdit | Glob | Grep | NotebookEdit) ;;
*) exit 0 ;;
esac

WORKTREE_PATH=$(loom_current_worktree) || exit 0
WORKTREE_PATH=$(cd "$WORKTREE_PATH" 2>/dev/null && pwd -P) || exit 2

is_within() {
	local target="$1"
	local root="$2"
	[[ "$target" == "$root" || "$target" == "$root/"* ]]
}

canonical_existing() {
	local path="$1"
	if command -v realpath &>/dev/null; then
		realpath "$path" 2>/dev/null
		return
	fi
	if command -v readlink &>/dev/null; then
		local resolved
		resolved=$(readlink -f "$path" 2>/dev/null || true)
		if [[ -n "$resolved" ]]; then
			printf '%s' "$resolved"
			return 0
		fi
	fi

	if [[ -d "$path" ]]; then
		(cd "$path" 2>/dev/null && pwd -P)
	else
		local parent leaf
		parent=$(dirname "$path")
		leaf=$(basename "$path")
		parent=$(cd "$parent" 2>/dev/null && pwd -P) || return 1
		printf '%s/%s' "$parent" "$leaf"
	fi
}

# Resolve an existing target completely. For a not-yet-created Write target,
# resolve the nearest existing ancestor and retain only plain child components.
canonical_target() {
	local path="$1"
	if [[ -e "$path" || -L "$path" ]]; then
		canonical_existing "$path"
		return
	fi

	local ancestor="$path"
	local suffix=""
	local parent leaf
	while [[ ! -e "$ancestor" && ! -L "$ancestor" ]]; do
		parent=$(dirname "$ancestor")
		leaf=$(basename "$ancestor")
		[[ "$leaf" != "." && "$leaf" != ".." && -n "$leaf" ]] || return 1
		if [[ -n "$suffix" ]]; then
			suffix="$leaf/$suffix"
		else
			suffix="$leaf"
		fi
		[[ "$parent" != "$ancestor" ]] || return 1
		ancestor="$parent"
	done

	local resolved
	resolved=$(canonical_existing "$ancestor") || return 1
	if [[ -n "$suffix" ]]; then
		printf '%s/%s' "$resolved" "$suffix"
	else
		printf '%s' "$resolved"
	fi
}

block_path() {
	local original="$1"
	local reason="$2"
	cat >&2 <<EOF

============================================================
  LOOM: BLOCKED - File operation crossed the worktree policy
============================================================

Tool: $TOOL_NAME
Path: $original
Reason: $reason

Use a path inside the current worktree. Shared orchestration state is
read-only to file tools, except for the explicit handoff write root.
============================================================

EOF
}

extract_path() {
	case "$TOOL_NAME" in
	Read | Write | Edit | MultiEdit)
		# MultiEdit's per-edit list hangs off one `file_path`, the same field
		# Read/Write/Edit use, so it needs no extraction of its own.
		printf '%s' "$INPUT_JSON" | jq -r '.tool_input.file_path // empty' 2>/dev/null || true
		;;
	NotebookEdit)
		# NotebookEdit's tool input field is `notebook_path`, not `file_path`.
		# Fall back to `file_path` too so a future field rename still resolves.
		printf '%s' "$INPUT_JSON" | jq -r '.tool_input.notebook_path // .tool_input.file_path // empty' 2>/dev/null || true
		;;
	Glob | Grep)
		printf '%s' "$INPUT_JSON" | jq -r '.tool_input.path // "."' 2>/dev/null || true
		;;
	esac
}

allow_background_output() {
	local lexical="$1"
	[[ "$TOOL_NAME" == "Read" ]] || return 1
	[[ "$lexical" =~ ^/tmp/claude-[^/]+/[^/]+/[^/]+/tasks/[^/]+\.output$ ]] || return 1
	[[ -f "$lexical" && ! -L "$lexical" ]] || return 1

	local resolved owner
	resolved=$(canonical_existing "$lexical") || return 1
	[[ "$resolved" =~ ^/tmp/claude-[^/]+/[^/]+/[^/]+/tasks/[^/]+\.output$ ]] || return 1
	owner=$(stat -c '%u' "$resolved" 2>/dev/null || stat -f '%u' "$resolved" 2>/dev/null || true)
	[[ -n "$owner" && "$owner" == "$(id -u)" ]]
}

FILE_PATH=$(extract_path)
if [[ -z "$FILE_PATH" ]]; then
	case "$TOOL_NAME" in
	Glob | Grep) FILE_PATH="." ;;
	*) block_path "<missing>" "required file_path metadata is missing"; exit 2 ;;
	esac
fi

if [[ "$FILE_PATH" == *$'\n'* || "$FILE_PATH" == *$'\r'* ]]; then
	block_path "$FILE_PATH" "control characters are not valid in guarded paths"
	exit 2
fi

# Reject every parent component. This deliberately fails closed even when a
# lexical `..` would normalize back into the worktree.
case "/$FILE_PATH/" in
*/../*) block_path "$FILE_PATH" "parent-directory components are forbidden"; exit 2 ;;
esac

if [[ "$FILE_PATH" == /* ]]; then
	LEXICAL_PATH="$FILE_PATH"
elif [[ "$FILE_PATH" == "~" ]]; then
	[[ -n "${HOME:-}" ]] || { block_path "$FILE_PATH" "HOME is unavailable for tilde expansion"; exit 2; }
	LEXICAL_PATH="$HOME"
elif [[ "$FILE_PATH" == "~/"* ]]; then
	[[ -n "${HOME:-}" ]] || { block_path "$FILE_PATH" "HOME is unavailable for tilde expansion"; exit 2; }
	LEXICAL_PATH="$HOME/${FILE_PATH#\~/}"
elif [[ "$FILE_PATH" == "~"* ]]; then
	block_path "$FILE_PATH" "named-user tilde expansion is not supported safely"
	exit 2
else
	CURRENT_DIR=$(pwd -P) || exit 2
	LEXICAL_PATH="$CURRENT_DIR/$FILE_PATH"
fi

if allow_background_output "$LEXICAL_PATH"; then
	exit 0
fi

WORK_LINK="$WORKTREE_PATH/.work"
WORK_SHARED=""
if [[ -e "$WORK_LINK" || -L "$WORK_LINK" ]]; then
	WORK_SHARED=$(canonical_existing "$WORK_LINK" 2>/dev/null || true)
fi

# A caller-provided symlink leaf is never followed. The one exception is the
# exact loom-owned `.work` link when used as an explicit trusted read root.
LEAF_PATH="${LEXICAL_PATH%/}"
if [[ -L "$LEAF_PATH" ]]; then
	if [[ "$LEAF_PATH" == "$WORK_LINK" && "$TOOL_NAME" =~ ^(Read|Glob|Grep)$ && -n "$WORK_SHARED" ]]; then
		exit 0
	fi
	block_path "$FILE_PATH" "symlink leaf targets are forbidden"
	exit 2
fi

RESOLVED_PATH=$(canonical_target "$LEXICAL_PATH" 2>/dev/null || true)
if [[ -z "$RESOLVED_PATH" ]]; then
	block_path "$FILE_PATH" "path could not be resolved safely"
	exit 2
fi

# A skill's SKILL.md is exempt from worktree containment for read-class tools
# only (Read, Glob, Grep - never Write/Edit/MultiEdit/NotebookEdit, which stay
# confined). The catalog splits skills across two roots under $HOME -
# $HOME/.claude/loom-skill-catalog/<name>/SKILL.md and
# $HOME/.claude/skills/<name>/SKILL.md - and both are meant to be read whole
# from inside a worktree: the loom-skills loader reaches them with the Read
# tool (skills/loom-skills/SKILL.md, allowed-tools: [Read]), which this hook
# gates on every worktree stage session.
#
# Anchored to the two real roots built from $HOME, with <name> constrained to
# EXACTLY one path component, matched against RESOLVED_PATH (after `..`
# rejection and symlink/ancestor resolution above) - NOT a bare path-suffix
# glob. A suffix glob like `*.claude/skills/*/SKILL.md` lets `*` cross `/` on
# both sides: it would match any directory anywhere merely ENDING in
# `.claude` (e.g. `~/.ssh/evil.claude/skills/a/SKILL.md`) and let `<name>`
# itself span several components, widening worktree containment to an
# attacker-chosen path. If HOME is unset or empty the exemption does not
# apply at all, rather than degrading to a bare-prefix match.
#
# Each root is canonicalized ONCE per call with canonical_existing - the SAME
# helper canonical_target used above to produce RESOLVED_PATH - before the
# comparison. RESOLVED_PATH has every symlink component already resolved, so
# comparing it against the RAW "$HOME/.claude/skills" string never matches
# when $HOME itself traverses a symlink (autofs, a `/home/x -> /data/x`
# layout, a dotfiles-managed ~/.claude) - that would silently re-block all 53
# catalogued skills, the exact bug this exemption exists to fix. A root that
# does not exist (e.g. a --skills all install with no catalog directory) is
# SKIPPED outright rather than compared as a raw string, which would reopen
# the escape closed above.
#
# hooks/_read_discipline.sh:105's `_loom_is_skill_md_path` keeps the looser
# suffix-glob shape deliberately - it gates a read-discipline WARNING, not
# worktree containment, so the wider match there is not a boundary widening.
# The two diverge on purpose; keep both in mind when touching either.
is_skill_md_path() {
	local path="$1" home="${HOME:-}" raw_root canon_root rest name
	[[ -n "$home" ]] || return 1
	for raw_root in "$home/.claude/loom-skill-catalog" "$home/.claude/skills"; do
		canon_root=$(canonical_existing "$raw_root" 2>/dev/null || true)
		[[ -n "$canon_root" ]] || continue
		[[ "$path" == "$canon_root"/*/SKILL.md ]] || continue
		rest="${path#"$canon_root"/}"
		name="${rest%/SKILL.md}"
		[[ -n "$name" && "$name" != */* ]] && return 0
	done
	return 1
}

case "$TOOL_NAME" in
Read | Glob | Grep)
	if is_skill_md_path "$RESOLVED_PATH"; then
		exit 0
	fi
	;;
esac

# Knowledge files are recorded through the `loom knowledge update` CLI, not
# file tools — the sandbox itself no longer enforces this (the CLI runs
# inside it too, so a sandbox deny would block the CLI as well as the file
# tools; see `sandbox::config::apply_knowledge_write_grant`). This hook is
# the doctrine's replacement enforcement point. Reads stay allowed (an agent
# legitimately reads its own knowledge base); only write-class tools are
# gated, and only inside this worktree — main-repo knowledge/knowledge-distill
# stages cannot be gated reliably here (never gate on LOOM_STAGE_ID).
KNOWLEDGE_DIR="$WORKTREE_PATH/doc/loom/knowledge"
case "$TOOL_NAME" in
Write | Edit | MultiEdit | NotebookEdit)
	if is_within "$RESOLVED_PATH" "$KNOWLEDGE_DIR"; then
		block_path "$FILE_PATH" "knowledge files are recorded through \`loom knowledge update\`, not file tools"
		exit 2
	fi
	;;
esac

if is_within "$RESOLVED_PATH" "$WORKTREE_PATH"; then
	exit 0
fi

if [[ -n "$WORK_SHARED" ]] && is_within "$RESOLVED_PATH" "$WORK_SHARED"; then
	RELATIVE_WORK_PATH="${RESOLVED_PATH#"$WORK_SHARED"}"
	case "$RELATIVE_WORK_PATH" in
	/admin.token | /user.token)
		block_path "$FILE_PATH" "orchestrator capability tokens are not readable by stages"
		exit 2
		;;
	esac

	case "$TOOL_NAME" in
	Read | Glob | Grep) exit 0 ;;
	Write | Edit | MultiEdit | NotebookEdit)
		if [[ "$RELATIVE_WORK_PATH" == /handoffs/* ]]; then
			exit 0
		fi
		block_path "$FILE_PATH" "shared orchestration state is not writable by file tools"
		exit 2
		;;
	esac
fi

block_path "$FILE_PATH" "resolved path is outside the current worktree"
exit 2
