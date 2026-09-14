//! Hung-worker detection for the bounded wait.
//!
//! The lifecycle journal only ever reports what a worker wrote about itself, so
//! a Claude subagent that dies mid-turn -- its process vanishes with no
//! `SubagentStop` record -- stays `Active` forever and the wait sits until its
//! own deadline with nothing to say. This module supplies the missing evidence
//! from transcript growth. A Codex companion job gets the equivalent treatment
//! from `codex_lifecycle::companion_outcome_with_progress`, called directly by
//! `engine::codex_outcome` rather than through this module, so the job is
//! located once per poll instead of twice; see that function's doc for its
//! process-liveness and log-freshness evidence.
//!
//! The evidence this module produces is deliberately weaker than a terminal
//! record, which is why it produces its own [`WorkerOutcome::Stalled`]
//! (exit 6) rather than `Failed`.
//!
//! **Tool-wait is not a timeout.** `classify`'s module doc records that a tool
//! call outstanding for 23 minutes is real and normal, so a long `tool-wait` is
//! never on its own a stall. Two rules keep that intact while still catching a
//! dead worker: an `Agent`/`Task` call is never stalled at any idle time,
//! because a nested spawn has no cap of its own, and a `Bash` call is measured
//! against [`BASH_STALL_FLOOR_SECS`] rather than the ordinary budget. Every
//! other tool is bounded tightly enough to measure against the budget itself --
//! across 19,007 sampled tool-waits the longest non-`Bash`, non-`Agent` call
//! was a 121-second `WebFetch`.

use std::path::Path;
use std::time::Duration;

use crate::subagent_lifecycle::{WorkerIdentity, WorkerOutcome};

use super::super::classify::{
    analyze_with_evidence_at_ceiling, resolve_subagent_ceiling, SubagentState,
    DEFAULT_DONE_DEBOUNCE_SECS,
};
use super::model::{BoundWorker, WaitIdentity};

/// Floor under the stall threshold for a `Bash` tool-wait.
///
/// Deliberately NOT the Bash tool's nominal 600-second cap: a call held at a
/// permission prompt has not started running and is not subject to it, so the
/// cap does not bound what a transcript can show. Measured over 19,007 real
/// `Bash` tool-waits in this machine's own subagent transcripts: 71 ran past
/// 600s, 11 past 660s, and the longest reached 1,298s. 1,800s is the first
/// round number above that maximum -- the same convention, and the same cost
/// asymmetry, as [`super::super::classify::DEFAULT_DONE_DEBOUNCE_SECS`]. A
/// false stall would have an orchestrator kill and re-dispatch onto a LIVE
/// worker's file set, so the threshold buys headroom with latency that only
/// ever applies to an already-hung worker.
const BASH_STALL_FLOOR_SECS: i64 = 1_800;

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
        // A companion job is classified by `engine::codex_outcome`, which calls
        // `codex_lifecycle::companion_outcome_with_progress` directly so the job
        // is located once per poll instead of twice. A direct Codex invocation
        // is killed by its own supervisor at 540s, and a teammate has no
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

/// The Claude stall rule table, over one transcript's classified state.
fn stall_reason(
    state: SubagentState,
    idle_secs: i64,
    last_tool: Option<&str>,
    budget_secs: i64,
) -> Option<String> {
    match state {
        SubagentState::Done if idle_secs > budget_secs => Some(format!(
            "turn ended {idle_secs}s ago with no SubagentStop record; harvest its report"
        )),
        SubagentState::Generating if idle_secs > budget_secs => Some(format!(
            "no transcript growth for {idle_secs}s while generating (stall budget {budget_secs}s)"
        )),
        SubagentState::ToolWait => tool_wait_reason(idle_secs, last_tool, budget_secs),
        // `Failed`, `Cancelled` and `Unknown` belong to the lifecycle index, and
        // the forward states belong to the forwarded job's own evidence; a
        // still-running `Done` or `Generating` worker is simply making progress.
        SubagentState::Done
        | SubagentState::Generating
        | SubagentState::Failed
        | SubagentState::Cancelled
        | SubagentState::Unknown
        | SubagentState::ForwardWait
        | SubagentState::ForwardFailed
        | SubagentState::ForwardUnknown => None,
    }
}

fn tool_wait_reason(idle_secs: i64, last_tool: Option<&str>, budget_secs: i64) -> Option<String> {
    match last_tool {
        // A nested spawn runs as long as its own work takes, with no cap to
        // measure it against.
        Some("Agent" | "Task") => None,
        Some("Bash") => {
            let threshold = budget_secs.max(BASH_STALL_FLOOR_SECS);
            (idle_secs > threshold).then(|| {
                format!(
                    "Bash call outstanding {idle_secs}s, past the {threshold}s ceiling on a real one"
                )
            })
        }
        tool => (idle_secs > budget_secs).then(|| {
            let tool = tool.unwrap_or("an unnamed tool");
            format!("waiting on {tool} for {idle_secs}s (stall budget {budget_secs}s)")
        }),
    }
}

/// Idle time is measured in whole seconds as a signed count, so the budget is
/// compared in the same units; a budget too large to represent never fires.
fn budget_secs(budget: Duration) -> i64 {
    i64::try_from(budget.as_secs()).unwrap_or(i64::MAX)
}

#[cfg(test)]
#[path = "stall_tests.rs"]
mod tests;
