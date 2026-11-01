//! Working-directory resolution shared by every memory command handler.

use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::env;

use crate::commands::common::work_dir_path;
use crate::fs::git_marker::is_real_git_dir;
use crate::fs::memory::init_memory_dir;
use crate::git::worktree::find_repo_root_from_cwd;

/// Sentinel stage ID used by the four recording commands (`note`, `decision`,
/// `change`, `question`) when neither `--stage` nor `LOOM_STAGE_ID` supplies
/// one. Lets ad-hoc/interactive sessions (no active loom plan) still record
/// insights instead of erroring outright.
pub(super) const AD_HOC_STAGE_ID: &str = "ad-hoc";

/// Resolve the state directory, creating one at the repository root if needed.
/// Read-only commands use `readonly_work_dir` instead.
pub(super) fn get_or_create_work_dir() -> Result<std::path::PathBuf> {
    if let Ok(work_dir) = work_dir_path() {
        return Ok(work_dir);
    }

    let cwd = env::current_dir().context("Failed to get current directory")?;
    // The resolver can fall back to cwd; require a real git directory.
    let repo_root =
        find_repo_root_from_cwd(&cwd).filter(|root| is_real_git_dir(&root.join(".git")));
    let Some(repo_root) = repo_root else {
        bail!("No loom workspace found. Run 'loom init' first.");
    };

    // Always the nested layout: loom never creates a `.work/`.
    let work_dir = repo_root.join(".loom").join("work");
    init_memory_dir(&work_dir)?;

    eprintln!(
        "{} No loom workspace found; recording to {} (stage '{}')",
        "ℹ".blue(),
        work_dir.display(),
        AD_HOC_STAGE_ID
    );

    Ok(work_dir)
}

/// Get the state directory for READ-ONLY commands (`query`, `list`, `show`).
///
/// Returns `None` instead of erroring when no workspace exists, so these
/// commands degrade gracefully rather than failing. This matters because
/// `loom memory list` is the first step of the post-compaction recovery flow
/// (the SessionStart hook's re-anchor message names it) - a hard failure there
/// would derail recovery before it starts. Unlike `get_or_create_work_dir`,
/// this never creates anything.
pub(super) fn readonly_work_dir() -> Option<std::path::PathBuf> {
    work_dir_path().ok()
}

/// Validate stage ID to prevent path traversal attacks
pub(super) fn validate_stage_id(id: &str) -> Result<()> {
    if !crate::validation::valid_stage_path_component(id) {
        bail!("Invalid stage ID: must be a safe path component");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_stage_id;

    #[test]
    fn stage_ids_require_safe_path_components() {
        for (id, valid) in [
            ("stage-1.a_b", true),
            ("", false),
            (".", false),
            ("..", false),
            ("a..b", false),
            ("bad/stage", false),
            ("bad\\stage", false),
            ("with space", false),
            ("é", false),
        ] {
            assert_eq!(validate_stage_id(id).is_ok(), valid, "{id:?}");
        }
    }
}
