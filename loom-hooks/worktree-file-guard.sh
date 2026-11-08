#!/usr/bin/env bash
# worktree-file-guard.sh - canonical PreToolUse guard for every file tool
#
# Read, Write, Edit, MultiEdit, Glob, Grep, and NotebookEdit all pass through
# this guard. Paths are resolved component-by-component against the canonical
# worktree root so an absolute host path, a symlink leaf, or a sibling
# sharing the worktree's string prefix cannot cross the boundary.
#
# Input: JSON from stdin (Claude Code passes tool info via stdin)
# Exit codes: 0 = allow, 2 = block (including jq not installed - fail closed)

set -euo pipefail
source "$(dirname "$0")/_common.sh"
loom_require_jq "worktree-file-guard.sh"

source "$(dirname "${BASH_SOURCE[0]}")/_bounded.sh"
INPUT_JSON=$(loom_read_input 5 2>/dev/null || true)

TOOL_NAME=$(printf '%s' "$INPUT_JSON" | jq -r '.tool_name // empty' 2>/dev/null || true)

if [[ -n "$INPUT_JSON" && -z "$TOOL_NAME" ]]; then
	loom_record_block "worktree-file-guard" "bad-payload" "unknown" ""
	cat >&2 <<EOF

[worktree-file-guard/bad-payload] LOOM: BLOCKED - File operation metadata could not be parsed
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
source "$(dirname "${BASH_SOURCE[0]}")/_worktree_policy.sh" || exit 2
loom_load_worktree_policy || exit 2

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

# block_path <rule> <original> <reason> - record the block in the stage block
# ledger, print the banner and exit 2.
block_path() {
	local rule="$1" original="$2" reason="$3"
	loom_record_block "worktree-file-guard" "$rule" "$TOOL_NAME" "$original"
	cat >&2 <<EOF

[worktree-file-guard/$rule] LOOM: BLOCKED - File operation crossed the worktree policy
============================================================

Tool: $TOOL_NAME
Path: $original
Reason: $reason

Shared orchestration state is read-only to file tools, except for the
explicitly permitted shared write roots.
============================================================

EOF
	exit 2
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

# owned_by_uid <path> - <path> exists, is not a symlink, and belongs to the current uid.
owned_by_uid() {
	local owner
	[[ -e "$1" && ! -L "$1" ]] || return 1
	owner=$(stat -c '%u' "$1" 2>/dev/null || stat -f '%u' "$1" 2>/dev/null || true)
	[[ -n "$owner" && "$owner" == "$(id -u)" ]]
}

# payload_session_id - print the payload session_id when it is a plain id.
payload_session_id() {
	local session
	session=$(printf '%s' "$INPUT_JSON" | jq -r '.session_id // empty' 2>/dev/null || true)
	[[ "$session" =~ ^[A-Za-z0-9_-]+$ ]] || return 1
	printf '%s' "$session"
}

# owned_plain_file <root> <rest> - <root>/<rest> is a uid-owned regular file with
# one link, reached through uid-owned real directories, no symlink, empty or `.`.
owned_plain_file() {
	local path="$1" rest="$2" comp mode links
	owned_by_uid "$path" && [[ -d "$path" ]] || return 1
	while [[ -n "$rest" ]]; do
		comp=${rest%%/*}
		[[ "$rest" == */* ]] && rest=${rest#*/} || rest=""
		[[ -n "$comp" && "$comp" != . && "$comp" != .. ]] || return 1
		path+="/$comp"
		owned_by_uid "$path" || return 1
		[[ -z "$rest" || -d "$path" ]] || return 1
	done
	[[ -f "$path" ]] || return 1
	# GNU and BSD `ls -n` both print the link count second; stat formats differ.
	read -r mode links rest <<<"$(ls -ldn -- "$path" 2>/dev/null)" || return 1
	[[ "$links" == 1 ]]
}

# Read of ~/.claude/projects/<slug>/<payload session_id>/tool-results/<file>, the
# harness output of this session and its subagents (they share the session_id).
# A symlinked HOME or ~/.claude resolves first; nothing below it may be a link.
allow_tool_results() {
	local lexical="$1" session raw projects rest
	[[ "$TOOL_NAME" == "Read" && "${HOME:-}" == /* ]] || return 1
	session=$(payload_session_id) || return 1
	raw="$HOME/.claude/projects"
	projects=$(canonical_existing "$raw" 2>/dev/null) && [[ -n "$projects" ]] || return 1
	case "$lexical" in
	"$projects"/*) rest=${lexical#"$projects"/} ;;
	"$raw"/*) rest=${lexical#"$raw"/} ;;
	*) return 1 ;;
	esac
	[[ "$rest" =~ ^[^/]+/${session}/tool-results/[^/]+$ ]] || return 1
	owned_plain_file "$projects" "$rest" || return 1
	[[ "$(canonical_existing "$lexical" 2>/dev/null)" == "$projects/$rest" ]]
}

# Read of a plain file of ours under the literal /tmp/claude-<uid> (macOS
# /private/tmp), Bash's TMPDIR; hooks never see that TMPDIR. Another session's
# <root>/<project>/<uuid> stays closed; Grep and Glob would recurse into it.
allow_tmpdir() {
	local lexical="$1" uid root rest re session canon
	[[ "$TOOL_NAME" == "Read" ]] || return 1
	uid=$(id -u) || return 1
	root="/tmp/claude-$uid"
	[[ "$lexical" == "/private$root"/* ]] && root="/private$root"
	[[ "$lexical" == "$root"/* ]] || return 1
	rest=${lexical#"$root"/}
	session=$(payload_session_id) || session=""
	re='^[^/]+/([0-9a-f]{8}(-[0-9a-f]{4}){3}-[0-9a-f]{12})/'
	[[ "$rest" =~ $re && "${BASH_REMATCH[1]}" != "$session" ]] && return 1
	owned_plain_file "$root" "$rest" || return 1
	canon=$(canonical_existing "$root" 2>/dev/null) && [[ -n "$canon" ]] || return 1
	[[ "$(canonical_existing "$lexical" 2>/dev/null)" == "$canon/$rest" ]]
}

allow_background_output() {
	local lexical="$1"
	[[ "$TOOL_NAME" == "Read" ]] || return 1
	[[ "$lexical" =~ ^/tmp/claude-[^/]+/[^/]+/[^/]+/tasks/[^/]+\.output$ ]] || return 1
	[[ -f "$lexical" && ! -L "$lexical" ]] || return 1

	local resolved
	resolved=$(canonical_existing "$lexical") || return 1
	[[ "$resolved" =~ ^/tmp/claude-[^/]+/[^/]+/[^/]+/tasks/[^/]+\.output$ ]] || return 1
	owned_by_uid "$resolved"
}

# Any file tool may use the session's own harness scratchpad (/tmp/claude-<uid>/<project>/
# <payload session_id>/scratchpad/) when it exists, the path is already canonical (no
# symlink, `.` or `//` component), and each existing component from claude-<uid> is ours.
allow_scratchpad() {
	local lexical=${1%/} uid session re root dir rest resolved
	uid=$(id -u) || return 1
	session=$(payload_session_id) || return 1
	re="^((/private)?/tmp/claude-${uid})/([^/]+)/${session}/scratchpad(/.*)?\$"
	[[ "$lexical" =~ $re ]] || return 1
	root=${BASH_REMATCH[1]}
	[[ -d "$root/${BASH_REMATCH[3]}/$session/scratchpad" ]] || return 1
	resolved=$(canonical_target "$lexical" 2>/dev/null) || return 1
	[[ "$resolved" == "$lexical" || "$resolved" == "/private$lexical" ]] || return 1
	dir=$root rest=${lexical#"$root"/}
	while owned_by_uid "$dir"; do
		[[ -n "$rest" ]] || return 0
		dir+="/${rest%%/*}"
		[[ "$rest" == */* ]] && rest=${rest#*/} || rest=""
		[[ -e "$dir" || -L "$dir" ]] || return 0
	done
	return 1
}

# Any file tool may use the session's loom relay scratch directory
# ($LOOM_SCRATCH_DIR), where judges draft verdicts and writers leave notes. It is
# trusted under the rules `relay::scratch::validate_session_dir` applies: a real
# directory named for $LOOM_SESSION_ID and owned by us; the target must resolve
# inside it.
allow_loom_scratch() {
	local lexical=${1%/} scratch=${LOOM_SCRATCH_DIR:-} resolved
	scratch=${scratch%/}
	[[ "$scratch" == /* && -n "${LOOM_SESSION_ID:-}" ]] || return 1
	[[ "${scratch##*/}" == "$LOOM_SESSION_ID" && -d "$scratch" ]] || return 1
	owned_by_uid "$scratch" || return 1
	[[ "$lexical" == "$scratch" || "$lexical" == "$scratch"/* ]] || return 1
	scratch=$(canonical_existing "$scratch") || return 1
	resolved=$(canonical_target "$lexical" 2>/dev/null) || return 1
	[[ "$resolved" == "$scratch" || "$resolved" == "$scratch"/* ]]
}

FILE_PATH=$(extract_path)
if [[ -z "$FILE_PATH" ]]; then
	case "$TOOL_NAME" in
	Glob | Grep) FILE_PATH="." ;;
	*) block_path missing-path "<missing>" "required file_path metadata is missing" ;;
	esac
fi

if [[ "$FILE_PATH" == *$'\n'* || "$FILE_PATH" == *$'\r'* ]]; then
	block_path control-chars "$FILE_PATH" "control characters are not valid in guarded paths"
fi

# Reject every parent component. This deliberately fails closed even when a
# lexical `..` would normalize back into the worktree.
if [[ "$FILE_PATH" =~ $LOOM_POLICY_PARENT_COMPONENT ]]; then
	block_path parent-component "$FILE_PATH" "parent-directory components are forbidden"
fi

if [[ "$FILE_PATH" == /* ]]; then
	LEXICAL_PATH="$FILE_PATH"
elif [[ "$FILE_PATH" == "~" ]]; then
	[[ -n "${HOME:-}" ]] || block_path tilde "$FILE_PATH" "HOME is unavailable for tilde expansion"
	LEXICAL_PATH="$HOME"
elif [[ "$FILE_PATH" == "~/"* ]]; then
	[[ -n "${HOME:-}" ]] || block_path tilde "$FILE_PATH" "HOME is unavailable for tilde expansion"
	LEXICAL_PATH="$HOME/${FILE_PATH#\~/}"
elif [[ "$FILE_PATH" == "~"* ]]; then
	block_path tilde "$FILE_PATH" "named-user tilde expansion is not supported safely"
else
	CURRENT_DIR=$(pwd -P) || exit 2
	LEXICAL_PATH="$CURRENT_DIR/$FILE_PATH"
fi

if allow_background_output "$LEXICAL_PATH" || allow_scratchpad "$LEXICAL_PATH" ||
	allow_loom_scratch "$LEXICAL_PATH" || allow_tool_results "$LEXICAL_PATH" ||
	allow_tmpdir "$LEXICAL_PATH"; then
	exit 0
fi

# The shared-state symlink is .loom/work under the current layout; a
# worktree created from a legacy .work workspace keeps .work forever (see
# doc/plans, "Back-compat"). Prefer .loom/work and fall back to .work.
WORK_LINK="$WORKTREE_PATH/.loom/work"
if [[ ! -e "$WORK_LINK" && ! -L "$WORK_LINK" ]]; then
	WORK_LINK="$WORKTREE_PATH/.work"
fi
WORK_SHARED=""
if [[ -e "$WORK_LINK" || -L "$WORK_LINK" ]]; then
	WORK_SHARED=$(canonical_existing "$WORK_LINK" 2>/dev/null || true)
fi

# A caller-provided symlink leaf is never followed. The one exception is the
# exact loom-owned state-directory link (.loom/work or legacy .work) when
# used as an explicit trusted read root.
LEAF_PATH="${LEXICAL_PATH%/}"
if [[ -L "$LEAF_PATH" ]]; then
	if [[ "$LEAF_PATH" == "$WORK_LINK" && "$TOOL_NAME" =~ ^(Read|Glob|Grep)$ && -n "$WORK_SHARED" ]]; then
		exit 0
	fi
	block_path symlink-leaf "$FILE_PATH" "symlink leaf targets are forbidden"
fi

RESOLVED_PATH=$(canonical_target "$LEXICAL_PATH" 2>/dev/null || true)
if [[ -z "$RESOLVED_PATH" ]]; then
	block_path unresolvable "$FILE_PATH" "path could not be resolved safely"
fi

# A skill's SKILL.md is exempt from containment for Read, Glob and Grep only:
# the loom-skills loader reads $HOME/.claude/loom-skill-catalog/<name>/SKILL.md
# and $HOME/.claude/skills/<name>/SKILL.md whole from inside a worktree.
# Anchored to those two roots with <name> EXACTLY one component, matched against
# RESOLVED_PATH. A suffix glob (`*.claude/skills/*/SKILL.md`) would match any
# directory ending in `.claude` and a multi-component <name>. Each root is
# canonicalized first, so a symlinked HOME or ~/.claude still matches; a missing
# root, or an unset HOME, disables the exemption. _read_discipline.sh's
# `_loom_is_skill_md_path` keeps the looser glob on purpose: it gates a warning.
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

if is_within "$RESOLVED_PATH" "$WORKTREE_PATH"; then
	exit 0
fi

if [[ -n "$WORK_SHARED" ]] && is_within "$RESOLVED_PATH" "$WORK_SHARED"; then
	RELATIVE_WORK_PATH="${RESOLVED_PATH#"$WORK_SHARED"}"
	case "$RELATIVE_WORK_PATH" in
	/admin.token | /user.token)
		block_path capability-token "$FILE_PATH" "orchestrator capability tokens are not readable by stages"
		;;
	esac

	case "$TOOL_NAME" in
	Read | Glob | Grep) exit 0 ;;
	Write | Edit | MultiEdit | NotebookEdit)
		if loom_policy_shared_write_allowed "$RELATIVE_WORK_PATH"; then
			exit 0
		fi
		block_path shared-write "$FILE_PATH" "shared orchestration state is not writable by file tools"
		;;
	esac
fi

block_path outside-worktree "$FILE_PATH" "resolved path is outside the current worktree"
