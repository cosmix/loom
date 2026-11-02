//! The daemon's sweep over merged stages that still hold a worktree or a
//! branch. Cleanup after a merge is deferred while a session still runs for
//! the stage, and a stage merged by the CLI from inside its own worktree
//! leaves the worktree behind. Nothing else revisits either, so each tick
//! retries them here, through the same `MergeLifecycle::cleanup` door that
//! refuses unless the stage's work is contained in the target branch.

use std::time::{Duration, Instant};

use crate::models::stage::{StageStatus, StageType};
use crate::orchestrator::core::persistence::Persistence;
use crate::orchestrator::core::{clear_status_line, Orchestrator};
use crate::orchestrator::merge_lifecycle::{CleanupOutcome, MergeLifecycle};
use crate::plan::StageNode;

/// How long a cleanup may stay deferred before the stage gets a warning.
const DEFERRAL_WARN_AFTER: Duration = Duration::from_secs(10 * 60);

impl Orchestrator {
    /// Clean up the worktree and branch of every `Completed` and `merged`
    /// non-knowledge stage not yet settled this daemon session.
    ///
    /// Candidates come from the in-memory graph; a stage file is read only for
    /// a candidate. A deferred cleanup is tried again next tick. Any other
    /// outcome settles the stage: it was cleaned, had nothing to clean, or was
    /// refused or failed and reported, which a retry would repeat unchanged.
    /// A deferral that lasts ten minutes is recorded once on the stage as a
    /// cleanup warning; the sweep keeps retrying.
    pub(in crate::orchestrator::core) fn sweep_merged_leftovers(&mut self) {
        self.sweep_merged_leftovers_at(Instant::now());
    }

    fn sweep_merged_leftovers_at(&mut self, now: Instant) {
        let graph = &self.graph;
        // A stage that left `Completed` and `merged` (re-queued, say) is swept
        // afresh when it merges again.
        self.settled_leftovers
            .retain(|id| is_merged(graph.get_node(id)));
        self.deferred_cleanups
            .retain(|id, _| is_merged(graph.get_node(id)));
        let candidates: Vec<String> = graph
            .all_nodes()
            .into_iter()
            .filter(|node| is_merged(Some(node)) && !self.settled_leftovers.contains(&node.id))
            .map(|node| node.id.clone())
            .collect();
        for stage_id in candidates {
            if self.sweep_one_leftover(&stage_id, now) {
                self.settled_leftovers.insert(stage_id);
            }
        }
    }

    /// Sweep `stage_id`; returns whether it is settled. A stage file that is
    /// unreadable or no longer `Completed` and `merged` stays unsettled.
    fn sweep_one_leftover(&mut self, stage_id: &str, now: Instant) -> bool {
        let stage = match self.load_stage(stage_id) {
            Ok(stage) => stage,
            Err(error) => {
                tracing::debug!(stage_id, %error, "Leftover sweep could not read the stage");
                return false;
            }
        };
        if stage.status != StageStatus::Completed || !stage.merged {
            return false;
        }
        // Knowledge stages run in the main checkout and own no worktree.
        if stage.stage_type == StageType::Knowledge {
            return true;
        }
        match self.cleanup_already_merged(stage_id) {
            CleanupOutcome::Deferred { reason } => {
                self.note_deferred_cleanup(stage_id, &reason, now);
                false
            }
            _ => {
                self.deferred_cleanups.remove(stage_id);
                true
            }
        }
    }

    /// Remember when `stage_id`'s deferral was first seen; once it has lasted
    /// [`DEFERRAL_WARN_AFTER`], record the cleanup warning, once.
    fn note_deferred_cleanup(&mut self, stage_id: &str, reason: &str, now: Instant) {
        let (since, warned) = self
            .deferred_cleanups
            .entry(stage_id.to_string())
            .or_insert((now, false));
        if *warned || now.saturating_duration_since(*since) < DEFERRAL_WARN_AFTER {
            return;
        }
        *warned = true;
        MergeLifecycle::new(stage_id, &self.config.repo_root, &self.config.work_dir)
            .record_cleanup_warning(Some(format!(
                "cleanup deferred for over 10 minutes: {reason}; run `loom worktree remove \
                 {stage_id}` once nothing uses it"
            )));
    }
}

fn is_merged(node: Option<&StageNode>) -> bool {
    node.is_some_and(|node| node.status == StageStatus::Completed && node.merged)
}

/// Make a failed or refused deferred cleanup visible on the daemon's console.
///
/// `MergeLifecycle::cleanup` already logs at `warn`, but the daemon's tracing
/// goes to stderr nobody watches; the stage is Completed and merged either
/// way, so the only thing lost by silence is the worktree the user later
/// finds still on disk. A deferral is silent: a later attempt finishes it.
pub(super) fn report_deferred_cleanup(stage_id: &str, outcome: &CleanupOutcome) {
    let (verb, detail) = match outcome {
        CleanupOutcome::Failed(error) => ("failed", error.clone()),
        CleanupOutcome::Refused { reason } => ("refused", reason.clone()),
        CleanupOutcome::Done(result) if !result.warnings.is_empty() => {
            ("retained a path", result.warnings.join("; "))
        }
        CleanupOutcome::Done(_) | CleanupOutcome::NothingToDo | CleanupOutcome::Deferred { .. } => {
            return
        }
    };
    clear_status_line();
    eprintln!("Warning: deferred cleanup for stage '{stage_id}' {verb}: {detail}");
    eprintln!("  Clean up manually with: loom worktree remove {stage_id}");
}

#[cfg(test)]
#[path = "leftover_sweep_tests.rs"]
mod tests;
