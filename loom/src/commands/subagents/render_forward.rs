//! Forwarding collection plus lifecycle-aware watch settlement rules.

use std::path::Path;

use super::super::classify::{self, DoneEvidence, SubagentState, SubagentSummary};
use super::super::{forward_jobs, ledger};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WatchOutcome {
    Settled,
    Failed,
    Cancelled,
    Pending,
}

pub(super) fn append_missing_summaries(
    summaries: &mut Vec<SubagentSummary>,
    forward_index: Option<&forward_jobs::ForwardIndex>,
    work_dir: Option<&Path>,
) {
    let Some(index) = forward_index else {
        return;
    };
    for agent_id in forward_jobs::expected_agent_ids(index) {
        if summaries.iter().any(|summary| summary.agent_id == agent_id) {
            continue;
        }
        let agent_type = ledger::agent_type(work_dir, &agent_id);
        if let Some(summary) = classify::forward::missing_summary(agent_id, agent_type, index) {
            summaries.push(summary);
        }
    }
}

pub(super) fn watch_outcome(
    summaries: &[SubagentSummary],
    lifecycle_required: bool,
) -> WatchOutcome {
    if summaries.iter().any(|summary| {
        matches!(
            summary.state,
            SubagentState::Failed | SubagentState::ForwardFailed
        )
    }) {
        return WatchOutcome::Failed;
    }
    if summaries
        .iter()
        .any(|summary| summary.state == SubagentState::Cancelled)
    {
        return WatchOutcome::Cancelled;
    }
    let settled = !summaries.is_empty()
        && summaries.iter().all(|summary| {
            summary.state == SubagentState::Done
                && (!lifecycle_required || summary.done_evidence == Some(DoneEvidence::Lifecycle))
        });
    if settled {
        WatchOutcome::Settled
    } else {
        WatchOutcome::Pending
    }
}

pub(super) fn exit_code(outcome: WatchOutcome, timed_out: bool) -> Option<i32> {
    match outcome {
        WatchOutcome::Settled => Some(0),
        WatchOutcome::Failed => Some(1),
        WatchOutcome::Cancelled => Some(3),
        WatchOutcome::Pending if timed_out => Some(2),
        WatchOutcome::Pending => None,
    }
}
