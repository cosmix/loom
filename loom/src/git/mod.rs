//! Git operations for loom worktree management
//!
//! This module provides:
//! - Worktree creation/removal for parallel stage execution
//! - Branch management for stage isolation
//! - Merge operations for integrating completed work
//! - Cleanup utilities for successful merges

pub mod branch;
pub mod cleanup;
pub mod merge;
pub mod worktree;

// Re-export commonly used types and functions
pub use worktree::{
    check_git_available, check_worktree_support, clean_worktrees, create_worktree,
    ensure_work_symlink, get_or_create_worktree, get_worktree_path, list_worktrees,
    remove_worktree, resolve_base_branch, worktree_exists, ResolvedBase, WorktreeInfo,
};

pub use merge::{
    abort_merge, checkout_branch, conflict_resolution_instructions, get_conflicting_files,
    merge_stage, MergeResult,
};

pub use branch::{
    branch_exists, branch_name_for_stage, cleanup_merged_branches, create_branch, current_branch,
    default_branch, delete_branch, get_branch_head, get_uncommitted_changes_summary,
    has_uncommitted_changes, is_branch_merged, list_branches, list_loom_branches,
    stage_id_from_branch, BranchInfo,
};

pub use cleanup::{
    base_branch_exists, cleanup_after_merge, cleanup_all_base_branches, cleanup_base_branch,
    cleanup_branch, cleanup_multiple_stages, cleanup_worktree, needs_cleanup, prune_worktrees,
    CleanupConfig, CleanupResult,
};

/// Initialize git module - check prerequisites
pub fn init() -> anyhow::Result<()> {
    check_git_available()?;
    check_worktree_support()?;
    Ok(())
}
