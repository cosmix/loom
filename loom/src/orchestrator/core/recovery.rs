//! Error recovery and state synchronization

use anyhow::{Context, Result};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

#[cfg(test)]
use super::recovery_guards;
#[cfg(test)]
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::retry::{calculate_backoff, is_backoff_elapsed, should_auto_retry};
use crate::orchestrator::session_registry::orphan_evidence;
use crate::verify::transitions::update_stage_at_path;

use super::persistence::Persistence;
use super::Orchestrator;

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct StageScanCounter {
    pub(super) directory_reads: usize,
    pub(super) entries_visited: usize,
}

/// Enumerate stage paths exactly once. Callers load the already-known path
/// directly instead of resolving every ID through another directory scan.
pub(super) fn scan_stage_paths(
    stages_dir: &Path,
    counter: &mut StageScanCounter,
) -> Result<Vec<PathBuf>> {
    counter.directory_reads += 1;
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(stages_dir)? {
        counter.entries_visited += 1;
        let path = entry?.path();
        if path.extension().and_then(|s| s.to_str()) == Some("md") {
            paths.push(path);
        }
    }
    Ok(paths)
}

pub(super) fn load_stage_at_path(path: &Path) -> Result<Stage> {
    let content = crate::fs::locking::locked_read(path)?;
    crate::verify::transitions::parse_stage_from_markdown(&content)
        .with_context(|| format!("Failed to parse stage from: {}", path.display()))
}

/// Trait for recovery operations
pub(super) trait Recovery: Persistence {
    /// Reconcile any active main-repo merge with stage state on disk and
    /// update the in-memory graph if a stage was mutated.
    ///
    /// MUST run BEFORE `sync_graph_with_stage_files` AND BEFORE
    /// `recover_orphaned_sessions`:
    /// - Recovery deletes orphaned merge sessions; attribution depends on
    ///   their metadata.
    /// - Sync reads stage files into the graph; if reconcile flips
    ///   `Completed + merged=true` -> `MergeConflict + merged=false` AFTER
    ///   sync, the graph still has the stale view and would queue dependents
    ///   based on a phantom merge.
    fn reconcile_and_update_graph(&mut self) -> Result<()>;

    /// Sync the execution graph with existing stage file statuses.
    /// This syncs FROM files TO graph.
    fn sync_graph_with_stage_files(&mut self) -> Result<()>;

    /// Sync queued status from graph back to stage files.
    /// This ensures files reflect when dependencies are satisfied.
    /// This syncs FROM graph TO files.
    fn sync_queued_status_to_files(&mut self) -> Result<()>;

    /// Re-adopt live agents that have no session record at all, rebuilding
    /// the record and relinking the stage to it. The mirror image of
    /// [`Self::recover_orphaned_sessions`], which iterates session FILES and
    /// so cannot see an agent that never got one; runs FIRST from inside
    /// that method for the same reason. Never fails the recovery pass —
    /// per-agent failures are logged and skipped.
    fn adopt_orphaned_agents(&mut self) -> usize;

    /// Recover orphaned sessions (process died but session/stage files exist).
    fn recover_orphaned_sessions(&mut self) -> Result<usize>;

    /// Check if all stages are in a terminal state (for watch mode exit)
    fn all_stages_terminal(&self) -> bool;

    /// Print a status update showing current stage counts
    fn print_status_update(&self);
}

/// Check whether a Blocked stage is *retryable at all* — i.e. it has a
/// retryable failure type and has not exhausted its retry budget — WITHOUT
/// considering backoff timing.
///
/// This is the shared predicate behind both the requeue decision and the
/// watch-mode exit check. `all_stages_terminal()` uses it so the daemon does
/// NOT shut down while a crashed stage is still *pending backoff* (the backoff
/// has not elapsed yet, so `check_retry_eligibility` is currently false, but
/// the retry WILL fire on a later tick). See O-1.
///
/// # Arguments
/// * `stage` - The stage to check
///
/// # Returns
/// `true` if the stage will be auto-retried now or after its backoff elapses.
fn is_retry_pending(stage: &Stage) -> bool {
    let Some(ref info) = stage.failure_info else {
        return false;
    };
    let max = stage.max_retries.unwrap_or(3);
    should_auto_retry(&info.failure_type, stage.retry_count, max)
}

/// Check if a blocked stage is eligible for automatic retry *right now*.
///
/// A stage is eligible for retry if:
/// - It is retryable at all (`is_retry_pending`: retryable failure_type and
///   retry_count < max_retries (default 3))
/// - Sufficient time has elapsed since the last failure (exponential backoff)
///
/// # Arguments
/// * `stage` - The stage to check
///
/// # Returns
/// `true` if the stage should be automatically retried now, `false` otherwise
pub(super) fn check_retry_eligibility(stage: &Stage) -> bool {
    if !is_retry_pending(stage) {
        return false;
    }

    // Calculate backoff: base 30s, max 300s (5 minutes)
    let backoff = calculate_backoff(stage.retry_count, 30, 300);
    is_backoff_elapsed(stage.last_failure_at, backoff)
}

/// Decide, from a freshly loaded stage FILE, whether it should keep the
/// watch-mode daemon alive. Pure and exhaustive over `StageStatus` so a new
/// variant fails to compile here instead of silently defaulting either way.
///
/// - `Completed`/`Skipped`/`CompletedWithFailures`/`NeedsHumanReview` are terminal.
/// - `MergeConflict`/`MergeBlocked` are never terminal: each tick the resolver spawn
///   loop (`spawn_resolver_if_due`) leaves a resolver running, retries a transient
///   failure, or routes the stage to `NeedsHumanReview`, which is how a spent resolver
///   budget, a held or missing branch, or an unrecordable attempt ends the wait.
/// - `Blocked` is terminal unless a crash auto-retry is pending (retryable failure
///   type + retry budget left), whether or not its backoff has elapsed (O-1).
/// - `Queued`/`WaitingForDeps` are terminal only when the stage is held.
/// - `NeedsAdjudication`/`Executing`/`WaitingForInput`/`NeedsHandoff` are never
///   terminal: each still needs the daemon to spawn, watch, or close out a session.
fn stage_file_is_terminal(stage: &Stage) -> bool {
    match stage.status {
        StageStatus::Completed
        | StageStatus::Skipped
        | StageStatus::CompletedWithFailures
        | StageStatus::NeedsHumanReview => true,
        StageStatus::Blocked => !is_retry_pending(stage),
        StageStatus::Queued | StageStatus::WaitingForDeps => stage.held,
        StageStatus::MergeConflict
        | StageStatus::MergeBlocked
        | StageStatus::NeedsAdjudication
        | StageStatus::Executing
        | StageStatus::WaitingForInput
        | StageStatus::NeedsHandoff => false,
    }
}

impl Orchestrator {
    /// Re-verify a `Completed + merged=true` non-knowledge stage at sync time.
    ///
    /// Derives `completed_commit` from `loom/<id>` HEAD when missing, then
    /// checks ancestry with `merged_into_accepted()` against the accepted tip of
    /// the (pre-resolved) target. If that check fails OR the branch is missing, reverts
    /// `merged=false` so dependents don't treat the stage as satisfied. A
    /// guard that cannot be evaluated leaves the flag unchanged.
    ///
    /// `target` is passed in so the caller resolves it once per sync pass
    /// rather than spawning `git symbolic-ref` per stage (P-3).
    ///
    /// # Returns
    /// `true` if the stage is verified `merged=true` (ancestry holds) — the
    /// caller memoizes this so the verification is not repeated every tick.
    /// `false` if the merge was reverted or could not be verified.
    pub(super) fn verify_merged_true_or_revert(
        &mut self,
        stage: &mut Stage,
        stage_path: &Path,
        target: &str,
    ) -> bool {
        let original_commit = stage.completed_commit.clone();
        let commit = original_commit.clone();
        let Some(verified) = self.probe_merged(&stage.id, commit.as_deref(), target) else {
            return false;
        };

        if !verified {
            tracing::error!(
                stage_id = %stage.id,
                commit = ?commit,
                target = %target,
                "Phantom merge detected at sync; reverting merged=false when the \
                 canonical commit still matches this verification probe"
            );
        }

        let mut applicable = false;
        let result =
            update_stage_at_path(&stage.id, stage_path, &self.config.work_dir, |current| {
                let commit_matches = current.completed_commit == original_commit;
                applicable = commit_matches;
                if commit_matches && !verified {
                    current.merged = false;
                }
                Ok(())
            });
        match result {
            Ok(updated) => {
                *stage = updated;
                applicable && verified && stage.merged
            }
            Err(error) => {
                tracing::warn!(stage_id = %stage.id, %error, "Failed to persist merge verification");
                false
            }
        }
    }
}

impl Recovery for Orchestrator {
    fn reconcile_and_update_graph(&mut self) -> Result<()> {
        use crate::orchestrator::merge_attribution::{
            reconcile_main_repo_active_merge, ReconciliationOutcome,
        };
        match reconcile_main_repo_active_merge(&self.config.repo_root, &self.config.work_dir)? {
            ReconciliationOutcome::NoActiveMerge
            | ReconciliationOutcome::UnattributedLogged
            | ReconciliationOutcome::AttributedNoOp { .. } => {}
            ReconciliationOutcome::StageMutated { stage_id, .. } => {
                // Disk was corrected; update the graph immediately so any
                // caller in this iteration sees the corrected state. The
                // next sync_graph_with_stage_files call will pick this up
                // again, which is harmless (idempotent).
                // P-3: a phantom-merge revert changes git ancestry reality, so
                // invalidate the verified-merged memo for this stage; the next
                // sync re-verifies it instead of trusting the cached result.
                self.verified_merged.remove(&stage_id);
                self.graph.set_node_merged(&stage_id, false);
                if let Err(e) = self
                    .graph
                    .mark_status(&stage_id, StageStatus::MergeConflict)
                {
                    tracing::warn!(
                        stage_id = %stage_id,
                        error = %e,
                        "Failed to update graph after phantom-merge revert; \
                         next sync will reconcile."
                    );
                }
            }
        }
        Ok(())
    }

    fn sync_graph_with_stage_files(&mut self) -> Result<()> {
        self.sync_authoritative_stages()
    }

    fn sync_queued_status_to_files(&mut self) -> Result<()> {
        self.sync_queued_files()
    }

    fn adopt_orphaned_agents(&mut self) -> usize {
        let work_dir = self.config.work_dir.clone();
        let stages_dir = work_dir.join("stages");
        let mut adopted = 0;

        for evidence in orphan_evidence(&work_dir) {
            if self.try_adopt_orphan(&work_dir, &stages_dir, &evidence) {
                adopted += 1;
            }
        }

        adopted
    }

    fn recover_orphaned_sessions(&mut self) -> Result<usize> {
        self.recover_current_sessions()
    }

    fn all_stages_terminal(&self) -> bool {
        // Don't exit while merge resolution sessions are running — the daemon
        // needs to stay alive to monitor them and handle their completion.
        if !self.active_sessions.is_empty() {
            return false;
        }

        self.stage_snapshot().is_ok_and(|snapshot| {
            snapshot
                .records
                .values()
                .all(|(stage, _)| stage_file_is_terminal(stage))
        })
    }

    fn print_status_update(&self) {
        // The `\r[Polling...]` line only makes sense on a TTY, where the
        // carriage return redraws it in place. When the daemon redirects
        // stdout to a log file (lifecycle.rs), these lines accumulate as
        // noise interleaved with tracing output (A-15 / O-17). Suppress them
        // when stdout is not a terminal; real tracing output is unaffected.
        if !io::stdout().is_terminal() {
            return;
        }

        let nodes = self.graph.all_nodes();
        let mut running = 0;
        let mut pending = 0;
        let mut completed = 0;
        let mut blocked = 0;

        for node in nodes {
            match node.status {
                StageStatus::Executing => running += 1,
                StageStatus::WaitingForDeps | StageStatus::Queued => pending += 1,
                StageStatus::Completed => completed += 1,
                StageStatus::Blocked => blocked += 1,
                StageStatus::Skipped => completed += 1, // Count skipped as completed for status display
                StageStatus::WaitingForInput => running += 1, // Paused but still active
                StageStatus::NeedsHandoff => running += 1, // Needs continuation but still in progress
                StageStatus::MergeConflict => blocked += 1, // Blocked on conflict resolution
                StageStatus::CompletedWithFailures => blocked += 1, // Failed acceptance, needs retry
                StageStatus::MergeBlocked => blocked += 1,          // Blocked on merge error
                StageStatus::NeedsHumanReview => blocked += 1,      // Waiting for human review
                StageStatus::NeedsAdjudication => blocked += 1, // Waiting for adjudicator verdict
            }
        }

        let mut status_parts = vec![
            format!("{running} running"),
            format!("{pending} pending"),
            format!("{completed} completed"),
        ];

        if blocked > 0 {
            status_parts.push(format!("{blocked} blocked"));
        }

        print!(
            "\r[Polling... {}] (Ctrl+C to detach, 'loom status' for details)    ",
            status_parts.join(", ")
        );
        // Flush stdout to ensure the status line appears immediately
        let _ = io::stdout().flush();
    }
}

#[cfg(test)]
#[path = "recovery_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "recovery_adoption_tests.rs"]
mod recovery_adoption_tests;

#[cfg(test)]
#[path = "recovery_guard_tests.rs"]
mod recovery_guard_tests;

#[cfg(test)]
#[path = "recovery_sync_tests.rs"]
mod recovery_sync_tests;

#[cfg(test)]
#[path = "recovery_terminal_tests.rs"]
mod recovery_terminal_tests;
