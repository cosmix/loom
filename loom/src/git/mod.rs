//! Git operations for loom worktree management
//!
//! This module provides:
//! - Worktree creation/removal for parallel stage execution
//! - Branch management for stage isolation
//! - Merge operations for integrating completed work

pub mod branch;
pub mod merge;
pub mod worktree;

// Re-export commonly used types and functions
pub use worktree::{
    check_git_available, check_worktree_support, clean_worktrees, create_worktree,
    ensure_work_symlink, get_or_create_worktree, get_worktree_path, list_worktrees,
    remove_worktree, worktree_exists, WorktreeInfo,
};

pub use merge::{
    abort_merge, checkout_branch, conflict_resolution_instructions, merge_stage, MergeResult,
};

pub use branch::{
    branch_exists, branch_name_for_stage, cleanup_merged_branches, create_branch, current_branch,
    default_branch, delete_branch, list_branches, list_loom_branches, stage_id_from_branch,
    BranchInfo,
};

/// Initialize git module - check prerequisites
pub fn init() -> anyhow::Result<()> {
    check_git_available()?;
    check_worktree_support()?;
    Ok(())
}
