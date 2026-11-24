//! Worktrees `loom clean` would force-remove although they hold commits no `loom/*` branch
//! listing shows: a detached HEAD in `.worktrees/<id>` keeps its commits only in the worktree's
//! admin directory, which the removal deletes.

use anyhow::{Context, Result};
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use crate::git::branch::branch_name_for_stage;
use crate::git::cleanup::require_stage_contained;

/// A `.worktrees/<id>` whose removal would lose, or may lose, commits.
pub(super) struct HeldWorktree {
    pub stage_id: String,
    /// What the containment check found, or why it could not run.
    pub why: String,
}

/// The stage ids under `.worktrees/`: every directory, whatever it holds.
pub(super) fn worktree_stage_ids(worktrees_dir: &Path) -> Result<Vec<String>> {
    let entries = match fs::read_dir(worktrees_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("cannot read {}", worktrees_dir.display()))
        }
    };
    let mut ids = Vec::new();
    for entry in entries {
        let entry = entry.with_context(|| format!("cannot read {}", worktrees_dir.display()))?;
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            ids.push(entry.file_name().to_string_lossy().to_string());
        }
    }
    Ok(ids)
}

/// The worktrees whose HEAD (or any other ref the stage owns) holds commits `target_branch`
/// lacks, or that cannot be checked; fail closed, as the branch listing does. A stage whose
/// branch is in `unmerged` is left out: the branch listing already names it.
pub(super) fn held_worktrees(
    repo_root: &Path,
    target_branch: &str,
    unmerged: &[(String, usize)],
) -> Result<Vec<HeldWorktree>> {
    let ids = worktree_stage_ids(&repo_root.join(".worktrees"))?;
    let held = ids
        .into_iter()
        .filter(|id| {
            let branch = branch_name_for_stage(id);
            !unmerged.iter().any(|(listed, _)| *listed == branch)
        })
        .filter_map(|stage_id| {
            let checked = require_stage_contained(&stage_id, None, target_branch, repo_root);
            checked.err().map(|error| HeldWorktree {
                stage_id,
                why: format!("{error:#}"),
            })
        })
        .collect();
    Ok(held)
}

#[cfg(test)]
mod tests;
