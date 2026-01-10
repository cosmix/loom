//! Stage file operations for worktrees

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use super::sessions::find_sessions_for_stage;

/// Archive a stage file by moving it to the archive directory
pub(crate) fn archive_stage_file(stage_id: &str, work_dir: &Path) -> Result<()> {
    let stages_dir = work_dir.join("stages");
    let archive_dir = work_dir.join("archive");

    // Find the stage file
    let stage_file = find_stage_file_by_id(&stages_dir, stage_id)?;
    let Some(stage_file) = stage_file else {
        return Ok(()); // No file to archive
    };

    // Ensure archive directory exists
    fs::create_dir_all(&archive_dir).with_context(|| "Failed to create archive directory")?;

    // Move to archive
    let archive_path = archive_dir.join(stage_file.file_name().unwrap_or_default());
    fs::rename(&stage_file, &archive_path)
        .with_context(|| format!("Failed to archive stage file to {}", archive_path.display()))?;

    Ok(())
}

/// Find a stage file by stage ID (handles depth prefix)
pub fn find_stage_file_by_id(
    stages_dir: &Path,
    stage_id: &str,
) -> Result<Option<std::path::PathBuf>> {
    if !stages_dir.exists() {
        return Ok(None);
    }

    let entries = fs::read_dir(stages_dir)
        .with_context(|| format!("Failed to read stages directory: {}", stages_dir.display()))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            // Check for exact match (no prefix)
            if stem == stage_id {
                return Ok(Some(path));
            }

            // Check for prefixed match: XX-{stage_id}
            if let Some(suffix) = stem.strip_prefix(|c: char| c.is_ascii_digit()) {
                if let Some(suffix) = suffix.strip_prefix(|c: char| c.is_ascii_digit()) {
                    if let Some(id) = suffix.strip_prefix('-') {
                        if id == stage_id {
                            return Ok(Some(path));
                        }
                    }
                }
            }
        }
    }

    Ok(None)
}

/// Check if any files exist for a stage that would need cleanup
pub fn stage_has_files(stage_id: &str, work_dir: &Path) -> bool {
    // Check for sessions
    if let Ok(sessions) = find_sessions_for_stage(stage_id, work_dir) {
        if !sessions.is_empty() {
            return true;
        }
    }

    // Check for stage file
    let stages_dir = work_dir.join("stages");
    if let Ok(Some(_)) = find_stage_file_by_id(&stages_dir, stage_id) {
        return true;
    }

    false
}
