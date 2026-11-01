//! Handoff file generation for session context exhaustion.

mod content;
mod formatter;
mod lookup;
mod merge;
mod numbering;

#[cfg(test)]
mod merge_tests;
#[cfg(test)]
mod tests;

use anyhow::{bail, ensure, Context, Result};
use std::path::{Path, PathBuf};

use crate::fs::locking::{atomic_write_locked, locked_dir_update};
use crate::handoff::schema::{HandoffOrigin, HandoffV2};
use crate::models::session::Session;
use crate::models::stage::Stage;

pub use content::HandoffContent;
pub use lookup::{
    find_continuation_handoff, find_continuation_handoff_name, find_latest_session_handoff,
    find_matching_handoff, load_session_checkpoint, load_trusted_session_checkpoint,
};
pub use merge::{merge_session_handoff, MergeOutcome};
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
        let reusable = match origin {
            HandoffOrigin::RedBand => lookup::find_matching_handoff_with_context(
                &stage.id,
                &session.id,
                origin,
                content.context_tokens,
                work_dir,
            )?,
            // Enforcement records: one per verified outgoing pair, reusable
            // only while it is still the newest artifact for the stage. A
            // newer document means the session moved on after the enforcement
            // snapshot, so the snapshot no longer describes it.
            HandoffOrigin::BudgetExceeded
            | HandoffOrigin::Stalled
            | HandoffOrigin::AgentCeiling
            | HandoffOrigin::Retired => {
                let matching = find_matching_handoff(&stage.id, &session.id, origin, work_dir)?;
                matching.is_some() && matching == find_latest_handoff(&stage.id, work_dir)?
            }
            HandoffOrigin::CompletionEvidence => {
                bail!("completion evidence is written through merge_session_handoff")
            }
        };
        if reusable {
            return Ok(None);
        }
        generate_handoff_locked(stage, &content, work_dir).map(Some)
    })
}

pub(super) fn generate_handoff_locked(
    stage: &Stage,
    content: &HandoffContent,
    work_dir: &Path,
) -> Result<PathBuf> {
    write_handoff_locked(stage, work_dir, format_handoff_markdown(content)?)
}

pub(super) fn generate_handoff_document_locked(
    stage: &Stage,
    content: &HandoffContent,
    document: &HandoffV2,
    work_dir: &Path,
) -> Result<PathBuf> {
    write_handoff_locked(
        stage,
        work_dir,
        formatter::format_handoff_document(content, document)?,
    )
}

fn write_handoff_locked(stage: &Stage, work_dir: &Path, markdown: String) -> Result<PathBuf> {
    let handoffs_dir = work_dir.join("handoffs");
    // Allocation and the crash-atomic write share one directory lock. Two
    // daemon/CLI producers therefore cannot choose or overwrite the same
    // numbered artifact.
    let handoff_number = get_next_handoff_number(&stage.id, work_dir)?;
    let filename = format!("{}-handoff-{:03}.md", stage.id, handoff_number);
    let handoff_path = handoffs_dir.join(filename);
    atomic_write_locked(&handoff_path, &markdown)
        .with_context(|| format!("Failed to write handoff file: {}", handoff_path.display()))?;
    Ok(handoff_path)
}
