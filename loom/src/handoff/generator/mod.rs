//! Handoff file generation for session context exhaustion.

mod content;
mod formatter;
mod lookup;
mod numbering;

#[cfg(test)]
mod tests;

use anyhow::{ensure, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::fs::locking::{atomic_write_locked, locked_dir_update};
use crate::handoff::schema::{HandoffOrigin, ParsedHandoff};
use crate::models::session::Session;
use crate::models::stage::Stage;

pub use content::HandoffContent;
pub use lookup::{
    find_continuation_handoff, find_continuation_handoff_name, find_latest_session_handoff,
    find_matching_handoff,
};
pub use numbering::find_latest_handoff;

use formatter::format_handoff_markdown;
use numbering::get_next_handoff_number;

/// Generate a handoff file for a session transitioning due to context exhaustion
///
/// # Arguments
/// * `session` - The session being handed off
/// * `stage` - The stage being worked on
/// * `content` - The handoff content
/// * `work_dir` - Path to the .loom/work directory
///
/// # Returns
/// Path to the created handoff file
pub fn generate_handoff(
    _session: &Session,
    stage: &Stage,
    content: HandoffContent,
    work_dir: &Path,
) -> Result<PathBuf> {
    let handoffs_dir = work_dir.join("handoffs");
    locked_dir_update(&handoffs_dir, || {
        generate_handoff_locked(stage, &content, work_dir)
    })
}

/// Reuse or create one cause-specific handoff as a single locked operation.
///
/// The lookup and allocation must share the same directory lock. Locking only
/// the final write prevents filename collisions but still lets two concurrent
/// retries both observe "missing" and append duplicate budget artifacts.
pub fn ensure_handoff(
    session: &Session,
    stage: &Stage,
    content: HandoffContent,
    origin: HandoffOrigin,
    work_dir: &Path,
) -> Result<Option<PathBuf>> {
    ensure!(content.session_id == session.id, "handoff session mismatch");
    ensure!(content.stage_id == stage.id, "handoff stage mismatch");
    ensure!(content.origin == Some(origin), "handoff origin mismatch");

    let handoffs_dir = work_dir.join("handoffs");
    locked_dir_update(&handoffs_dir, || {
        let matching = find_matching_handoff(&stage.id, &session.id, origin, work_dir)?;
        let reusable = match origin {
            HandoffOrigin::RedBand => match matching.as_deref() {
                Some(path) => handoff_has_context(path, content.context_tokens)?,
                None => false,
            },
            // Enforcement records: one per verified outgoing pair, reusable
            // only while it is still the newest artifact for the stage. A
            // newer document means the session moved on after the enforcement
            // snapshot, so the snapshot no longer describes it.
            HandoffOrigin::BudgetExceeded
            | HandoffOrigin::Stalled
            | HandoffOrigin::AgentCeiling => {
                matching.is_some() && matching == find_latest_handoff(&stage.id, work_dir)?
            }
        };
        if reusable {
            return Ok(None);
        }
        generate_handoff_locked(stage, &content, work_dir).map(Some)
    })
}

fn handoff_has_context(path: &Path, context_tokens: u32) -> Result<bool> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read handoff file: {}", path.display()))?;
    Ok(ParsedHandoff::parse(&content)
        .as_v2()
        .is_some_and(|handoff| handoff.context_tokens == context_tokens))
}

fn generate_handoff_locked(
    stage: &Stage,
    content: &HandoffContent,
    work_dir: &Path,
) -> Result<PathBuf> {
    let handoffs_dir = work_dir.join("handoffs");
    // Allocation and the crash-atomic write share one directory lock. Two
    // daemon/CLI producers therefore cannot choose or overwrite the same
    // numbered artifact.
    let handoff_number = get_next_handoff_number(&stage.id, work_dir)?;
    let filename = format!("{}-handoff-{:03}.md", stage.id, handoff_number);
    let handoff_path = handoffs_dir.join(filename);
    let markdown = format_handoff_markdown(content)?;
    atomic_write_locked(&handoff_path, &markdown)
        .with_context(|| format!("Failed to write handoff file: {}", handoff_path.display()))?;
    Ok(handoff_path)
}
