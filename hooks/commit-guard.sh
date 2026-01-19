#!/usr/bin/env bash
# Stop hook: Enforce commit and stage completion in loom worktrees
#
# This hook runs when Claude finishes responding. In loom worktrees, it enforces:
# 1. All changes must be committed before ending
# 2. Stage must be marked complete (not left in "Executing" state)
#
# Exit codes:
#   0 - Allow Claude to stop (no issues or not in worktree)
#   2 - Block and return error to Claude (uncommitted changes or stage incomplete)
#
# Output format when blocking:
#   {"continue": false, "reason": "..."}

set -euo pipefail

# Drain stdin to prevent blocking (Stop hooks receive JSON from Claude Code)
timeout 1 cat >/dev/null 2>&1 || true

# Configuration
readonly WORKTREE_MARKER=".worktrees/"
readonly LOOM_BRANCH_PREFIX="loom/"
readonly WORK_DIR=".work"
readonly STAGES_DIR="$WORK_DIR/stages"
readonly KNOWLEDGE_STAGE_PATTERN="knowledge"

# Debug logging - enabled when LOOM_HOOK_DEBUG=1
debug_log() {
	if [[ "${LOOM_HOOK_DEBUG:-0}" == "1" ]]; then
		printf "[loom-stop] %s\n" "$*" >&2
	fi
}

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

# Check if current stage is a knowledge stage
# Knowledge stages don't require commits - they only update doc/loom/knowledge/
# Returns: 0 if knowledge stage, 1 otherwise
# Args: $1 = project root, $2 = stage ID
is_knowledge_stage() {
	local project_root="$1"
	local stage_id="$2"

	debug_log "Checking if stage '$stage_id' is a knowledge stage"

	# Method 1: Check if stage ID contains "knowledge" (case-insensitive)
	local stage_id_lower
	stage_id_lower=$(echo "$stage_id" | tr '[:upper:]' '[:lower:]')
	if [[ "$stage_id_lower" == *"$KNOWLEDGE_STAGE_PATTERN"* ]]; then
		debug_log "Stage ID contains '$KNOWLEDGE_STAGE_PATTERN' - is knowledge stage"
		return 0
	fi

	# Method 2: Check stage file for stage_type: knowledge field
	local stage_file
	stage_file=$(find_stage_file "$project_root" "$stage_id")

	if [[ -n "$stage_file" && -f "$stage_file" ]]; then
		# Parse YAML frontmatter for stage_type field
		local in_frontmatter=0
		local stage_type=""

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
				# Match stage_type: <value>
				if [[ "$line" =~ ^stage_type:\ *(.+)$ ]]; then
					stage_type="${BASH_REMATCH[1]}"
					# Trim whitespace
					stage_type="${stage_type#"${stage_type%%[![:space:]]*}"}"
					stage_type="${stage_type%"${stage_type##*[![:space:]]}"}"
					break
				fi
			fi
		done <"$stage_file"

		local stage_type_lower
		stage_type_lower=$(echo "$stage_type" | tr '[:upper:]' '[:lower:]')
		if [[ "$stage_type_lower" == "$KNOWLEDGE_STAGE_PATTERN" ]]; then
			debug_log "Stage file has stage_type: $KNOWLEDGE_STAGE_PATTERN - is knowledge stage"
			return 0
		fi
	fi

	debug_log "Stage '$stage_id' is not a knowledge stage"
	return 1
}

# Find the project root (where .work directory is)
# Searches upward from current directory
find_project_root() {
	local dir
	dir=$(pwd)
	debug_log "Searching for project root from: $dir"

	while [[ "$dir" != "/" ]]; do
		if [[ -d "$dir/$WORK_DIR" ]]; then
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
		if [[ -d "$root/$WORK_DIR" ]]; then
			debug_log "Found project root via worktree path at: $root"
			echo "$root"
			return 0
		fi
	fi

	debug_log "Could not find project root (.work directory not found)"
	return 1
}

# Check for uncommitted changes (staged or unstaged)
# Returns: 0 if clean, 1 if dirty
check_git_clean() {
	# Check for any changes (staged, unstaged, or untracked)
	# --porcelain gives machine-readable output
	# Empty output means clean
	local status
	status=$(git status --porcelain 2>/dev/null || echo "")

	if [[ -z "$status" ]]; then
		debug_log "Git status: clean (no uncommitted changes)"
		return 0 # Clean
	else
		debug_log "Git status: dirty (has uncommitted changes)"
		return 1 # Dirty
	fi
}

# Get list of uncommitted changes for error message
get_uncommitted_changes() {
	git status --porcelain 2>/dev/null | head -10
}

# Parse stage status from stage file YAML frontmatter
# Args: $1 = path to stage file
# Returns: status string or empty if not found
# Handles:
#   - Quoted values: status: "completed"
#   - Inline comments: status: completed  # done
#   - Leading/trailing whitespace
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
	local stages_path="$project_root/$STAGES_DIR"
	debug_log "find_stage_file: looking for stage '$stage_id' in: $stages_path"

	# Check if stages directory exists and is accessible
	if [[ ! -d "$stages_path" ]]; then
		debug_log "find_stage_file: stages directory does not exist: $stages_path"
		echo ""
		return
	fi

	# Check if .work is a symlink and accessible
	if [[ -L "$project_root/$WORK_DIR" ]]; then
		debug_log "find_stage_file: .work is a symlink"
		if [[ ! -e "$project_root/$WORK_DIR" ]]; then
			debug_log "find_stage_file: .work symlink is broken/inaccessible"
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

# Output blocking JSON and exit
# Args: $1 = reason string
block_with_reason() {
	local reason="$1"
	# Escape special characters in reason for JSON
	reason="${reason//\\/\\\\}"   # Escape backslashes
	reason="${reason//\"/\\\"}"   # Escape quotes
	reason="${reason//$'\n'/\\n}" # Escape newlines
	reason="${reason//$'\r'/}"    # Remove carriage returns

	printf '{"continue": false, "reason": "%s"}\n' "$reason"
	exit 2
}

# Non-blocking reminder about knowledge capture
# Called after blocking checks pass, outputs to stderr
remind_knowledge_capture() {
	local project_root="$1"

	# Only show if knowledge directory exists
	local knowledge_dir="$project_root/$WORK_DIR/knowledge"
	if [[ ! -d "$knowledge_dir" ]]; then
		return
	fi

	# Check if any knowledge file has content beyond template
	local has_content=false
	for file in entry-points.md patterns.md conventions.md; do
		local filepath="$knowledge_dir/$file"
		if [[ -f "$filepath" ]]; then
			# Check if file has more than just template content (>10 lines)
			local lines
			lines=$(wc -l <"$filepath")
			if [[ "$lines" -gt 15 ]]; then
				has_content=true
				break
			fi
		fi
	done

	# Always show reminder (soft prompt, not blocking)
	printf '\n' >&2
	printf '%s\n' "------------------------------------------------------------" >&2
	printf '%s\n' "Knowledge Capture Reminder" >&2
	printf '%s\n' "------------------------------------------------------------" >&2
	printf '%s\n' "Did you discover anything worth sharing with future sessions?" >&2
	printf '%s\n' "" >&2
	printf '%s\n' "  loom knowledge update entry-points \"## Section\\n- file - desc\"" >&2
	printf '%s\n' "  loom knowledge update patterns \"## Pattern\\n- description\"" >&2
	printf '%s\n' "  loom knowledge update conventions \"## Convention\\n- details\"" >&2
	printf '%s\n' "------------------------------------------------------------" >&2
}

# Main hook logic
main() {
	debug_log "=== loom-stop hook starting ==="
	debug_log "CWD: $(pwd)"
	debug_log "LOOM_HOOK_DEBUG: ${LOOM_HOOK_DEBUG:-0}"

	# Check if we're in a loom worktree
	local STAGE_ID=""
	if ! detect_loom_worktree; then
		# Not in a worktree - allow stop, nothing to enforce
		debug_log "Not in loom worktree, allowing stop"
		exit 0
	fi

	# Find project root
	local project_root
	if ! project_root=$(find_project_root); then
		# Cannot find .work directory - allow stop, may be manual worktree
		debug_log "Project root not found (.work missing), allowing stop"
		exit 0
	fi

	# Log complete state for debugging
	debug_log "=== State Summary ==="
	debug_log "  Stage ID: $STAGE_ID"
	debug_log "  Project Root: $project_root"
	if [[ -L "$project_root/$WORK_DIR" ]]; then
		debug_log "  .work: symlink -> $(readlink -f "$project_root/$WORK_DIR" 2>/dev/null || echo 'unresolvable')"
		if [[ -e "$project_root/$WORK_DIR" ]]; then
			debug_log "  .work accessible: yes"
		else
			debug_log "  .work accessible: NO (broken symlink)"
		fi
	elif [[ -d "$project_root/$WORK_DIR" ]]; then
		debug_log "  .work: directory (not symlink)"
	else
		debug_log "  .work: does not exist"
	fi
	debug_log "===================="

	# Check if this is a knowledge stage - bypass commit requirement
	# Knowledge stages only update doc/loom/knowledge/ which is shared state
	if is_knowledge_stage "$project_root" "$STAGE_ID"; then
		debug_log "Knowledge stage detected - bypassing commit requirement"
		# Show reminder but don't block
		remind_knowledge_capture "$project_root"
		printf '\n' >&2
		printf '%s\n' "[loom-stop] Knowledge stage '$STAGE_ID' - commit not required" >&2
		printf '%s\n' "Tip: Consider capturing discoveries with 'loom knowledge update'" >&2
		exit 0
	fi

	# Show knowledge reminder (non-blocking, informational)
	remind_knowledge_capture "$project_root"

	# Collect issues
	local issues=()
	local has_uncommitted=0
	local stage_incomplete=0
	local git_is_clean=0

	# Check for uncommitted changes
	if check_git_clean; then
		git_is_clean=1
	else
		has_uncommitted=1
		issues+=("UNCOMMITTED CHANGES detected")
	fi

	# Check stage status
	local stage_file
	stage_file=$(find_stage_file "$project_root" "$STAGE_ID")

	if [[ -n "$stage_file" ]]; then
		local status
		status=$(get_stage_status "$stage_file")
		debug_log "Stage '$STAGE_ID' status: $status"

		# Status values and their blocking behavior
		# BLOCK: executing (work in progress, must complete)
		# BLOCK: merge-conflict (needs manual resolution)
		# ALLOW: all other valid statuses
		case "$status" in
		# BLOCKING STATUSES - work is not done or needs attention
		executing | Executing)
			stage_incomplete=1
			issues+=("Stage '$STAGE_ID' is still in EXECUTING status")
			debug_log "Stage is still executing - will block"
			;;
		merge-conflict | MergeConflict)
			stage_incomplete=1
			issues+=("Stage '$STAGE_ID' has MERGE CONFLICT that needs resolution")
			debug_log "Stage has merge conflict - will block"
			;;

		# ALLOWING STATUSES - stage is in a valid terminal or waiting state

		# Waiting states - haven't started or waiting for external input
		queued | Queued)
			debug_log "Stage is queued (not yet started) - allowing stop"
			;;
		waiting-for-deps | WaitingForDeps | pending)
			debug_log "Stage is waiting for dependencies - allowing stop"
			;;
		waiting-for-input | WaitingForInput)
			debug_log "Stage is waiting for user input - allowing stop"
			;;

		# Blocked/paused states - intentionally stopped
		blocked | Blocked)
			debug_log "Stage is blocked - allowing stop"
			;;
		needs-handoff | NeedsHandoff)
			debug_log "Stage needs handoff - allowing stop"
			;;
		merge-blocked | MergeBlocked)
			debug_log "Stage merge is blocked - allowing stop"
			;;

		# Completion states - work is done
		completed | Completed)
			debug_log "Stage is completed - allowing stop"
			;;
		verified | Verified)
			debug_log "Stage is verified - allowing stop"
			;;
		completed-with-failures | CompletedWithFailures)
			debug_log "Stage completed with failures - allowing stop"
			;;
		skipped | Skipped)
			debug_log "Stage is skipped - allowing stop"
			;;

		# Empty status - stage file exists but no status field
		"")
			debug_log "Stage has empty status - allowing stop (may be newly created)"
			;;

		# Unknown status - don't block, but log for debugging
		*)
			debug_log "Stage has unrecognized status '$status' - allowing stop (add to case statement if blocking needed)"
			;;
		esac
	else
		# Edge case: stage file not found
		# This can happen in non-loom worktrees that happen to have loom/ branch prefix
		# If git is also clean, this is likely a non-loom worktree - allow stop
		debug_log "No stage file found for '$STAGE_ID'"
		if [[ $git_is_clean -eq 1 ]]; then
			debug_log "Git is clean and no stage file - likely non-loom worktree, allowing stop"
			exit 0
		fi
		debug_log "Git has uncommitted changes - will require commit even without stage file"
	fi

	# If no issues, allow stop
	if [[ ${#issues[@]} -eq 0 ]]; then
		debug_log "No issues found, allowing stop"
		exit 0
	fi

	debug_log "Found ${#issues[@]} issue(s), will block stop"

	# Build error message with detailed context
	local message="LOOM WORKTREE EXIT BLOCKED for stage '$STAGE_ID':"
	message+="\n\nChecked: worktree=$(pwd), project_root=$project_root"

	if [[ $has_uncommitted -eq 1 ]]; then
		message+="\n\n1. You have uncommitted changes. Run:\n   git add <specific-files> && git commit -m 'feat: <description>'"
		message+="\n   (NOTE: Do NOT use 'git add -A' or 'git add .' as these will stage .work)"
		local changes
		changes=$(get_uncommitted_changes)
		if [[ -n "$changes" ]]; then
			message+="\n\n   Modified files:\n"
			while IFS= read -r line; do
				message+="\n   $line"
			done <<<"$changes"
		fi
	fi

	if [[ $stage_incomplete -eq 1 ]]; then
		local step_num=1
		if [[ $has_uncommitted -eq 1 ]]; then
			step_num=2
		fi

		# Get status for error message
		local blocking_status=""
		if [[ -n "$stage_file" ]]; then
			blocking_status=$(get_stage_status "$stage_file")
		fi

		if [[ "$blocking_status" == "merge-conflict" ]] || [[ "$blocking_status" == "MergeConflict" ]]; then
			message+="\n\n${step_num}. Stage has a MERGE CONFLICT. Resolve it manually:\n   - Check the conflicting files\n   - Resolve conflicts and commit\n   - Run: loom stage complete $STAGE_ID"
		else
			message+="\n\n${step_num}. Stage is still in '$blocking_status' status. After committing, run:\n   loom stage complete $STAGE_ID"
		fi

		if [[ -n "$stage_file" ]]; then
			message+="\n   (Stage file: $stage_file)"
		fi
	fi

	message+="\n\nDo NOT end this session until all steps are complete."

	# Add debug info section
	message+="\n\n--- Debug Info ---"
	message+="\nStage ID: $STAGE_ID"
	if [[ -n "$stage_file" ]]; then
		local parsed_status
		parsed_status=$(get_stage_status "$stage_file")
		message+="\nParsed status: '$parsed_status'"
		message+="\nStage file: $stage_file"
	else
		message+="\nStage file: NOT FOUND"
	fi
	message+="\nTo enable verbose logging, set: LOOM_HOOK_DEBUG=1"
	message+="\n------------------"

	block_with_reason "$message"
}

# Run main
main "$@"
