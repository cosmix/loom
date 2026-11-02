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

use super::{DetectedIssue, RepairAction, Severity};
use crate::fs::work_dir::{load_config, WorkDir};
use crate::git::branch::{branch_ref, is_ancestor_of, resolve_target_branch};
use crate::git::{branch_exists, branch_name_for_stage};
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::verify::transitions::{list_all_stages, update_stage};

/// Check 11: the phantom-merge audit. See the module doc comment for the
/// full rationale and per-case behavior.
pub(super) fn check(repo_root: &Path) -> Vec<DetectedIssue> {
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
            issues.push(DetectedIssue {
                action: RepairAction::Manual,
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
        audit_stage(stage, &target_branch, repo_root, &mut issues);
    }

    issues
}

/// Dispatch one stage to the audit that applies to it, if any.
fn audit_stage(
    stage: &Stage,
    target_branch: &str,
    repo_root: &Path,
    issues: &mut Vec<DetectedIssue>,
) {
    // Knowledge stages legitimately have merged=true with no commit —
    // they have no branch and no git work to verify.
    if stage.stage_type == StageType::Knowledge {
        return;
    }
    // An operator's assumed merge is final, not a phantom merge.
    if stage.merge_assumed {
        return;
    }
    if stage.merged {
        audit_merged_stage(stage, target_branch, repo_root, issues);
    } else if stage.status == StageStatus::Completed {
        audit_completed_unmerged(stage, repo_root, issues);
    }
}

/// merged=true: verify the recorded commit actually landed in the target
/// branch, or warn if there is no commit to verify against.
fn audit_merged_stage(
    stage: &Stage,
    target_branch: &str,
    repo_root: &Path,
    issues: &mut Vec<DetectedIssue>,
) {
    let Some(ref commit) = stage.completed_commit else {
        // WARNING: merged=true but no commit SHA to verify against.
        // Cannot confirm whether the work actually landed.
        issues.push(DetectedIssue {
            action: RepairAction::Manual,
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
    match is_ancestor_of(commit, &branch_ref(target_branch), repo_root) {
        Ok(true) => {
            // All good — commit is in target branch.
        }
        Ok(false) => {
            // Phantom merge: commit exists but is not in target branch.
            issues.push(DetectedIssue {
                action: RepairAction::PhantomMerge(stage.id.clone()),
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
fn audit_completed_unmerged(stage: &Stage, repo_root: &Path, issues: &mut Vec<DetectedIssue>) {
    let branch = branch_name_for_stage(&stage.id);
    match branch_exists(&branch, repo_root) {
        Ok(false) => {
            issues.push(DetectedIssue {
                action: RepairAction::Manual,
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
pub(super) fn fix_phantom_merge(repo_root: &Path, stage_id: &str) -> Result<()> {
    let work_dir = WorkDir::new(repo_root)?.root().to_path_buf();
    update_stage(stage_id, &work_dir, |stage| {
        stage.merged = false;
        Ok(())
    })
    .with_context(|| format!("Failed to save stage '{stage_id}' after reverting merged flag"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::process::Command;

    use tempfile::TempDir;

    use super::*;

    fn git(root: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?} failed");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// A repo on `main` plus a commit on a side branch that `main` lacks.
    fn repo_with_stranded_commit() -> (TempDir, String) {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        git(root, &["init", "-q", "-b", "main"]);
        git(root, &["config", "user.email", "t@t.com"]);
        git(root, &["config", "user.name", "T"]);
        git(root, &["commit", "-q", "--allow-empty", "-m", "initial"]);
        git(root, &["checkout", "-q", "-b", "side"]);
        git(root, &["commit", "-q", "--allow-empty", "-m", "stranded"]);
        let commit = git(root, &["rev-parse", "HEAD"]);
        git(root, &["checkout", "-q", "main"]);
        (dir, commit)
    }

    fn completed_stage(root: &Path, commit: &str, assumed: bool) -> (std::path::PathBuf, String) {
        let work_dir = WorkDir::new(root).unwrap().root().to_path_buf();
        let mut stage = Stage::new("s".to_string(), None);
        stage.status = StageStatus::Completed;
        stage.merged = true;
        stage.merge_assumed = assumed;
        stage.completed_commit = Some(commit.to_string());
        crate::verify::transitions::save_stage(&stage, &work_dir).unwrap();
        (work_dir, stage.id)
    }

    #[test]
    fn an_assumed_merge_is_final_not_a_phantom_merge() {
        let (dir, commit) = repo_with_stranded_commit();
        let (work_dir, id) = completed_stage(dir.path(), &commit, true);

        assert!(check(dir.path()).is_empty());
        // `loom repair --fix` reaches fix_phantom_merge only through a finding.
        let stage = crate::verify::transitions::load_stage(&id, &work_dir).unwrap();
        assert!(stage.merged && stage.merge_assumed);
    }

    #[test]
    fn a_merged_stage_missing_from_the_target_is_still_a_phantom_merge() {
        let (dir, commit) = repo_with_stranded_commit();
        let (work_dir, id) = completed_stage(dir.path(), &commit, false);

        let mut issues = check(dir.path());

        assert_eq!(issues.len(), 1);
        assert!(issues[0].description.starts_with("Phantom merge: "));
        assert!(matches!(&issues[0].action, RepairAction::PhantomMerge(target) if target == &id));
        issues[0].description = "Changed diagnostic wording".into();
        assert!(super::super::fix_issue(dir.path(), &issues[0]).unwrap());
        assert!(
            !crate::verify::transitions::load_stage(&id, &work_dir)
                .unwrap()
                .merged
        );
    }

    #[test]
    fn completed_stage_with_missing_branch_is_manual() {
        let (dir, _) = repo_with_stranded_commit();
        let work = WorkDir::new(dir.path()).unwrap().root().to_path_buf();
        let mut stage = Stage::new("missing-branch".into(), None);
        stage.status = StageStatus::Completed;
        crate::verify::transitions::save_stage(&stage, &work).unwrap();
        let issues = check(dir.path());
        let issue = issues
            .iter()
            .find(|issue| issue.description.starts_with("Stale:"))
            .expect("missing branch must be reported");
        assert_eq!(issue.action, RepairAction::Manual);
    }
}
