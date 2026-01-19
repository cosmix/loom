//! Git branch management operations
//!
//! This module provides branch operations organized by concern:
//!
//! - `info`: BranchInfo type and parsing
//! - `operations`: Core CRUD operations (create, delete, list)
//! - `status`: Working tree status checking
//! - `ancestry`: Branch ancestry and merge detection
//! - `cleanup`: Branch cleanup after merges
//! - `naming`: Loom branch naming conventions

mod ancestry;
mod cleanup;
mod info;
mod naming;
mod operations;
mod status;

// Re-export all public items
pub use ancestry::{get_branch_head, is_ancestor_of, is_branch_merged};
pub use cleanup::cleanup_merged_branches;
pub use info::BranchInfo;
pub use naming::{branch_name_for_stage, stage_id_from_branch};
pub use operations::{
    branch_exists, create_branch, current_branch, default_branch, delete_branch, list_branches,
    list_loom_branches,
};
pub use status::{get_uncommitted_changes_summary, has_uncommitted_changes};
