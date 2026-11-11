//! Hung-worker detection for the bounded wait.
//!
//! The lifecycle journal only ever reports what a worker wrote about itself, so
//! a Claude subagent that dies mid-turn -- its process vanishes with no
//! `SubagentStop` record -- stays `Active` forever and the wait sits until its
//! own deadline with nothing to say. This module supplies the missing evidence
//! from transcript growth. A Codex companion job gets the equivalent treatment
//! from `codex_lifecycle::companion_outcome_with_progress`, called directly by
//! `evidence::LifecycleEvidence::codex_outcome` rather than through this module, so the job is
//! located once per poll instead of twice; see that function's doc for its
//! process-liveness and log-freshness evidence.
//!
//! The evidence this module produces is deliberately weaker than a terminal
//! record, which is why it produces its own [`WorkerOutcome::Stalled`]
//! (exit 6) rather than `Failed`.
//!
//! Diagnostic tool-wait classification does not time out. Watch independently
//! treats ordinary tool waits beyond the budget and Bash waits beyond the
//! greater of the budget or the floor as stalled. Agent/Task waits are exempt;
//! none of these heuristics proves that a worker is dead.

use std::path::Path;
use std::time::Duration;

use crate::agent_state::stall_reason;
use crate::subagent_lifecycle::{WorkerIdentity, WorkerOutcome};

use super::super::classify::{
    analyze_with_evidence_at_ceiling, resolve_subagent_ceiling, DEFAULT_DONE_DEBOUNCE_SECS,
};
use super::model::{BoundWorker, WaitIdentity};

/// Classify a worker the lifecycle evidence still calls active.
///
/// `None` means the worker is making progress, or that no evidence settles the
/// question -- in both cases the caller keeps [`WorkerOutcome::Active`].
pub(super) fn detect(
    worker: &BoundWorker,
    work_dir: &Path,
    budget: Duration,
) -> Option<WorkerOutcome> {
    match &worker.lifecycle_identity {
        WorkerIdentity::ClaudeSubagent {
            agent_id,
            transcript_path,
            ..
        } => claude_stall(transcript_path, agent_id, work_dir, budget)
            .map(|detail| WorkerOutcome::Stalled(format!("claude worker {agent_id}: {detail}"))),
        // A companion job is classified by `evidence::LifecycleEvidence::codex_outcome`, which calls
        // `codex_lifecycle::companion_outcome_with_progress` directly so the job
        // is located once per poll instead of twice. A direct Codex invocation
        // is killed by its own supervisor at 1500s, and a teammate has no
        // transcript of its own to measure.
        WorkerIdentity::Codex { .. } | WorkerIdentity::ClaudeTeammate { .. } => None,
    }
}

/// Join every stalled worker's name and detail for one terminal wait result.
pub(super) fn stalled_detail(
    identity: &WaitIdentity,
    outcomes: &[WorkerOutcome],
) -> Option<String> {
    let stalled: Vec<_> = identity
        .workers
        .iter()
        .zip(outcomes)
        .filter_map(|(worker, outcome)| match outcome {
            WorkerOutcome::Stalled(reason) => Some(format!(
                "{} stalled: {reason}",
                super::engine::worker_name(worker)
            )),
            _ => None,
        })
        .collect();
    (!stalled.is_empty()).then(|| stalled.join("; "))
}

/// Read a Claude subagent's transcript and apply the stall rules to it.
///
/// An unreadable transcript yields `None`: a file loom cannot read is not
/// evidence that the agent behind it stopped working.
fn claude_stall(
    transcript: &Path,
    agent_id: &str,
    work_dir: &Path,
    budget: Duration,
) -> Option<String> {
    let summary = analyze_with_evidence_at_ceiling(
        transcript,
        agent_id.to_owned(),
        DEFAULT_DONE_DEBOUNCE_SECS,
        Some(work_dir),
        resolve_subagent_ceiling(Some(work_dir)),
        None,
        None,
    )
    .ok()?;
    stall_reason(
        summary.state,
        summary.idle_secs,
        summary.last_tool.as_deref(),
        budget_secs(budget),
    )
}

/// Idle time is measured in whole seconds as a signed count, so the budget is
/// compared in the same units; a budget too large to represent never fires.
fn budget_secs(budget: Duration) -> i64 {
    i64::try_from(budget.as_secs()).unwrap_or(i64::MAX)
}

#[cfg(test)]
#[path = "stall_tests.rs"]
mod tests;
