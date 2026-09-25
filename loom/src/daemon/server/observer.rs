//! The daemon as the one observer of every stage's change fingerprint
//! (DESIGN D12, D13; `verify::review::observer` has the why).
//!
//! `Request::ObserveChanges` names a stage and nothing else. The worktree and
//! target branch come from the stage record and this state directory's own
//! repository root and configuration, never from the caller, who may be a
//! sandboxed agent; `client.rs` settles authentication and stage ownership
//! as for every other self-service request (`self_service.rs`). Git runs
//! pinned to the stage's registered git directory (`WorktreeGit::pinned`),
//! never to whatever the worktree's `.git` file names.
//!
//! A stage session's sandbox cannot reach this socket, so its `loom stage
//! complete` compares no fingerprint. The completion transition
//! (`control_complete.rs`) runs both fingerprint gates here instead, before
//! it applies (`check_completion_gates`).

use anyhow::{Context, Result};
use std::path::Path;

use super::core::DaemonServer;
use super::lock::{inspect_lock, LockState};
use crate::daemon::protocol::Response;
use crate::fs::work_dir::WorkDir;
use crate::git::worktree::WorktreeGit;
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;
use crate::verify::integrity;
use crate::verify::review::fingerprint::{self, ChangeFingerprint};
use crate::verify::review::gate;
use crate::verify::transitions::load_stage;

impl DaemonServer {
    /// Whether this process holds positive evidence that no daemon owns the
    /// state directory `work_dir`: its singleton lock is a regular file that
    /// opens without following a symlink, and a non-blocking exclusive
    /// `flock` on it succeeds (released again at once). A missing lock file,
    /// one that cannot be opened or is no regular file, a held lock and a
    /// failed probe all answer `false`: inside a sandbox, which can hide the
    /// state directory or mount `/dev/null` over a name, none of them proves
    /// anything. Like `check_status`, the probe briefly contends with a
    /// `loom run` starting at the same instant.
    pub(crate) fn proven_stopped(work_dir: &Path) -> bool {
        match inspect_lock(work_dir) {
            LockState::Free(Some(lock)) => lock.metadata().is_ok_and(|meta| meta.is_file()),
            LockState::Free(None) | LockState::Held(_) | LockState::Indeterminate => false,
        }
    }
}

/// Serve one `ObserveChanges`: the stage's change fingerprint as this daemon
/// computes it, with the target branch it measured against. A stage that
/// cannot be fingerprinted comes back as a `Response::Error` naming why.
pub(super) fn handle_observe_changes(work_dir: &Path, stage_id: &str) -> Response {
    match observe(work_dir, stage_id) {
        Ok((target_branch, fingerprint)) => Response::ChangesObserved {
            target_branch,
            fingerprint,
        },
        Err(error) => Response::Error {
            message: format!("Change fingerprint unavailable: {error:#}"),
        },
    }
}

fn observe(work_dir: &Path, stage_id: &str) -> Result<(String, ChangeFingerprint)> {
    // The stage id arrives unvalidated from the wire and names files.
    crate::validation::validate_id(stage_id).context("invalid stage id")?;
    let stage = load_stage(stage_id, work_dir)?;
    let (repo, target_branch) = stage_worktree_and_target(work_dir, &stage)?;
    let fingerprint = fingerprint::compute_local(&repo, &target_branch)?;
    Ok((target_branch, fingerprint))
}

/// DESIGN D13 and D12 for a stage the gates cover (`gate::covers`), measured
/// by this daemon: fail unless `reviews/<stage>/integrity.json` accepts every
/// test-integrity event of the worktree, the latest well-formed review round
/// saw exactly its current changes, and no finding is open. Any other stage
/// passes. The completion transition runs this before it applies.
pub(super) fn check_completion_gates(work_dir: &Path, stage_id: &str) -> Result<()> {
    let stage = load_stage(stage_id, work_dir)?;
    if !gate::covers(&stage) {
        return Ok(());
    }
    let (repo, target_branch) = stage_worktree_and_target(work_dir, &stage)?;
    let changes = fingerprint::compute_local(&repo, &target_branch)
        .context("failed to compute the worktree's change fingerprint for the completion gates")?;
    integrity::check_changes(&stage, work_dir, &repo, &changes)?;
    gate::check(&stage, work_dir, &changes)
}

/// The stage's test-integrity events as this daemon measures them: its
/// worktree scanned against the configured target branch, the base the
/// completion gates use.
pub(super) fn integrity_events(
    work_dir: &Path,
    stage: &Stage,
) -> Result<Vec<integrity::IntegrityEvent>> {
    let (repo, target_branch) = stage_worktree_and_target(work_dir, stage)?;
    let changes = fingerprint::compute_local(&repo, &target_branch)
        .context("failed to list the worktree's changes for the test-integrity check")?;
    Ok(integrity::scan_changes(&repo, &changes, &stage.ratchet_files)?.events)
}

/// Git in the stage's worktree and the target branch its changes are
/// measured against, from this state directory alone: the worktree under its
/// repository root by the stage's validated worktree id, canonicalized, git
/// pinned to its registered git directory in that repository, and the target
/// from its configuration.
fn stage_worktree_and_target(work_dir: &Path, stage: &Stage) -> Result<(WorktreeGit, String)> {
    let workspace = WorkDir::new(work_dir)?;
    let repo_root = workspace
        .repo_root()
        .context("cannot resolve the repository root of the state directory")?;
    let worktree_id = stage.worktree.as_deref().unwrap_or(&stage.id);
    crate::validation::validate_id(worktree_id).context("invalid worktree id")?;
    let worktree = Worktree::worktree_path(repo_root, worktree_id)
        .canonicalize()
        .with_context(|| format!("stage '{}' has no worktree", stage.id))?;
    let repo = WorktreeGit::pinned(repo_root, &worktree)?;
    let target_branch = crate::fs::resolve_target_branch_from_config(work_dir, repo_root)?;
    Ok((repo, target_branch))
}

#[cfg(test)]
#[path = "observer_tests.rs"]
mod tests;
