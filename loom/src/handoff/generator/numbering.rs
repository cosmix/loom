//! Handoff file numbering and lookup utilities.

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Get the next sequential handoff number for a stage
///
/// Scans existing handoff files in .work/handoffs/ and returns the next available number.
pub fn get_next_handoff_number(stage_id: &str, work_dir: &Path) -> Result<u32> {
    let handoffs_dir = work_dir.join("handoffs");

    // If directory doesn't exist, this is the first handoff
    if !handoffs_dir.exists() {
        return Ok(1);
    }

    // Read directory entries
    let entries = fs::read_dir(&handoffs_dir).with_context(|| {
        format!(
            "Failed to read handoffs directory: {}",
            handoffs_dir.display()
        )
    })?;

    let mut max_number = 0u32;
    let prefix = format!("{stage_id}-handoff-");

    for entry in entries {
        let entry = entry.with_context(|| "Failed to read directory entry")?;
        let filename = entry.file_name();
        let filename_str = filename.to_string_lossy();

        // Check if this is a handoff file for our stage
        if let Some(rest) = filename_str.strip_prefix(&prefix) {
            // Extract number from "NNN.md"
            if let Some(num_str) = rest.strip_suffix(".md") {
                if let Ok(num) = num_str.parse::<u32>() {
                    if num > max_number {
                        max_number = num;
                    }
                }
            }
        }
    }

    Ok(max_number + 1)
}

/// Find the most recent handoff file for a stage
///
/// Returns the path to the latest handoff file, or None if no handoffs exist.
pub fn find_latest_handoff(stage_id: &str, work_dir: &Path) -> Result<Option<PathBuf>> {
    let handoffs_dir = work_dir.join("handoffs");

    // If directory doesn't exist, no handoffs exist
    if !handoffs_dir.exists() {
        return Ok(None);
    }

    // Read directory entries
    let entries = fs::read_dir(&handoffs_dir).with_context(|| {
        format!(
            "Failed to read handoffs directory: {}",
            handoffs_dir.display()
        )
    })?;

    let mut max_number = 0u32;
    let mut latest_path: Option<PathBuf> = None;
    let prefix = format!("{stage_id}-handoff-");

    for entry in entries {
        let entry = entry.with_context(|| "Failed to read directory entry")?;
        let path = entry.path();
        let filename = entry.file_name();
        let filename_str = filename.to_string_lossy();

        // Check if this is a handoff file for our stage
        if let Some(rest) = filename_str.strip_prefix(&prefix) {
            // Extract number from "NNN.md"
            if let Some(num_str) = rest.strip_suffix(".md") {
                if let Ok(num) = num_str.parse::<u32>() {
                    if num > max_number {
                        max_number = num;
                        latest_path = Some(path);
                    }
                }
            }
        }
    }

    Ok(latest_path)
}
