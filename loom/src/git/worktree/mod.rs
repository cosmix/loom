//! Git worktree management for parallel stage isolation
//!
//! Each parallel stage gets its own worktree to prevent file conflicts.
//! Worktrees are created in .worktrees/{stage_id}/ directories.
//!
//! ## Module structure
//!
//! - `base`: Base branch resolution for worktree creation
//! - `operations`: Core CRUD operations (create, remove, list, get_or_create)
//! - `settings`: Settings management (.claude/, CLAUDE.md, symlinks)
//! - `parser`: Git worktree output parsing
//! - `checks`: Validation checks (git availability, worktree support)

mod base;
mod checks;
mod operations;
mod parser;
mod settings;

// Re-export all public items for backwards compatibility
pub use base::{cleanup_all_temp_branches, cleanup_temp_branch, resolve_base_branch, ResolvedBase};
pub use checks::{check_git_available, check_worktree_support, get_worktree_path, worktree_exists};
pub use operations::{
    clean_worktrees, create_worktree, extract_worktree_stage_id, find_worktree_by_prefix,
    get_or_create_worktree, list_worktrees, remove_worktree,
};
pub use parser::WorktreeInfo;
pub use settings::{ensure_work_symlink, setup_worktree_hooks};
