//! Cleanup operations for worktree files

use anyhow::Result;
use std::path::Path;

use super::config::{StageFileCleanupConfig, StageFileCleanupResult};
use super::sessions::cleanup_sessions_for_stage;
use super::signals::cleanup_signals_for_sessions;
use super::stages::archive_stage_file;

/// Clean up all files associated with a stage after successful merge
///
/// This function removes or archives:
/// - Session files associated with the stage
/// - Signal files for those sessions
/// - Optionally the stage file itself
///
/// # Arguments
/// * `stage_id` - The stage ID to clean up files for
/// * `work_dir` - Path to the `.work/` directory
/// * `config` - Cleanup configuration options
///
/// # Returns
/// A `StageFileCleanupResult` describing what was cleaned up
pub fn cleanup_stage_files(
    stage_id: &str,
    work_dir: &Path,
    config: &StageFileCleanupConfig,
) -> Result<StageFileCleanupResult> {
    let mut result = StageFileCleanupResult::default();

    // Find and clean up sessions associated with this stage
    if config.cleanup_sessions {
        let sessions_result = cleanup_sessions_for_stage(stage_id, work_dir, config.verbose)?;
        result.sessions_removed = sessions_result.sessions_removed;
        result.cleaned_session_ids = sessions_result.session_ids;
        result.warnings.extend(sessions_result.warnings);

        // Clean up signals for those sessions
        if config.cleanup_signals {
            let signals_removed =
                cleanup_signals_for_sessions(&result.cleaned_session_ids, work_dir, config.verbose);
            result.signals_removed = signals_removed;
        }
    }

    // Handle stage file
    if config.archive_stage {
        if let Err(e) = archive_stage_file(stage_id, work_dir) {
            result
                .warnings
                .push(format!("Failed to archive stage file: {e}"));
        } else {
            result.stage_file_handled = true;
        }
    }

    Ok(result)
}
