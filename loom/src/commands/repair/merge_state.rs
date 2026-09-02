//! Check 11: Phantom merge audit — stages marked merged without their commit in the target branch.
//!
//! WHY THIS EXISTS: A bug in the orchestrator's "defensive fallback" paths can write
//! `merged = true` on a stage whose branch was never actually git-merged into the target
//! branch. This silently gates dependent stages on work that never landed, causing lost work.
//! This check provides a post-hoc safety net that users can run (or that CI can run) to
//! detect these phantom merges before they cause further damage.
//!
//! Only runs when the state directory's stages/ subdirectory exists. Skips Knowledge
//! stages (they legitimately have `merged = true` with no branch/commit — that's by
//! design). For all other stages:
//!   - merged=true + commit present -> verify commit is an ancestor of target branch
//!   - merged=true + no commit      -> warn (cannot verify, needs manual check)
//!   - Completed + !merged + branch gone -> warn (branch deleted without merge confirmation)

use std::path::Path;

use anyhow::{Context, Result};

use super::{RepairIssue, Severity};
use crate::fs::work_dir::{load_config, WorkDir};
use crate::git::branch::{is_ancestor_of, resolve_target_branch};
use crate::git::{branch_exists, branch_name_for_stage};
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::verify::transitions::{list_all_stages, update_stage};

/// Check 11: the phantom-merge audit. See the module doc comment for the
/// full rationale and per-case behavior.
pub(super) fn check(repo_root: &Path) -> Vec<RepairIssue> {
    let mut issues = Vec::new();

    let work_dir = WorkDir::new(repo_root)
        .map(|wd| wd.root().to_path_buf())
        .unwrap_or_else(|_| repo_root.join(".loom").join("work"));
    if !work_dir.is_dir() {
        return issues;
    }

    // Determine the target branch for ancestry checks.
    // Load from config.toml if available, otherwise fall back to repo default.
    let base_branch_opt = load_config(&work_dir)
        .ok()
        .flatten()
        .and_then(|c| c.base_branch());
    let target_branch = resolve_target_branch(&base_branch_opt, repo_root);

    let stages = match list_all_stages(&work_dir) {
        Err(_) => {
            // Cannot enumerate stages (e.g., stages dir missing or unparseable).
            // Push an INFO rather than failing the whole repair run.
            issues.push(RepairIssue {
                severity: Severity::Info,
                description: "Could not audit stage merge status (stages directory unreadable)"
                    .to_string(),
                fix_description: "Investigate the state directory's stages/ subdirectory manually"
                    .to_string(),
            });
            return issues;
        }
        Ok(stages) => stages,
    };

    for stage in &stages {
        // Knowledge stages legitimately have merged=true with no commit —
        // they have no branch and no git work to verify. Skip them.
        if stage.stage_type == StageType::Knowledge {
            continue;
        }
        if stage.merged {
            audit_merged_stage(stage, &target_branch, repo_root, &mut issues);
        } else if stage.status == StageStatus::Completed {
            audit_completed_unmerged(stage, repo_root, &mut issues);
        }
    }

    issues
}

/// merged=true: verify the recorded commit actually landed in the target
/// branch, or warn if there is no commit to verify against.
fn audit_merged_stage(
    stage: &Stage,
    target_branch: &str,
    repo_root: &Path,
    issues: &mut Vec<RepairIssue>,
) {
    let Some(ref commit) = stage.completed_commit else {
        // WARNING: merged=true but no commit SHA to verify against.
        // Cannot confirm whether the work actually landed.
        issues.push(RepairIssue {
            severity: Severity::Warning,
            description: format!(
                "Stage {} marked merged but has no completed_commit (cannot verify)",
                stage.id
            ),
            fix_description: "No automatic fix available — manual investigation required"
                .to_string(),
        });
        return;
    };

    // CRITICAL: merged=true and we have a commit SHA.
    // Verify the commit is actually an ancestor of the target branch.
    // If it isn't, the stage was marked merged without a real merge.
    match is_ancestor_of(commit, target_branch, repo_root) {
        Ok(true) => {
            // All good — commit is in target branch.
        }
        Ok(false) => {
            // Phantom merge: commit exists but is not in target branch.
            issues.push(RepairIssue {
                severity: Severity::Critical,
                description: format!(
                    "Phantom merge: {} marked merged but commit not in {}",
                    stage.id, target_branch
                ),
                fix_description:
                    "Revert merged flag to false (manual investigation needed for lost work)"
                        .to_string(),
            });
        }
        Err(_) => {
            // Git is unavailable or the commit/branch reference is
            // broken. Skip silently rather than producing noise.
        }
    }
}

/// Completed + !merged: warn if the branch was deleted without a merge
/// being recorded, suggesting the work may have been merged manually.
fn audit_completed_unmerged(stage: &Stage, repo_root: &Path, issues: &mut Vec<RepairIssue>) {
    let branch = branch_name_for_stage(&stage.id);
    match branch_exists(&branch, repo_root) {
        Ok(false) => {
            issues.push(RepairIssue {
                severity: Severity::Warning,
                description: format!(
                    "Stale: {} completed but branch deleted without merge confirmation",
                    stage.id
                ),
                fix_description: "No automatic fix available — verify the work was merged manually"
                    .to_string(),
            });
        }
        Ok(true) | Err(_) => {
            // Branch still exists (normal unmerged state) or git is
            // unavailable. Nothing to flag here.
        }
    }
}

/// Revert the `merged` flag on a phantom-merged stage.
///
/// A phantom merge is a stage that has `merged = true` in its state file but whose
/// `completed_commit` is not an ancestor of the target branch — meaning the branch
/// was never actually git-merged. This function sets `merged = false` so the
/// orchestrator treats the stage as unmerged and does not let dependents proceed on
/// the assumption that the work landed.
///
/// We deliberately do NOT attempt a re-merge here. The user's repository may be in
/// an inconsistent state (conflicting later stages, stranded commits) that requires
/// manual investigation before another merge is safe.
pub(super) fn fix_phantom_merge(repo_root: &Path, description: &str) -> Result<()> {
    // Parse stage ID from description: "Phantom merge: <stage-id> marked merged but ..."
    let stage_id = description
        .strip_prefix("Phantom merge: ")
        .and_then(|s| s.split(' ').next())
        .with_context(|| format!("Cannot parse stage ID from: {description}"))?;

    let work_dir = WorkDir::new(repo_root)?.root().to_path_buf();
    update_stage(stage_id, &work_dir, |stage| {
        stage.merged = false;
        Ok(())
    })
    .with_context(|| format!("Failed to save stage '{stage_id}' after reverting merged flag"))?;

    Ok(())
}
