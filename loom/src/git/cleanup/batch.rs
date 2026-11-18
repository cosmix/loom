//! Batch cleanup operations

use anyhow::{Context, Result};
use std::path::Path;

use super::base::cleanup_base_branch;
use super::branch::cleanup_branch;
use super::config::{CleanupConfig, CleanupResult};
use super::worktree::{cleanup_worktree, tracked_scaffold_paths, worktree_directory_exists};
use super::worktree_failure::RetainedScaffold;
use crate::git::branch::branch_name_for_stage;
use crate::git::runner::{run_git, run_git_bool};
use crate::models::worktree::Worktree;

/// Perform full cleanup after a successful merge
///
/// This function removes all resources associated with a stage after its
/// branch has been successfully merged. It's safe to call even if some
/// resources have already been cleaned up.
///
/// # Arguments
/// * `stage_id` - The stage ID to clean up
/// * `repo_root` - Path to the repository root
/// * `config` - Cleanup configuration options
///
/// # Returns
/// A `CleanupResult` describing what was cleaned up
pub fn cleanup_after_merge(
    stage_id: &str,
    repo_root: &Path,
    config: &CleanupConfig,
) -> Result<CleanupResult> {
    let ownership = spool_ownership(stage_id, repo_root, config.force_worktree_removal)?;
    let branch_name = branch_name_for_stage(stage_id);
    drain_spool_before_removal(stage_id, repo_root, ownership);
    let (worktree_removed, warnings) = cleanup_stage_worktree(stage_id, repo_root, config)?;
    let branch_deleted = cleanup_branch(stage_id, repo_root, config.force_branch_deletion)
        .with_context(|| format!("Failed to delete branch '{branch_name}'"))?;
    let base_branch_deleted = cleanup_base_branch(stage_id, repo_root)
        .with_context(|| format!("Failed to delete base branch for stage '{stage_id}'"))?;
    // Git removal reconciles this worktree's registration. Global pruning can
    // discard an unrelated detached HEAD or staged index; keep it explicit.
    if config.prune_worktrees && config.force_worktree_removal {
        prune_worktrees(repo_root).context("Failed to prune worktree metadata")?;
    }

    let result = CleanupResult {
        worktree_removed,
        branch_deleted,
        base_branch_deleted,
        warnings,
    };
    if config.verbose {
        report_cleanup(stage_id, &branch_name, &result);
    }
    Ok(result)
}

#[derive(Default)]
struct SpoolOwnership {
    memory: bool,
    telemetry: bool,
}

fn spool_ownership(stage_id: &str, repo_root: &Path, force: bool) -> Result<SpoolOwnership> {
    crate::validation::validate_id(stage_id).context("Invalid stage ID for cleanup")?;
    let worktree = Worktree::worktree_path(repo_root, stage_id);
    if !worktree_directory_exists(&worktree)? {
        return Ok(SpoolOwnership::default());
    }
    if force {
        return Ok(SpoolOwnership {
            memory: true,
            telemetry: true,
        });
    }
    if !worktree.join(".git").try_exists()? {
        return Ok(SpoolOwnership::default());
    }
    let tracked = tracked_scaffold_paths(&worktree, repo_root)?;
    Ok(SpoolOwnership {
        memory: !tracked.contains(".loom") && !tracked.contains(crate::fs::memory::SPOOL_RELPATH),
        telemetry: !tracked.contains(".loom")
            && !tracked.contains(crate::telemetry::TELEMETRY_SPOOL_RELPATH),
    })
}

fn cleanup_stage_worktree(
    stage_id: &str,
    repo_root: &Path,
    config: &CleanupConfig,
) -> Result<(bool, Vec<String>)> {
    match cleanup_worktree(stage_id, repo_root, config.force_worktree_removal) {
        Ok(removed) => Ok((removed, Vec::new())),
        Err(error) if error.is::<RetainedScaffold>() => Ok((false, vec![error.to_string()])),
        Err(error) => {
            Err(error).with_context(|| format!("Failed to remove worktree for stage '{stage_id}'"))
        }
    }
}

/// Drain a stage's memory and telemetry spools one last time before its
/// worktree is removed, and cancel the stage's host job. The spool files live
/// inside the worktree, so anything still pending when the worktree is
/// deleted is lost for good; a host job still verifying the tree would fail on
/// the missing worktree, so the daemon's runner kills it instead.
///
/// Ownership is checked before entry; tracked or unowned spools are untouched.
/// Draining is best-effort. Non-forced removal subsequently refuses undrained
/// spool contents; explicit forced cleanup can discard them. The daemon's
/// per-tick drain (`orchestrator::core::spool_drain`) handles the normal case,
/// so this is a final attempt before removal.
fn drain_spool_before_removal(stage_id: &str, repo_root: &Path, ownership: SpoolOwnership) {
    let worktree_root = Worktree::worktree_path(repo_root, stage_id);
    if !worktree_root.exists() {
        return;
    }
    let work_dir = match crate::fs::work_dir::WorkDir::new(repo_root) {
        Ok(wd) => wd.root().to_path_buf(),
        Err(e) => {
            tracing::warn!(
                stage_id = %stage_id,
                error = %e,
                "Failed to resolve state directory before draining worktree spools; \
                 undrained spools prevent non-forced cleanup"
            );
            return;
        }
    };
    if ownership.memory {
        drain_memory_spool(stage_id, &work_dir, &worktree_root);
    }
    if ownership.telemetry {
        drain_telemetry_spool(stage_id, &work_dir, &worktree_root);
    }
    if let Err(e) = crate::host_verify::submit::cancel(&work_dir, stage_id, "worktree removed") {
        tracing::warn!(
            stage_id = %stage_id,
            error = %format!("{e:#}"),
            "Failed to cancel the stage's host job before worktree removal"
        );
    }
}

fn drain_memory_spool(stage_id: &str, work_dir: &Path, worktree_root: &Path) {
    match crate::fs::memory::drain_into_journal(work_dir, stage_id, worktree_root) {
        Ok(outcome) if outcome.drained > 0 || outcome.skipped_malformed > 0 => {
            tracing::info!(
                stage_id = %stage_id,
                drained = outcome.drained,
                skipped_malformed = outcome.skipped_malformed,
                "Drained memory spool before worktree removal"
            );
        }
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(
                stage_id = %stage_id,
                worktree_root = %worktree_root.display(),
                error = %e,
                "Failed to drain memory spool before worktree removal; \
                 undrained contents prevent non-forced cleanup"
            );
        }
    }
}

fn drain_telemetry_spool(stage_id: &str, work_dir: &Path, worktree_root: &Path) {
    match crate::telemetry::spool::drain_into_events(work_dir, worktree_root) {
        Ok(outcome) if outcome.drained > 0 || outcome.skipped_malformed > 0 => {
            tracing::info!(
                stage_id = %stage_id,
                drained = outcome.drained,
                skipped_malformed = outcome.skipped_malformed,
                "Drained telemetry spool before worktree removal"
            );
        }
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(
                stage_id = %stage_id,
                worktree_root = %worktree_root.display(),
                error = %e,
                "Failed to drain telemetry spool before worktree removal; undrained contents prevent non-forced cleanup"
            );
        }
    }
}

fn report_cleanup(stage_id: &str, branch_name: &str, result: &CleanupResult) {
    if result.worktree_removed {
        println!("  Removed worktree: .worktrees/{stage_id}");
    }
    if result.branch_deleted {
        println!("  Deleted branch: {branch_name}");
    }
    if result.base_branch_deleted {
        println!("  Deleted base branch: loom/_base/{stage_id}");
    }
    for warning in &result.warnings {
        eprintln!("  Warning: {warning}");
    }
}

/// Prune stale worktree references
///
/// Runs `git worktree prune` to clean up any stale worktree metadata.
pub fn prune_worktrees(repo_root: &Path) -> Result<()> {
    let output =
        run_git(&["worktree", "prune"], repo_root).context("Failed to run git worktree prune")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git worktree prune failed: {stderr}");
    }

    Ok(())
}

/// Clean up multiple stages at once
///
/// # Arguments
/// * `stage_ids` - List of stage IDs to clean up
/// * `repo_root` - Path to the repository root
/// * `config` - Cleanup configuration options
///
/// # Returns
/// Map of stage_id to CleanupResult
pub fn cleanup_multiple_stages(
    stage_ids: &[&str],
    repo_root: &Path,
    config: &CleanupConfig,
) -> Vec<(String, CleanupResult)> {
    let mut results = Vec::with_capacity(stage_ids.len());

    for stage_id in stage_ids {
        let result =
            cleanup_after_merge(stage_id, repo_root, config).unwrap_or_else(|e| CleanupResult {
                worktree_removed: false,
                branch_deleted: false,
                base_branch_deleted: false,
                warnings: vec![e.to_string()],
            });
        results.push(((*stage_id).to_string(), result));
    }

    // Global pruning is reserved for explicitly destructive cleanup.
    if config.prune_worktrees && config.force_worktree_removal {
        let _ = prune_worktrees(repo_root);
    }

    results
}

/// Check if a stage has resources that need cleanup
///
/// Returns true if the stage has a worktree, branch, or base branch that
/// exists. The base branch check matters: a prior cleanup that removed the
/// worktree and branch but failed on `loom/_base/<id>` must still be
/// reported as needing cleanup, or the still-true `cleanup_warning` on the
/// stage is silently stranded.
pub fn needs_cleanup(stage_id: &str, repo_root: &Path) -> bool {
    let worktree_path = repo_root.join(".worktrees").join(stage_id);
    let branch_name = branch_name_for_stage(stage_id);

    // Check worktree exists
    if worktree_path.exists() {
        return true;
    }

    // Check branch exists
    let branch = format!("refs/heads/{branch_name}");
    if run_git_bool(&["rev-parse", "--verify", &branch], repo_root) {
        return true;
    }

    if super::base::base_branch_exists(stage_id, repo_root).unwrap_or(true) {
        return true;
    }
    // A detached administrative HEAD can survive a vanished checkout. Unknown
    // ownership must keep the warning visible until cleanup can inspect it.
    super::worktree_failure::registered_worktree(&worktree_path, repo_root)
        .map_or(true, |registered| registered.is_some())
}

#[cfg(test)]
mod host_job_tests {
    use super::*;
    use crate::host_verify::store::load_current;
    use crate::host_verify::submit::{submit, Submission, SubmitRequest};
    use crate::host_verify::{JobKind, JobState};
    use tempfile::TempDir;

    #[test]
    fn removing_a_worktree_cancels_the_stages_host_job() {
        let repo = TempDir::new().unwrap();
        let work_dir = repo.path().join(".loom").join("work");
        std::fs::create_dir_all(&work_dir).unwrap();
        std::fs::create_dir_all(Worktree::worktree_path(repo.path(), "stage-a")).unwrap();
        let request = SubmitRequest {
            stage_id: "stage-a",
            session_id: "session-a",
            request_id: "r1",
            kind: JobKind::Check,
            head: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            tree_digest: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            no_cache: false,
            criteria: None,
        };
        let job = match submit(&work_dir, &request, chrono::Utc::now()).unwrap() {
            Submission::Queued(record) => record,
            other => panic!("expected a queued job, got {other:?}"),
        };

        drain_spool_before_removal("stage-a", repo.path(), super::SpoolOwnership::default());

        let current = load_current(&work_dir, "stage-a")
            .unwrap()
            .expect("a current record");
        assert_eq!(current.id, job.id);
        assert_eq!(current.state, JobState::Cancelled);
        assert_eq!(current.detail.as_deref(), Some("worktree removed"));
    }
}

#[cfg(test)]
#[path = "batch/redirect_tests.rs"]
mod redirect_tests;
