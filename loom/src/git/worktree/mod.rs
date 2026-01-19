//! Git worktree management for parallel stage isolation
//!
//! Each parallel stage gets its own worktree to prevent file conflicts.
//! Worktrees are created in .worktrees/{stage_id}/ directories.
//!
//! ## Module structure
//!
//! - `base`: Base branch resolution for worktree creation
//! - `checks`: Validation checks (git availability, worktree support)
//! - `discovery`: Worktree lookup and stage ID extraction
//! - `operations`: Core CRUD operations (create, remove, list, get_or_create)
//! - `parser`: Git worktree output parsing
//! - `paths`: Path resolution utilities for worktrees
//! - `settings`: Settings management (.claude/, CLAUDE.md, symlinks)

mod base;
mod checks;
mod discovery;
mod operations;
mod parser;
mod paths;
mod settings;

// Re-export all public items for backwards compatibility
pub use base::{resolve_base_branch, ResolvedBase};
pub use checks::{check_git_available, check_worktree_support, get_worktree_path, worktree_exists};
pub use discovery::{extract_stage_id_from_path, extract_worktree_stage_id, find_worktree_by_prefix};
pub use operations::{
    clean_worktrees, create_worktree, get_or_create_worktree, list_worktrees, remove_worktree,
};
pub use parser::WorktreeInfo;
pub use paths::{find_repo_root_from_cwd, find_worktree_root_from_cwd};
pub use settings::{ensure_work_symlink, setup_worktree_hooks};
