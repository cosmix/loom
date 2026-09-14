//! Forwarding collection for list and harvest.

use std::path::Path;

use super::super::classify::{self, SubagentSummary};
use super::super::{forward_jobs, ledger};

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
