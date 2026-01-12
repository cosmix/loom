//! Merge recovery command for failed/interrupted merge sessions
//!
//! Usage: loom merge <stage_id> [--force]
//!
//! ## Purpose
//!
//! The `loom merge` command is primarily a **recovery command** for handling merge
//! failures and interruptions. When a stage reaches Verified status, loom automatically
//! attempts to merge it. If that auto-merge encounters conflicts, a Claude Code session
//! is spawned to resolve them.
//!
//! Use `loom merge` when:
//! - The auto-merge conflict resolution session was interrupted or terminated
//! - A previous merge attempt failed and needs to be retried
//! - You want to manually trigger a merge for a completed stage
//!
//! ## Workflow
//!
//! 1. Stage reaches Verified status
//! 2. Orchestrator auto-attempts merge
//! 3. If conflicts detected: CC session spawns in main repo to resolve
//! 4. If CC session terminates before completion: stage stays in "Conflict" state
//! 5. User runs `loom merge <stage>` to restart the conflict resolution session
//!
//! ## Module Organization
//!
//! - `execute`: Main merge/recovery execution logic
//! - `validation`: Pre-merge validation and safety checks
//! - `helpers`: Git utility functions for merge operations

mod execute;
mod helpers;
mod validation;

// Re-export the public API
pub use execute::{execute, mark_stage_merged, worktree_path};

// Re-export validation functions that may be useful externally
pub use validation::{
    check_active_session, extract_frontmatter_field, find_session_for_stage, validate_stage_status,
};

// Re-export helpers that may be useful externally
pub use helpers::{
    auto_commit_changes, ensure_work_gitignored, get_uncommitted_files, has_merge_conflicts,
    has_uncommitted_changes, pop_stash, remove_loom_dirs_from_branch, stash_changes,
};
