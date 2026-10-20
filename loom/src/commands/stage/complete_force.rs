//! The `--force-unsafe` completion path.

use anyhow::{Context, Result};
use std::path::Path;

use crate::commands::stage::progressive_complete::{
    branch_work, report_recorded_commit, warn_unconfirmed, BranchWork,
};
use crate::git::worktree::find_repo_root_from_cwd;
use crate::handoff::completion::content::VerificationSnapshot;
use crate::models::stage::{Stage, StageStatus};
use crate::verify::transitions::{trigger_dependents, update_stage};

use crate::commands::stage::acceptance_runner::resolve_stage_execution_paths;

use super::sync_worktree_permissions;

fn warn_force_unsafe(landing: &ForcedLanding) {
    eprintln!();
    eprintln!("⚠️  WARNING: Using --force-unsafe bypasses state machine validation!");
    eprintln!("⚠️  This can corrupt dependency tracking and cause unexpected behavior.");
    eprintln!("⚠️  Use only for manual recovery scenarios.");
    if matches!(landing, ForcedLanding::Unmerged(Some(_))) {
        eprintln!("⚠️  This records the committed branch tip and queues it for the daemon's auto-merge into the target.");
    }
    eprintln!();
}

/// How a forced completion relates to the merge target.
pub(super) enum ForcedLanding {
    /// `--assume-merged`: the router proved the commit is in the target.
    AssumeMerged,
    /// Left unmerged for the daemon's auto-merge (or `loom stage merge`),
    /// which lands the recorded branch tip. `None` records nothing.
    Unmerged(Option<VerificationSnapshot>),
}

/// The branch tip `--force-unsafe` records for landing. A missing or
/// zero-commit branch records nothing, and so does a tip that cannot be
/// captured (missing worktree, uncommitted changes): the administrative
/// override still completes the stage, which then cannot land on its own.
/// The recorded tip lands through the daemon's merge gate, which counts
/// commits again, so an unconfirmed ahead-count is reported and recorded.
pub(super) fn unmerged_tip(
    stage: &Stage,
    repo_root: &Path,
    work_dir: &Path,
) -> Option<VerificationSnapshot> {
    match branch_work(stage, repo_root, work_dir) {
        Ok(BranchWork::Committed(snapshot)) => Some(snapshot),
        Ok(BranchWork::Unconfirmed { snapshot, error }) => {
            warn_unconfirmed(&error);
            Some(snapshot)
        }
        Ok(BranchWork::Missing { .. } | BranchWork::Empty { .. }) => None,
        Err(e) => {
            eprintln!("Warning: no commit recorded for landing: {e:#}");
            None
        }
    }
}

fn apply_forced_merge(stage: &mut Stage, landing: &ForcedLanding) {
    // Only set merged=true if explicitly requested via --assume-merged
    stage.merge_assumed = matches!(landing, ForcedLanding::AssumeMerged);
    stage.merged = stage.merge_assumed;
}

/// Report how the persisted completion lands. Runs only after the completion
/// is written, so a recorded commit is never announced for a refused write.
fn report_forced_landing(stage: &Stage, landing: &ForcedLanding) {
    match landing {
        ForcedLanding::AssumeMerged => {
            println!("  → Stage marked as merged (manual merge assumed)");
        }
        ForcedLanding::Unmerged(Some(snapshot)) => {
            report_recorded_commit(
                &stage.id,
                stage.completed_commit.as_deref(),
                &snapshot.commit,
            );
            println!(
                "  → Not merged yet: the daemon's auto-merge (or `loom stage merge {}`) \
                 lands it and triggers dependents.",
                stage.id
            );
        }
        ForcedLanding::Unmerged(None) => {
            eprintln!();
            eprintln!(
                "⚠️  WARNING: Stage NOT marked as merged and no commit recorded for landing."
            );
            eprintln!("⚠️  Dependent stages will NOT be automatically triggered.");
            eprintln!("⚠️  If you manually merged the branch, re-run with --assume-merged to trigger dependents.");
            eprintln!();
        }
    }
}

fn persist_forced_completion(
    stage: &Stage,
    landing: &ForcedLanding,
    work_dir: &Path,
) -> Result<()> {
    // Re-apply only the force-completion-owned fields (forced status, merged,
    // merge_assumed, completed_commit) onto the FRESH on-disk stage so an
    // administrative force-complete does not revert concurrent daemon/dispute
    // writes to unrelated fields (A-5). force_status_with_reason bypasses
    // transition validation by design — this is the documented administrative
    // override. A recorded tip must still match the branch under the lock.
    let forced_merged = stage.merged;
    let forced_merge_assumed = stage.merge_assumed;
    let forced_commit = stage.completed_commit.clone();
    update_stage(&stage.id, work_dir, |s| {
        s.completed_commit = match landing {
            ForcedLanding::Unmerged(Some(snapshot)) => {
                snapshot.validate_locked(s, work_dir)?;
                Some(snapshot.commit.clone())
            }
            _ => forced_commit.clone(),
        };
        s.force_status_with_reason(
            StageStatus::Completed,
            "--force-unsafe: administrative force-completion from any state",
        );
        s.merged = forced_merged;
        s.merge_assumed = forced_merge_assumed;
        Ok(())
    })?;
    Ok(())
}

fn trigger_forced_dependents(stage_id: &str, work_dir: &Path) -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let repo_root = find_repo_root_from_cwd(&cwd).unwrap_or_else(|| cwd.clone());
    let target_branch = crate::fs::resolve_target_branch_from_config(work_dir, &repo_root)?;
    let triggered = trigger_dependents(stage_id, work_dir, &repo_root, &target_branch)
        .context("Failed to trigger dependent stages")?;

    if !triggered.is_empty() {
        println!("Triggered {} dependent stage(s):", triggered.len());
        for dep_id in &triggered {
            println!("  → {dep_id}");
        }
    }
    Ok(())
}

/// Handle force-unsafe completion mode.
///
/// Bypasses state machine validation and marks stage as completed directly.
/// This is a manual recovery command for administrative use only.
///
/// # Invariant
///
/// **Callers MUST invoke `route_complete_for_conflicts` first.** This function
/// performs no ancestry check on its own — the verified-route guarantees the
/// router already established that the commit is in the target branch's
/// history (`AssumeMerged`) or that no active merge would be orphaned
/// (`Unmerged`).
pub(super) fn handle_force_unsafe_completion(
    stage: &Stage,
    landing: ForcedLanding,
    work_dir: &Path,
) -> Result<()> {
    warn_force_unsafe(&landing);
    let mut stage = stage.clone();
    let stage_id = stage.id.clone();

    // Best-effort permission sync before force-completing
    // Uses resolve_stage_execution_paths to get worktree paths, same as normal completion
    if let Ok(execution_paths) = resolve_stage_execution_paths(&stage) {
        sync_worktree_permissions(
            &execution_paths.worktree_root,
            &execution_paths.acceptance_dir,
        );
    }

    println!(
        "Force-completing stage '{}' (was: {:?})",
        stage_id, stage.status
    );

    // Forced status assignment: --force-unsafe is an explicit administrative
    // override that may be invoked from any source status. Use
    // force_status_with_reason so the bypass is logged and visible.
    stage.force_status_with_reason(
        StageStatus::Completed,
        "--force-unsafe: administrative force-completion from any state",
    );

    apply_forced_merge(&mut stage, &landing);
    persist_forced_completion(&stage, &landing, work_dir)?;
    report_forced_landing(&stage, &landing);
    println!("Stage '{stage_id}' force-completed!");

    // Only trigger dependent stages if merged=true (i.e., --assume-merged was used)
    if stage.merged {
        trigger_forced_dependents(&stage_id, work_dir)?;
    }

    Ok(())
}

#[cfg(test)]
#[path = "complete_force_tests.rs"]
mod tests;
