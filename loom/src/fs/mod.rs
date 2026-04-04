pub mod knowledge;
pub mod locking;
pub mod memory;
pub mod permissions;
pub mod plan_lifecycle;
pub mod session_files;
pub mod stage_files;
pub mod stage_loading;
pub mod verifications;
pub mod work_dir;
pub mod work_integrity;
pub mod worktree_files;

use anyhow::Result;
use std::path::{Path, PathBuf};

// Re-export commonly used config functions
pub use work_dir::{load_config, load_config_required, Config};

// Re-export stage loading functions
pub use stage_loading::{extract_stage_frontmatter, load_stages_from_work_dir, StageFrontmatter};

// Re-export session file utilities
pub use session_files::{find_session_for_stage, find_sessions_for_stage};

// Re-export verification utilities
pub use verifications::{
    delete_verification, list_verifications, load_verification, store_verification, GapRecord,
    VerificationRecord,
};

// Re-export plan lifecycle utilities
pub use plan_lifecycle::{
    all_stages_merged, mark_plan_done_if_all_merged, mark_plan_in_progress, DONE_PREFIX,
    IN_PROGRESS_PREFIX,
};

/// Parse base_branch from config.toml in a work directory.
///
/// This is a convenience function for extracting the base_branch field
/// from the plan configuration.
///
/// # Arguments
/// * `work_dir` - Path to the .work directory
///
/// # Returns
/// * `Ok(Some(String))` - base_branch found in config
/// * `Ok(None)` - No config file or no base_branch field
/// * `Err(_)` - Failed to read or parse config
pub fn parse_base_branch_from_config(work_dir: &Path) -> Result<Option<String>> {
    match load_config(work_dir)? {
        Some(config) => Ok(config.base_branch()),
        None => Ok(None),
    }
}

/// Get the merge point (base_branch) from config.toml in a work directory.
///
/// This is a convenience function that returns the base_branch field,
/// falling back to "main" if not configured.
///
/// # Arguments
/// * `work_dir` - Path to the .work directory
///
/// # Returns
/// * `Ok(String)` - base_branch found in config, or "main" as default
/// * `Err(_)` - Failed to read or parse config
pub fn get_merge_point(work_dir: &Path) -> Result<String> {
    match load_config(work_dir)? {
        Some(config) => Ok(config.base_branch().unwrap_or_else(|| "main".to_string())),
        None => Ok("main".to_string()),
    }
}

/// Get the plan source path from config.toml in a work directory.
///
/// # Arguments
/// * `work_dir` - Path to the .work directory
///
/// # Returns
/// * `Ok(Some(PathBuf))` - source_path found in config
/// * `Ok(None)` - No config file or no source_path field
/// * `Err(_)` - Failed to read or parse config
pub fn get_source_path(work_dir: &Path) -> Result<Option<PathBuf>> {
    match load_config(work_dir)? {
        Some(config) => Ok(config.source_path()),
        None => Ok(None),
    }
}

/// Resolve the plan source path to an absolute path.
///
/// In worktrees, `.work/` is a symlink to `../../.work`. A relative `source_path`
/// (e.g., `doc/plans/PLAN-foo.md`) must be resolved from the **main** project root,
/// not the worktree root. This function canonicalizes the `.work/` directory to
/// follow the symlink and find the real project root.
///
/// Absolute paths are returned as-is for backward compatibility.
pub fn resolve_source_path(work_dir: &Path) -> Result<Option<PathBuf>> {
    let config = match load_config(work_dir)? {
        Some(c) => c,
        None => return Ok(None),
    };

    let source_path = match config.source_path() {
        Some(p) => p,
        None => return Ok(None),
    };

    if source_path.is_absolute() {
        return Ok(Some(source_path));
    }

    // Resolve relative paths from the main project root.
    // Canonicalize .work/ to follow the symlink in worktrees,
    // then take its parent as the project root.
    let real_work_dir = work_dir
        .canonicalize()
        .unwrap_or_else(|_| work_dir.to_path_buf());
    let project_root = real_work_dir.parent().unwrap_or(&real_work_dir);

    Ok(Some(project_root.join(&source_path)))
}
