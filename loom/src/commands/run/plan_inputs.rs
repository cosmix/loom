//! Require execution inputs in the base branch before creating stage worktrees.
//!
//! Input discovery and the per-file commit check live in `crate::plan::inputs`,
//! shared with `loom plan verify`.

#[cfg(test)]
mod tests;

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::fs::{plan_lifecycle, work_dir::WorkDir};
use crate::git::worktree::WorktreeGit;
use crate::plan::inputs::{collect_inputs, uncommitted_reason_pinned};

/// Unrelated scratch files are allowed, but every plan input must be committed.
pub(super) fn require_committed_plan(work_dir: &WorkDir) -> Result<()> {
    let root = work_dir
        .project_root()
        .context("Cannot determine project root")?;
    crate::git::worktree::require_main_identity(root)?;
    let git = WorktreeGit::pinned_main(root)?;
    let configured_base = crate::fs::parse_base_branch_from_config(work_dir.root())?;
    let base = crate::git::branch::resolve_target_branch_with(&configured_base, &git)?;
    let branch = crate::git::branch::current_branch_with(&git)?;
    if branch != base {
        bail!(
            "Cannot start loom run from '{branch}': the plan's base branch is '{base}'. \
             Switch to '{base}', or re-initialize the plan on the intended branch."
        );
    }
    let plan = crate::fs::resolve_source_path(work_dir.root())?
        .context("No active plan; run loom init <plan-path> first")?;
    let inputs = collect_inputs(root, &plan)?;
    let mut problems: Vec<String> = inputs
        .invalid
        .iter()
        .map(|reference| format!("  {reference}"))
        .collect();
    for path in inputs.files.keys() {
        if let Some(reason) = uncommitted_reason_pinned(&git, path)? {
            problems.push(format!("  {}: {reason}", path.display()));
        }
    }
    if !problems.is_empty() {
        bail!(
            "Cannot start loom run: the active plan and its briefs must be committed on \
             '{base}'.\n{}\nStage worktrees only receive committed files. Restore missing \
             inputs, fix invalid references and commit the listed paths, then run loom run again. \
             Copying inputs into a stage worktree can block its merge or cleanup.",
            problems.join("\n")
        );
    }
    Ok(())
}

/// Commit Loom's own rename so the active filename exists in every new worktree.
pub(super) fn mark_plan_in_progress(work_dir: &WorkDir) -> Result<()> {
    let old = plan_lifecycle::get_plan_source_path(work_dir)?
        .context("No active plan; run loom init <plan-path> first")?;
    let root = work_dir
        .project_root()
        .context("Cannot determine project root")?;
    crate::git::worktree::require_main_identity(root)?;
    if !plan_lifecycle::has_prefix(&old, plan_lifecycle::IN_PROGRESS_PREFIX)
        && !plan_lifecycle::has_prefix(&old, plan_lifecycle::DONE_PREFIX)
    {
        let new = plan_lifecycle::add_prefix_to_filename(&old, plan_lifecycle::IN_PROGRESS_PREFIX);
        if root.join(&new).symlink_metadata().is_ok() {
            bail!("Cannot rename plan: {} already exists", new.display());
        }
    }
    let Some(new) = plan_lifecycle::mark_plan_in_progress(work_dir)? else {
        return Ok(());
    };
    let old = old.to_str().context("Plan path is not UTF-8")?;
    let new = new.to_str().context("Plan path is not UTF-8")?;
    commit_rename(work_dir, root, old, new).with_context(|| {
        format!(
            "Could not commit Loom's plan rename. Commit the rename from '{old}' to \
             '{new}', then run loom run again; no stage has started"
        )
    })
}

/// Commit `paths` alone. A pinned commit moves the branch before it publishes the
/// index, so a failure with the branch moved lost only that publication, to a
/// concurrent git: the commit stands and still needs its attestation.
fn commit_paths(git: &WorktreeGit, paths: &[String; 2]) -> Result<()> {
    let before = git.run_checked(&["rev-parse", "HEAD"])?;
    let committed = git.run_checked(&[
        "commit",
        "--only",
        "-m",
        "chore(loom): mark plan in progress",
        "--",
        &paths[0],
        &paths[1],
    ]);
    let Err(error) = committed else {
        return Ok(());
    };
    if git.run_checked(&["rev-parse", "HEAD"])? == before {
        return Err(error);
    }
    tracing::warn!("the plan rename committed, but its index update was lost: {error:#}");
    Ok(())
}

fn commit_rename(work_dir: &WorkDir, root: &Path, old: &str, new: &str) -> Result<()> {
    let git = WorktreeGit::pinned_main(root)?;
    let paths = [format!(":(literal){old}"), format!(":(literal){new}")];
    git.run_checked(&["add", "--", &paths[0], &paths[1]])?;
    commit_paths(&git, &paths)?;
    // A hook must not silently leave the committed input different from the checkout.
    let status = git.run(&["diff", "--exit-code", "HEAD", "--", &paths[0], &paths[1]])?;
    if !status.status.success() {
        bail!("Plan rename still differs from HEAD after committing");
    }
    // Loom's git runs without hooks, so the `reference-transaction` hook never
    // attests this commit; attest it here when it holds only the rename.
    let renamed = [Path::new(old), Path::new(new)];
    if let Err(error) = plan_lifecycle::commit::attest_plan_commit(work_dir.root(), root, &renamed)
    {
        tracing::warn!("could not attest the plan rename commit: {error:#}");
    }
    Ok(())
}
