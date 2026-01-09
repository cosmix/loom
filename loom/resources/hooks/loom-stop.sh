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

# Configuration
readonly WORKTREE_MARKER=".worktrees/"
readonly LOOM_BRANCH_PREFIX="loom/"
readonly WORK_DIR=".work"
readonly STAGES_DIR="$WORK_DIR/stages"

# Detect if running in a loom worktree
# Returns: 0 if in worktree, 1 otherwise
# Sets: STAGE_ID variable if in worktree
detect_loom_worktree() {
    local cwd
    cwd=$(pwd)

    # Method 1: Check if path contains .worktrees/
    if [[ "$cwd" == *"$WORKTREE_MARKER"* ]]; then
        # Extract stage ID from path: /path/to/.worktrees/<stage-id>/...
        local worktree_part="${cwd#*$WORKTREE_MARKER}"
        STAGE_ID="${worktree_part%%/*}"
        return 0
    fi

    # Method 2: Check git branch name
    local branch
    if branch=$(git rev-parse --abbrev-ref HEAD 2>/dev/null); then
        if [[ "$branch" == "$LOOM_BRANCH_PREFIX"* ]]; then
            STAGE_ID="${branch#$LOOM_BRANCH_PREFIX}"
            return 0
        fi
    fi

    return 1
}

# Find the project root (where .work directory is)
# Searches upward from current directory
find_project_root() {
    local dir
    dir=$(pwd)

    while [[ "$dir" != "/" ]]; do
        if [[ -d "$dir/$WORK_DIR" ]]; then
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
            echo "$root"
            return 0
        fi
    fi

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
        return 0  # Clean
    else
        return 1  # Dirty
    fi
}

# Get list of uncommitted changes for error message
get_uncommitted_changes() {
    git status --porcelain 2>/dev/null | head -10
}

# Parse stage status from stage file YAML frontmatter
# Args: $1 = path to stage file
# Returns: status string or empty if not found
get_stage_status() {
    local stage_file="$1"

    if [[ ! -f "$stage_file" ]]; then
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
                break  # End of frontmatter
            fi
            continue
        fi

        if [[ $in_frontmatter -eq 1 ]]; then
            # Match status: <value>
            if [[ "$line" =~ ^status:\ *(.+)$ ]]; then
                status="${BASH_REMATCH[1]}"
                # Trim whitespace
                status="${status#"${status%%[![:space:]]*}"}"
                status="${status%"${status##*[![:space:]]}"}"
                break
            fi
        fi
    done < "$stage_file"

    echo "$status"
}

# Find the stage file for a given stage ID
# Args: $1 = project root, $2 = stage ID
find_stage_file() {
    local project_root="$1"
    local stage_id="$2"
    local stages_path="$project_root/$STAGES_DIR"

    if [[ ! -d "$stages_path" ]]; then
        echo ""
        return
    fi

    # Look for files matching the stage ID
    # Stage files can be named: <prefix>-<stage-id>.md or just <stage-id>.md
    for file in "$stages_path"/*"$stage_id"*.md "$stages_path"/"$stage_id".md; do
        if [[ -f "$file" ]]; then
            echo "$file"
            return
        fi
    done

    # Also try exact match with number prefix (e.g., 01-stage-id.md)
    for file in "$stages_path"/*-"$stage_id".md; do
        if [[ -f "$file" ]]; then
            echo "$file"
            return
        fi
    done

    echo ""
}

# Output blocking JSON and exit
# Args: $1 = reason string
block_with_reason() {
    local reason="$1"
    # Escape special characters in reason for JSON
    reason="${reason//\\/\\\\}"  # Escape backslashes
    reason="${reason//\"/\\\"}"  # Escape quotes
    reason="${reason//$'\n'/\\n}"  # Escape newlines
    reason="${reason//$'\r'/}"  # Remove carriage returns

    printf '{"continue": false, "reason": "%s"}\n' "$reason"
    exit 2
}

# Main hook logic
main() {
    # Check if we're in a loom worktree
    local STAGE_ID=""
    if ! detect_loom_worktree; then
        # Not in a worktree - allow stop, nothing to enforce
        exit 0
    fi

    # Find project root
    local project_root
    if ! project_root=$(find_project_root); then
        # Cannot find .work directory - allow stop, may be manual worktree
        exit 0
    fi

    # Collect issues
    local issues=()
    local has_uncommitted=0
    local stage_incomplete=0

    # Check for uncommitted changes
    if ! check_git_clean; then
        has_uncommitted=1
        issues+=("UNCOMMITTED CHANGES detected")
    fi

    # Check stage status
    local stage_file
    stage_file=$(find_stage_file "$project_root" "$STAGE_ID")

    if [[ -n "$stage_file" ]]; then
        local status
        status=$(get_stage_status "$stage_file")

        # Status values that mean work is not done
        case "$status" in
            executing|Executing)
                stage_incomplete=1
                issues+=("Stage '$STAGE_ID' is still in EXECUTING status")
                ;;
            waiting-for-input|WaitingForInput)
                # This is acceptable - waiting for user
                ;;
            blocked|Blocked)
                # This is acceptable - explicitly blocked
                ;;
            needs-handoff|NeedsHandoff)
                # This is acceptable - handoff in progress
                ;;
            completed|Completed|verified|Verified)
                # All good
                ;;
            *)
                # Unknown status, don't block
                ;;
        esac
    fi

    # If no issues, allow stop
    if [[ ${#issues[@]} -eq 0 ]]; then
        exit 0
    fi

    # Build error message
    local message="LOOM WORKTREE EXIT BLOCKED for stage '$STAGE_ID':"

    if [[ $has_uncommitted -eq 1 ]]; then
        message+="\n\n1. You have uncommitted changes. Run:\n   git add -A && git commit -m 'feat: <description>'"
        local changes
        changes=$(get_uncommitted_changes)
        if [[ -n "$changes" ]]; then
            message+="\n\n   Modified files:\n"
            while IFS= read -r line; do
                message+="\n   $line"
            done <<< "$changes"
        fi
    fi

    if [[ $stage_incomplete -eq 1 ]]; then
        local step_num=1
        if [[ $has_uncommitted -eq 1 ]]; then
            step_num=2
        fi
        message+="\n\n${step_num}. Stage is still EXECUTING. After committing, run:\n   loom stage complete $STAGE_ID"
    fi

    message+="\n\nDo NOT end this session until both steps are complete."

    block_with_reason "$message"
}

# Run main
main "$@"
