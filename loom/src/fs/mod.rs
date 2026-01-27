pub mod checkpoints;
pub mod knowledge;
pub mod memory;
pub mod permissions;
pub mod session_files;
pub mod stage_files;
pub mod stage_loading;
pub mod task_state;
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
