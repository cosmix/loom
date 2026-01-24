pub mod checkpoints;
pub mod knowledge;
pub mod memory;
pub mod permissions;
pub mod session_files;
pub mod stage_files;
pub mod stage_loading;
pub mod task_state;
pub mod work_dir;
pub mod worktree_files;

use anyhow::Result;
use std::path::Path;

// Re-export commonly used config functions
pub use work_dir::{load_config, load_config_required, Config};

// Re-export stage loading functions
pub use stage_loading::{extract_stage_frontmatter, load_stages_from_work_dir, StageFrontmatter};

// Re-export session file utilities
pub use session_files::{find_session_for_stage, find_sessions_for_stage};

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
