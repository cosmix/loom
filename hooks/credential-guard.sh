#!/usr/bin/env bash
# credential-guard.sh - PreToolUse guard keeping the native file tools out of
# credential files.
#
# This is the replacement for the `Read(...)` permission deny rules loom used
# to write into settings. Claude Code's Bash path validator (verified against
# 2.1.259) refuses `rg`, `grep`, `egrep`, `fgrep`, `diff`, `git`, `cp` and `mv`
# with a bypass-immune operator prompt whenever ANY settings file carries ANY
# `permissions.deny` entry beginning with `Read(`, if the command names a
# relative path in a compound command containing `cd`. Loom emitted seven such
# rules, so every stage session paid that prompt; the rules are gone.
#
# What is NOT gone is the boundary they described. `sandbox.filesystem.denyRead`
# stays exactly as it is - it is an OS-level sandbox list, not a permission
# rule, it does not trigger the Bash path check, and it keeps Bash out of the
# credentials. It binds Bash and nothing else, though: the native file tools
# (Read, Glob, Grep, Edit, MultiEdit, Write, NotebookEdit) never touch the
# sandbox. This hook applies the same boundary to them, so the two layers
# together cover what the `Read(...)` denies used to cover alone.
#
# Two rules, in order:
#   (a) hardcoded: `admin.token` / `user.token` directly under a state root
#       (`.loom/work`, or legacy `.work`). Hardcoded so a missing, renamed or
#       hand-edited settings file can never let an orchestrator capability
#       token through.
#   (b) the project's own `sandbox.filesystem.denyRead` list, applied to the
#       file tools. A missing or unparsable settings file contributes nothing
#       here and leaves rule (a) standing.
#
# Input: JSON from stdin (Claude Code passes tool info via stdin)
# Exit codes: 0 = allow, 2 = block (including jq not installed - fail closed)

set -euo pipefail
source "$(dirname "$0")/_common.sh"
loom_require_jq "credential-guard.sh"

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

Retry the file operation so the credential policy can validate its path.
============================================================

EOF
	exit 2
fi

case "$TOOL_NAME" in
Read | Glob | Grep | Edit | MultiEdit | Write | NotebookEdit) ;;
*) exit 0 ;;
esac

PROJECT_DIR="${CLAUDE_PROJECT_DIR:-$PWD}"

# --- Canonicalization --------------------------------------------------------
#
# Copied from worktree-file-guard.sh rather than factored into _common.sh: the
# two are the only callers, they differ in leaf handling (that guard REFUSES a
# symlink leaf, this one must FOLLOW it), and a helper with one other caller
# earns nothing. Resolve the deepest existing ancestor and re-append the
# missing remainder, so a worktree's `.loom/work/admin.token` (or legacy
# `.work/admin.token`) resolves THROUGH the state-root symlink to the main
# repo's real file - which is the whole point of rule (a).

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

# normalize_lexical <absolute-path> - Collapse `.` and `..` components the way
# Node's path.resolve does, matching how Claude Code's Write tool has already
# normalized the path by the time it opens the file. Without this, a target
# with a `..` after a component that does not yet exist (e.g.
# `~/.ssh/missing/../authorized_keys`) makes canonical_target below give up
# entirely - TARGET comes back empty and the "unresolvable is allowed" case a
# few lines down waves it through, even though the tool itself resolves the
# same string to `~/.ssh/authorized_keys`. Prints the normalized path on
# success; a `..` with no component left to pop climbs above the filesystem
# root and returns 1 instead, for the caller to block outright.
normalize_lexical() {
	local path="$1"
	local -a parts=() raw_components
	local component

	IFS='/' read -ra raw_components <<<"$path"
	for component in "${raw_components[@]}"; do
		case "$component" in
		"" | ".") continue ;;
		"..")
			[[ ${#parts[@]} -gt 0 ]] || return 1
			parts=("${parts[@]:0:$((${#parts[@]} - 1))}")
			;;
		*)
			parts+=("$component")
			;;
		esac
	done

	local result="/" part
	for part in "${parts[@]}"; do
		result="$result$part/"
	done
	[[ "$result" == "/" ]] || result="${result%/}"
	printf '%s' "$result"
}

block_target() {
	local original="$1"
	local resolved="$2"
	local reason="$3"
	cat >&2 <<EOF

============================================================
  LOOM: BLOCKED - Credential path is closed to file tools
============================================================

Tool: $TOOL_NAME
Path: $original
Resolved: $resolved
Reason: $reason

This path is denied at both layers that reach it: the OS sandbox keeps Bash
out of it (sandbox.filesystem.denyRead in .claude/settings.local.json) and
this hook keeps the file tools out of it. There is no permission to approve
and no prompt to accept - the file is simply not readable from a session.
Use a different path.
============================================================

EOF
}

# --- Target extraction -------------------------------------------------------

RAW_TARGET=$(printf '%s' "$INPUT_JSON" |
	jq -r '.tool_input.file_path // .tool_input.notebook_path // .tool_input.path // empty' \
		2>/dev/null || true)

if [[ -z "$RAW_TARGET" ]]; then
	case "$TOOL_NAME" in
	# A search with no explicit root searches the project root.
	Glob | Grep) RAW_TARGET="$PROJECT_DIR" ;;
	# Anything else without a path is not a path decision this guard owns;
	# worktree-file-guard.sh is the hook that rejects missing metadata.
	*) exit 0 ;;
	esac
fi

if [[ "$RAW_TARGET" == /* ]]; then
	LEXICAL_TARGET="$RAW_TARGET"
elif [[ "$RAW_TARGET" == "~" ]]; then
	[[ -n "${HOME:-}" ]] || {
		block_target "$RAW_TARGET" "<unresolved>" "HOME is unavailable for tilde expansion"
		exit 2
	}
	LEXICAL_TARGET="$HOME"
elif [[ "$RAW_TARGET" == "~/"* ]]; then
	[[ -n "${HOME:-}" ]] || {
		block_target "$RAW_TARGET" "<unresolved>" "HOME is unavailable for tilde expansion"
		exit 2
	}
	LEXICAL_TARGET="$HOME/${RAW_TARGET#\~/}"
else
	LEXICAL_TARGET="$PWD/$RAW_TARGET"
fi

# Strip a trailing slash so a directory target compares as its own path rather
# than as an empty child of itself.
if [[ "$LEXICAL_TARGET" != "/" ]]; then
	LEXICAL_TARGET="${LEXICAL_TARGET%/}"
fi

# Resolve `.`/`..` lexically before canonicalization, since the Write tool
# already has by the time it opens the file - see normalize_lexical above. A
# `..` that climbs above `/` is blocked here outright.
if ! LEXICAL_TARGET=$(normalize_lexical "$LEXICAL_TARGET"); then
	block_target "$RAW_TARGET" "<unresolved>" "path climbs above the filesystem root"
	exit 2
fi

# An unresolvable path is ALLOWED here on purpose, for the corner cases that
# remain after the normalization above - an empty leaf, or an ancestor that is
# itself unreadable. `..` is no longer among them: normalize_lexical already
# resolved every `..` component lexically, or blocked a climb above `/`.
# worktree-file-guard.sh still blocks anything that reaches this guard
# unresolvable; this guard must never be the one that stops an ordinary edit
# over a remaining path-resolution corner case.
TARGET=$(canonical_target "$LEXICAL_TARGET" 2>/dev/null || true)
[[ -n "$TARGET" ]] || exit 0

# --- Rule (a): the orchestrator capability tokens -----------------------------

TARGET_BASE="${TARGET##*/}"
TARGET_PARENT="${TARGET%/*}"
case "$TARGET_BASE" in
admin.token | user.token)
	case "$TARGET_PARENT" in
	*/.work | */.loom/work)
		block_target "$RAW_TARGET" "$TARGET" \
			"orchestrator capability tokens are never readable by a session"
		exit 2
		;;
	esac
	;;
esac

# --- Rule (b): the project's own sandbox deny-read list -----------------------

# expand_deny_entry <entry> - Echo the absolute path a denyRead entry names.
# Claude Code's own convention: a single leading `/` is PROJECT-relative and a
# leading `//` is absolute (https://code.claude.com/docs/en/permissions.md).
# A bare relative entry is project-relative too. Returns 1 for a `~` entry that
# cannot be expanded, so the caller skips it instead of rooting it at `/`.
expand_deny_entry() {
	local entry="$1"
	case "$entry" in
	"~")
		[[ -n "${HOME:-}" ]] || return 1
		printf '%s' "$HOME"
		;;
	"~/"*)
		[[ -n "${HOME:-}" ]] || return 1
		printf '%s/%s' "$HOME" "${entry#\~/}"
		;;
	//*) printf '/%s' "${entry#//}" ;;
	/*) printf '%s/%s' "$PROJECT_DIR" "${entry#/}" ;;
	*) printf '%s/%s' "$PROJECT_DIR" "$entry" ;;
	esac
}

# split_pattern <absolute-pattern> - Split at the FIRST path component holding a
# glob metacharacter, leaving the wildcard-free head in SPLIT_PREFIX and the
# rest in SPLIT_REMAINDER (empty when the pattern has no wildcard at all). Only
# the head can be canonicalized; the remainder has to stay a pattern.
SPLIT_PREFIX=""
SPLIT_REMAINDER=""
split_pattern() {
	local pattern="$1"
	local prefix="" rest="$pattern" component
	SPLIT_PREFIX=""
	SPLIT_REMAINDER=""

	if [[ "$pattern" == /* ]]; then
		prefix="/"
		rest="${pattern#/}"
	fi

	while [[ -n "$rest" ]]; do
		component="${rest%%/*}"
		case "$component" in
		*'*'* | *'?'* | *'['*)
			SPLIT_PREFIX="$prefix"
			SPLIT_REMAINDER="$rest"
			return 0
			;;
		esac
		if [[ -z "$prefix" || "$prefix" == "/" ]]; then
			prefix="$prefix$component"
		else
			prefix="$prefix/$component"
		fi
		if [[ "$rest" == */* ]]; then
			rest="${rest#*/}"
		else
			rest=""
		fi
	done

	SPLIT_PREFIX="$prefix"
	SPLIT_REMAINDER=""
}

# pattern_matches <target> <canonical-prefix> <remainder>
#
# A wildcard-free pattern, and a `/**` or `/*` tail, both name a directory AND
# everything under it - the containment test. Any other remainder is matched as
# a bash glob, where `*` deliberately crosses `/`: these are deny rules, so the
# wider reading is the safe one.
#
# Containment is also what gives Glob/Grep the right answer for free: the
# SEARCH ROOT is the target, so a search rooted ABOVE a denied directory is not
# contained by it and runs, exactly as the old `Read(...)` denies behaved.
pattern_matches() {
	local target="$1" prefix="$2" remainder="$3" joined child_prefix
	case "$remainder" in
	"" | "**" | "*")
		# A root prefix would otherwise build the child test as `//*` and
		# match nothing - the unsafe direction for a deny rule.
		child_prefix="$prefix"
		if [[ "$child_prefix" == "/" ]]; then
			child_prefix=""
		fi
		[[ "$target" == "$prefix" || "$target" == "$child_prefix"/* ]]
		return
		;;
	esac
	if [[ "$prefix" == "/" ]]; then
		joined="/$remainder"
	else
		joined="$prefix/$remainder"
	fi
	[[ "$target" == $joined ]]
}

# deny_read_blocks <target> - Return 0 when some `sandbox.filesystem.denyRead`
# entry covers <target>, publishing the offending entry in DENY_ENTRY.
DENY_ENTRY=""
deny_read_blocks() {
	local target="$1"
	local settings="$PROJECT_DIR/.claude/settings.local.json"
	[[ -f "$settings" && -r "$settings" ]] || return 1

	local entries
	entries=$(jq -r '.sandbox.filesystem.denyRead[]? | select(type == "string")' \
		"$settings" 2>/dev/null || true)
	[[ -n "$entries" ]] || return 1

	local entry expanded canonical_prefix
	while IFS= read -r entry; do
		[[ -n "$entry" ]] || continue
		# Loom never emits a parent-traversal entry
		# (`sandbox::settings::policy::deny_read_patterns` filters `../` out),
		# and one cannot be resolved meaningfully against a pattern root here.
		case "$entry" in *"../"*) continue ;; esac

		expanded=$(expand_deny_entry "$entry") || continue
		[[ -n "$expanded" ]] || continue

		split_pattern "$expanded"
		canonical_prefix=$(canonical_existing "$SPLIT_PREFIX" 2>/dev/null || true)
		[[ -n "$canonical_prefix" ]] || canonical_prefix="$SPLIT_PREFIX"

		if pattern_matches "$target" "$canonical_prefix" "$SPLIT_REMAINDER"; then
			DENY_ENTRY="$entry"
			return 0
		fi
	done <<<"$entries"

	return 1
}

if deny_read_blocks "$TARGET"; then
	block_target "$RAW_TARGET" "$TARGET" \
		"sandbox.filesystem.denyRead entry '$DENY_ENTRY' covers this path"
	exit 2
fi

exit 0
