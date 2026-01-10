//! Handoff file generation for session context exhaustion.

mod content;
mod formatter;
mod numbering;

#[cfg(test)]
mod tests;

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::session::Session;
use crate::models::stage::Stage;

pub use content::HandoffContent;
pub use numbering::find_latest_handoff;

use formatter::format_handoff_markdown;
use numbering::get_next_handoff_number;

/// Generate a handoff file for a session transitioning due to context exhaustion
///
/// # Arguments
/// * `session` - The session being handed off
/// * `stage` - The stage being worked on
/// * `content` - The handoff content
/// * `work_dir` - Path to the .work directory
///
/// # Returns
/// Path to the created handoff file
pub fn generate_handoff(
    _session: &Session,
    stage: &Stage,
    content: HandoffContent,
    work_dir: &Path,
) -> Result<PathBuf> {
    // Ensure handoffs directory exists
    let handoffs_dir = work_dir.join("handoffs");
    if !handoffs_dir.exists() {
        fs::create_dir_all(&handoffs_dir).with_context(|| {
            format!(
                "Failed to create handoffs directory: {}",
                handoffs_dir.display()
            )
        })?;
    }

    // Get next sequential number for this stage
    let handoff_number = get_next_handoff_number(&stage.id, work_dir)?;

    // Generate filename: {stage_id}-handoff-{NNN}.md
    let filename = format!("{}-handoff-{:03}.md", stage.id, handoff_number);
    let handoff_path = handoffs_dir.join(&filename);

    // Generate markdown content
    let markdown = format_handoff_markdown(&content)?;

    // Write the file
    fs::write(&handoff_path, markdown)
        .with_context(|| format!("Failed to write handoff file: {}", handoff_path.display()))?;

    Ok(handoff_path)
}
