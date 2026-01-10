//! Configuration and result types for cleanup operations

/// Configuration for cleanup operations
#[derive(Debug, Clone)]
pub struct CleanupConfig {
    /// Force removal even if worktree has uncommitted changes
    pub force_worktree_removal: bool,
    /// Force branch deletion even if not fully merged
    pub force_branch_deletion: bool,
    /// Run git worktree prune after cleanup
    pub prune_worktrees: bool,
    /// Print progress messages
    pub verbose: bool,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            force_worktree_removal: true,
            force_branch_deletion: false,
            prune_worktrees: true,
            verbose: true,
        }
    }
}

impl CleanupConfig {
    /// Create a quiet config (no verbose output)
    pub fn quiet() -> Self {
        Self {
            verbose: false,
            ..Self::default()
        }
    }

    /// Create a config for forced cleanup (for use by loom clean command)
    pub fn forced() -> Self {
        Self {
            force_worktree_removal: true,
            force_branch_deletion: true,
            prune_worktrees: true,
            verbose: true,
        }
    }
}

/// Result of a cleanup operation
#[derive(Debug, Clone, Default)]
pub struct CleanupResult {
    /// Whether the worktree was successfully removed
    pub worktree_removed: bool,
    /// Whether the branch was successfully deleted
    pub branch_deleted: bool,
    /// Whether the base branch was successfully deleted (if it existed)
    pub base_branch_deleted: bool,
    /// Errors that occurred (non-fatal)
    pub warnings: Vec<String>,
}

impl CleanupResult {
    /// Check if cleanup was fully successful (no warnings)
    pub fn is_complete(&self) -> bool {
        self.worktree_removed && self.branch_deleted && self.warnings.is_empty()
    }

    /// Check if cleanup made any progress
    pub fn any_cleanup_done(&self) -> bool {
        self.worktree_removed || self.branch_deleted || self.base_branch_deleted
    }
}
