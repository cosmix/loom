//! Merge completed stage worktree back to main
//!
//! Usage: loom merge <stage_id> [--force]
//!
//! This module is organized into submodules:
//! - `execute`: Main merge execution logic
//! - `validation`: Pre-merge validation and safety checks
//! - `helpers`: Git utility functions for merge operations

mod execute;
mod helpers;
mod validation;

// Re-export the public API
pub use execute::{execute, worktree_path};

// Re-export validation functions that may be useful externally
#[allow(deprecated)]
pub use validation::{
    check_active_session, check_active_tmux_session, extract_frontmatter_field,
    find_tmux_session_for_stage, validate_stage_status,
};

// Re-export helpers that may be useful externally
pub use helpers::{
    auto_commit_changes, ensure_work_gitignored, get_uncommitted_files, has_merge_conflicts,
    has_uncommitted_changes, pop_stash, remove_loom_dirs_from_branch, stash_changes,
};
