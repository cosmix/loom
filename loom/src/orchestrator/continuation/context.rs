//! Context preparation for stage continuation.

use anyhow::{anyhow, bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::fs::stage_files::find_stage_file;
use crate::handoff::generator::find_latest_handoff;
use crate::handoff::schema::{HandoffV2, ParsedHandoff};
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;

use super::yaml_parse::parse_stage_from_markdown;

/// Context prepared for continuing a stage after handoff
#[derive(Debug)]
pub struct ContinuationContext {
    pub stage: Stage,
    pub handoff_path: Option<PathBuf>,
    pub worktree_path: PathBuf,
    pub branch: String,
}

/// Prepare context for continuing a stage
///
/// Loads the stage, finds the latest handoff if available, and verifies
/// the worktree exists. Returns all the context needed to continue work.
///
/// # Arguments
/// * `stage_id` - The ID of the stage to continue
/// * `work_dir` - The .work directory path
///
/// # Returns
/// ContinuationContext with stage, handoff path, worktree path, and branch
pub fn prepare_continuation(stage_id: &str, work_dir: &Path) -> Result<ContinuationContext> {
    let stage = load_stage(work_dir, stage_id)?;
    let handoff_path = find_latest_handoff(stage_id, work_dir)?;
    let (worktree_path, branch) = resolve_worktree_info(&stage, work_dir)?;

    Ok(ContinuationContext {
        stage,
        handoff_path,
        worktree_path,
        branch,
    })
}

/// Load handoff content from a markdown file
///
/// # Arguments
/// * `handoff_path` - Path to the handoff markdown file
///
/// # Returns
/// The full markdown content of the handoff file
pub fn load_handoff_content(handoff_path: &Path) -> Result<String> {
    if !handoff_path.exists() {
        bail!("Handoff file does not exist: {}", handoff_path.display());
    }

    fs::read_to_string(handoff_path)
        .with_context(|| format!("Failed to read handoff file: {}", handoff_path.display()))
}

/// Load and parse a handoff file, returning V2 structured data if available
///
/// # Arguments
/// * `handoff_path` - Path to the handoff markdown file
///
/// # Returns
/// ParsedHandoff which is either V2 structured data or V1 fallback content
pub fn load_and_parse_handoff(handoff_path: &Path) -> Result<ParsedHandoff> {
    let content = load_handoff_content(handoff_path)?;
    Ok(ParsedHandoff::parse(&content))
}

/// Load handoff as V2 structured data if possible, otherwise return None
///
/// This is useful when you specifically need V2 data and want to handle
/// V1 fallback separately.
///
/// # Arguments
/// * `handoff_path` - Path to the handoff markdown file
///
/// # Returns
/// Some(HandoffV2) if the file contains valid V2 data, None otherwise
pub fn load_handoff_v2(handoff_path: &Path) -> Result<Option<HandoffV2>> {
    let parsed = load_and_parse_handoff(handoff_path)?;
    Ok(parsed.as_v2().cloned())
}

/// Load a stage from .work/stages/
fn load_stage(work_dir: &Path, stage_id: &str) -> Result<Stage> {
    let stages_dir = work_dir.join("stages");

    let stage_path = find_stage_file(&stages_dir, stage_id)?.ok_or_else(|| {
        anyhow!("Stage file not found for: {stage_id}. Run 'loom stage create' first.")
    })?;

    let content = fs::read_to_string(&stage_path)
        .with_context(|| format!("Failed to read stage file: {}", stage_path.display()))?;

    parse_stage_from_markdown(&content)
}

/// Resolve worktree path and branch for a stage
fn resolve_worktree_info(stage: &Stage, work_dir: &Path) -> Result<(PathBuf, String)> {
    if let Some(worktree_id) = &stage.worktree {
        let path = load_worktree_path(work_dir, worktree_id)?;
        let branch = Worktree::branch_name(&stage.id);
        Ok((path, branch))
    } else {
        let project_root = work_dir.parent().ok_or_else(|| {
            anyhow!(
                "Cannot determine project root from work_dir: {}",
                work_dir.display()
            )
        })?;
        Ok((project_root.to_path_buf(), "main".to_string()))
    }
}

/// Load worktree path from worktree ID
fn load_worktree_path(work_dir: &Path, worktree_id: &str) -> Result<PathBuf> {
    let project_root = work_dir.parent().ok_or_else(|| {
        anyhow!(
            "Cannot determine project root from work_dir: {}",
            work_dir.display()
        )
    })?;

    let worktree_path = Worktree::worktree_path(project_root, worktree_id);

    if !worktree_path.exists() {
        bail!(
            "Worktree directory does not exist: {}. Create it with 'loom worktree create'.",
            worktree_path.display()
        );
    }

    Ok(worktree_path)
}
