#!/usr/bin/env bash
# PreToolUse hook: Block Write/Edit/Task/Agent once a stage is completed/verified
#
# `loom stage complete` is meant to be a session's LAST act (CLAUDE.md hard
# stop 3, Rule 4): the merge starts from the completed commit, so any edit or
# subagent spawn AFTER completion is lost work. commit-guard.sh (Stop hook)
# is only advisory and can be talked past; this hook is the hard enforcement
# — it blocks the next Write, Edit, Task, or Agent call once the stage's own
# status file says the stage is already completed or verified.
#
# `completed-with-failures` (and any other non-terminal status) MUST allow:
# that status means the agent is still expected to keep fixing and re-run
# `loom stage complete` — blocking it would strand the session.
#
# Exit codes:
#   0 - Allow the operation
#   2 - Block with guidance message
#
# Environment:
#   LOOM_MERGE_SESSION=1 - merge resolution sessions are exempt (see commit-guard.sh)

set -euo pipefail

# Configuration (mirrors commit-guard.sh)
readonly WORKTREE_MARKER=".worktrees/"
readonly LOOM_BRANCH_PREFIX="loom/"
# The state directory name is resolved per-root, not fixed: the current
# layout is .loom/work, but a workspace created before this migration keeps
# the legacy .work forever (see doc/plans, "Back-compat"). Set by
# resolve_work_dir_name() once the project root is known.
WORK_DIR=""

debug_log() {
	if [[ "${LOOM_HOOK_DEBUG:-0}" == "1" ]]; then
		printf "[stage-terminal-guard] %s\n" "$*" >&2
	fi
}

# Read JSON input from stdin (drain to prevent blocking, matching sibling
# PreToolUse guards)
if command -v gtimeout &>/dev/null; then
	INPUT_JSON=$(gtimeout 1 cat 2>/dev/null || true)
elif command -v timeout &>/dev/null; then
	INPUT_JSON=$(timeout 1 cat 2>/dev/null || true)
else
	INPUT_JSON=$(cat 2>/dev/null || true)
fi
debug_log "INPUT_JSON: $INPUT_JSON"

# --- Functions copied verbatim from commit-guard.sh ---------------------
# Kept as independent copies (like commit-guard.sh keeps its own), not
# sourced, so this hook has no load-order dependency on another script.

# Detect if running in a loom worktree
# Returns: 0 if in worktree, 1 otherwise
# Sets: STAGE_ID variable if in worktree
detect_loom_worktree() {
	local cwd
	cwd=$(pwd)
	debug_log "Checking for loom worktree in: $cwd"

	# Method 1: Check if path contains .worktrees/
	if [[ "$cwd" == *"$WORKTREE_MARKER"* ]]; then
		# Extract stage ID from path: /path/to/.worktrees/<stage-id>/...
		local worktree_part="${cwd#*$WORKTREE_MARKER}"
		STAGE_ID="${worktree_part%%/*}"
		debug_log "Detected worktree via path, stage ID: $STAGE_ID"
		return 0
	fi

	# Method 2: Check git branch name
	local branch
	if branch=$(git rev-parse --abbrev-ref HEAD 2>/dev/null); then
		debug_log "Current git branch: $branch"
		if [[ "$branch" == "$LOOM_BRANCH_PREFIX"* ]]; then
			STAGE_ID="${branch#$LOOM_BRANCH_PREFIX}"
			debug_log "Detected worktree via branch prefix, stage ID: $STAGE_ID"
			return 0
		fi
	fi

	debug_log "Not in a loom worktree"
	return 1
}

# Resolve the state directory name under a given root: current .loom/work
# takes priority, falling back to the legacy .work. Echoes empty if neither
# exists.
resolve_work_dir_name() {
	local root="$1"
	if [[ -d "$root/.loom/work" ]]; then
		echo ".loom/work"
	elif [[ -d "$root/.work" ]]; then
		echo ".work"
	fi
}

# Find the project root (where the state directory is: .loom/work, or the
# legacy .work). Searches upward from current directory.
#
# Called as `project_root=$(find_project_root)`, which runs in a subshell -
# any variable this function assigned would be lost when that subshell
# exits, so it only communicates via stdout. Callers resolve WORK_DIR
# separately with resolve_work_dir_name() once project_root is known.
find_project_root() {
	local dir
	dir=$(pwd)
	debug_log "Searching for project root from: $dir"

	while [[ "$dir" != "/" ]]; do
		if [[ -d "$dir/.loom/work" || -d "$dir/.work" ]]; then
			debug_log "Found project root at: $dir"
			echo "$dir"
			return 0
		fi
		dir=$(dirname "$dir")
	done

	# Also check if we're in a worktree - root is 2 levels up from worktree
	dir=$(pwd)
	if [[ "$dir" == *"$WORKTREE_MARKER"* ]]; then
		local root="${dir%%$WORKTREE_MARKER*}"
		if [[ -d "$root/.loom/work" || -d "$root/.work" ]]; then
			debug_log "Found project root via worktree path at: $root"
			echo "$root"
			return 0
		fi
	fi

	debug_log "Could not find project root (state directory not found)"
	return 1
}

# Parse stage status from stage file YAML frontmatter
# Args: $1 = path to stage file
# Returns: status string or empty if not found
get_stage_status() {
	local stage_file="$1"

	if [[ ! -f "$stage_file" ]]; then
		debug_log "get_stage_status: file not found: $stage_file"
		echo ""
		return
	fi

	# Parse YAML frontmatter for status field
	# Frontmatter is between --- markers
	local in_frontmatter=0
	local status=""

	while IFS= read -r line; do
		if [[ "$line" == "---" ]]; then
			if [[ $in_frontmatter -eq 0 ]]; then
				in_frontmatter=1
			else
				break # End of frontmatter
			fi
			continue
		fi

		if [[ $in_frontmatter -eq 1 ]]; then
			# Match status: <value>
			if [[ "$line" =~ ^status:\ *(.+)$ ]]; then
				status="${BASH_REMATCH[1]}"

				# Strip inline comments (everything after #)
				status="${status%%#*}"

				# Trim whitespace
				status="${status#"${status%%[![:space:]]*}"}"
				status="${status%"${status##*[![:space:]]}"}"

				# Strip surrounding quotes (single or double)
				if [[ "$status" =~ ^\"(.*)\"$ ]] || [[ "$status" =~ ^\'(.*)\'$ ]]; then
					status="${BASH_REMATCH[1]}"
				fi

				debug_log "get_stage_status: parsed status='$status' from line: $line"
				break
			fi
		fi
	done <"$stage_file"

	debug_log "get_stage_status: final status='$status'"
	echo "$status"
}

# Find the stage file for a given stage ID
# Args: $1 = project root, $2 = stage ID
# Matches Rust logic: files are named NN-stage-id.md (e.g., 01-my-stage.md)
# Uses exact matching to avoid false positives (e.g., "fix" matching "fix-bug")
find_stage_file() {
	local project_root="$1"
	local stage_id="$2"
	local stages_path="$project_root/$WORK_DIR/stages"
	debug_log "find_stage_file: looking for stage '$stage_id' in: $stages_path"

	# Check if stages directory exists and is accessible
	if [[ ! -d "$stages_path" ]]; then
		debug_log "find_stage_file: stages directory does not exist: $stages_path"
		echo ""
		return
	fi

	# Check if the state directory is a symlink and accessible
	if [[ -L "$project_root/$WORK_DIR" ]]; then
		debug_log "find_stage_file: $WORK_DIR is a symlink"
		if [[ ! -e "$project_root/$WORK_DIR" ]]; then
			debug_log "find_stage_file: $WORK_DIR symlink is broken/inaccessible"
			echo ""
			return
		fi
	fi

	# Exact match: NN-<stage-id>.md (Rust naming convention)
	# Pattern: digits followed by dash, then exact stage-id, then .md
	for file in "$stages_path"/*.md; do
		if [[ ! -f "$file" ]]; then
			continue
		fi

		local basename
		basename=$(basename "$file")

		# Match pattern: NN-stage-id.md (depth prefix + exact stage id)
		# Strip the numeric prefix and dash, check if remainder matches stage-id.md
		if [[ "$basename" =~ ^[0-9]+-(.+)\.md$ ]]; then
			local extracted_id="${BASH_REMATCH[1]}"
			if [[ "$extracted_id" == "$stage_id" ]]; then
				debug_log "find_stage_file: found exact match: $file"
				echo "$file"
				return
			fi
		fi

		# Also check for exact match without prefix: stage-id.md
		if [[ "$basename" == "${stage_id}.md" ]]; then
			debug_log "find_stage_file: found exact match (no prefix): $file"
			echo "$file"
			return
		fi
	done

	debug_log "find_stage_file: no exact match found for stage '$stage_id'"
	echo ""
}

# --- Main hook logic ------------------------------------------------------

main() {
	debug_log "=== stage-terminal-guard starting ==="
	debug_log "CWD: $(pwd)"

	# Merge resolution sessions are exempt - they run on the main repo, own no
	# worktree, and their stage's status transition is exactly what they exist
	# to perform.
	if [[ "${LOOM_MERGE_SESSION:-}" == "1" ]]; then
		debug_log "Merge session detected (LOOM_MERGE_SESSION=1) - allowing"
		exit 0
	fi

	local STAGE_ID=""
	if ! detect_loom_worktree; then
		debug_log "Not in loom worktree - allowing"
		exit 0
	fi

	local project_root
	if ! project_root=$(find_project_root); then
		debug_log "Project root not found (.loom/work and .work both missing) - allowing"
		exit 0
	fi
	WORK_DIR=$(resolve_work_dir_name "$project_root")

	local stage_file
	stage_file=$(find_stage_file "$project_root" "$STAGE_ID")
	if [[ -z "$stage_file" ]]; then
		debug_log "No stage file found for '$STAGE_ID' - allowing"
		exit 0
	fi

	local status
	status=$(get_stage_status "$stage_file")
	debug_log "Stage '$STAGE_ID' status: $status"

	case "$status" in
	completed | Completed | verified | Verified)
		debug_log "Stage is $status - BLOCKING"
		;;
	*)
		debug_log "Stage status '$status' is not terminal - allowing"
		exit 0
		;;
	esac

	cat >&2 <<EOF

============================================================
  LOOM: BLOCKED - stage already $status
============================================================

BLOCKED: stage '$STAGE_ID' is already $status. \`loom stage complete\` was this session's LAST act -
nothing runs after it. This edit/spawn will NOT be merged: the merge starts from the completed
commit, so post-completion work is LOST WORK. End the session now.
If the stage was completed prematurely (subagents still out, defects unfixed), record the gap:
  loom memory note "completed prematurely: <what is missing>"
then end the session; recovery belongs to the orchestrator (loom stage retry / a new stage),
not to this session.

============================================================

EOF
	exit 2
}

main "$@"
