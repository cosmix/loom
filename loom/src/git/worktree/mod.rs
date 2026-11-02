//! Git worktree management for parallel stage isolation
//!
//! Each parallel stage gets its own worktree to prevent file conflicts.
//! Worktrees are created in .worktrees/{stage_id}/ directories.
//!
//! ## Module structure
//!
//! - `base`: Base branch resolution for worktree creation
//! - `checks`: Validation checks (git availability, worktree support)
//! - `config_worktree`: Guards for a worktree's `config.worktree` file
//! - `discovery`: Worktree lookup and stage ID extraction
//! - `operations`: Core CRUD operations (create, remove, list, get_or_create)
//! - `parser`: Git worktree output parsing
//! - `paths`: Path resolution utilities for worktrees
//! - `pinned`: Git run in a stage worktree from outside its sandbox
//! - `scratch_admin`: The private git directory that keeps pinned git away from
//!   the whole administrative directory
//! - `settings`: Settings management (.claude/, CLAUDE.md, symlinks)

mod base;
mod checks;
mod config_worktree;
mod discovery;
mod main_identity;
mod operations;
mod parser;
mod paths;
mod pinned;
mod scratch_admin;
mod settings;

// Re-export all public items for backwards compatibility
pub use base::{resolve_base_branch, BaseBranchError, ResolvedBase};
pub use checks::{
    check_git_available, check_worktree_support, get_worktree_path, stage_worktree_path,
    worktree_exists,
};
pub use config_worktree::{partition_admin_dirs, worktree_admin_dirs, AdminDirs};
pub use discovery::{
    extract_stage_id_from_path, extract_worktree_stage_id, find_worktree_by_prefix,
};
pub(crate) use main_identity::{
    capture_main_identity, main_identity_exists, require_main_identity, require_main_identity_at,
};
pub use operations::{
    clean_worktrees, create_worktree, get_or_create_worktree, list_worktrees, remove_worktree,
};
pub use parser::WorktreeInfo;
pub use paths::{find_repo_root_from_cwd, find_worktree_root_from_cwd};
pub use pinned::WorktreeGit;
pub use settings::{
    add_settings_local_to_worktree_gitignore, ensure_work_symlink, is_worktree_scaffold_path,
};
