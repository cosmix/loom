//! Cleanup utilities for successful merge operations
//!
//! This module provides reusable functions for cleaning up resources after
//! a successful merge. It consolidates cleanup logic that was previously
//! duplicated across multiple commands (verify, stage complete, clean).
//!
//! ## Cleanup Phases
//!
//! A successful merge cleanup involves several phases:
//! 1. Worktree removal - Remove the isolated git worktree
//! 2. Branch deletion - Delete the loom/{stage-id} branch
//! 3. Git pruning - Clean up any stale worktree references
//!
//! ## Usage
//!
//! ```rust,ignore
//! use loom::git::cleanup::{CleanupConfig, cleanup_after_merge};
//!
//! let config = CleanupConfig::default();
//! cleanup_after_merge("stage-1", repo_root, &config)?;
//! ```

mod batch;
mod branch;
mod config;
pub(crate) mod worktree;

#[cfg(test)]
mod tests;

// Re-export public API
pub use batch::{cleanup_after_merge, cleanup_multiple_stages, needs_cleanup, prune_worktrees};
pub use branch::cleanup_branch;
pub use config::{CleanupConfig, CleanupResult};
pub use worktree::cleanup_worktree;
