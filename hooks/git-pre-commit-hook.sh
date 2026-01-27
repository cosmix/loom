#!/usr/bin/env bash
# Git pre-commit hook: Block commits containing .work or .worktrees
#
# This hook is installed by loom to prevent accidental commits of orchestration
# state files. The .work directory in worktrees is a symlink to shared state -
# committing it would corrupt the main repository.
#
# Installation: Appended to .git/hooks/pre-commit by `loom init`
#
# Exit codes:
#   0 - Allow commit
#   1 - Block commit (staged files contain .work or .worktrees)

set -euo pipefail

# Marker for loom hook section (for idempotent installation)
# LOOM_PRE_COMMIT_HOOK_START

# Check if any staged files are in .work/ or .worktrees/ directories
check_forbidden_paths() {
    local forbidden_patterns=(
        "^\.work/"
        "^\.work$"
        "^\.worktrees/"
        "^\.worktrees$"
    )

    # Get list of staged files
    local staged_files
    staged_files=$(git diff --cached --name-only 2>/dev/null || true)

    if [[ -z "$staged_files" ]]; then
        return 0  # No staged files
    fi

    local found_forbidden=0
    local forbidden_files=()

    while IFS= read -r file; do
        for pattern in "${forbidden_patterns[@]}"; do
            if [[ "$file" =~ $pattern ]]; then
                forbidden_files+=("$file")
                found_forbidden=1
                break
            fi
        done
    done <<< "$staged_files"

    if [[ $found_forbidden -eq 1 ]]; then
        echo ""
        echo "============================================================"
        echo "  LOOM: BLOCKED COMMIT - Forbidden files staged"
        echo "============================================================"
        echo ""
        echo "The following files MUST NOT be committed:"
        echo ""
        for file in "${forbidden_files[@]}"; do
            echo "  - $file"
        done
        echo ""
        echo "WHY: .work/ contains orchestration state shared across worktrees."
        echo "     Committing it corrupts the main repository."
        echo ""
        echo "FIX: Unstage these files:"
        echo ""
        for file in "${forbidden_files[@]}"; do
            echo "  git reset HEAD -- $file"
        done
        echo ""
        echo "PREVENTION: Always use 'git add <specific-files>' instead of"
        echo "            'git add -A' or 'git add .'"
        echo ""
        echo "============================================================"
        return 1
    fi

    return 0
}

# Run the check
check_forbidden_paths

# LOOM_PRE_COMMIT_HOOK_END
