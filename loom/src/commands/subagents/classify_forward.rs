//! Applies forwarding-job evidence after structural transcript classification.

use std::path::Path;

use super::super::forward_jobs::{
    self, ForwardEvidence, ForwardIndex, ForwardOverlay, FORWARDER_TYPE,
};
use super::super::summary;
use super::{DoneEvidence, SubagentState, SubagentSummary};

pub(super) fn apply(summary: &mut SubagentSummary, index: &ForwardIndex, transcript: &Path) {
    let Some(evidence) = forward_jobs::evidence_for_agent(index, &summary.agent_id, transcript)
    else {
        return;
    };
    apply_evidence(summary, evidence);
}

pub(in crate::commands::subagents) fn missing_summary(
    agent_id: String,
    agent_type: Option<String>,
    index: &ForwardIndex,
) -> Option<SubagentSummary> {
    let evidence = forward_jobs::evidence_for_missing_agent(index, &agent_id)?;
    let mut summary = summary::empty(agent_id, 0, agent_type);
    apply_evidence(&mut summary, evidence);
    Some(summary)
}

fn apply_evidence(summary: &mut SubagentSummary, evidence: ForwardEvidence) {
    // Exact child lifecycle evidence owns a forwarder's terminal state.
    if settled_forwarder(summary) {
        return;
    }
    // Active and unknown exact Codex lifecycle states are likewise authoritative
    // for a forwarder. Receipt data remains useful JSON diagnostics, but cannot
    // make the row settle or change its lifecycle-derived state.
    if lifecycle_pending_forwarder(summary) {
        summary.forward = Some(evidence.metadata);
        return;
    }
    // Future non-forwarder lifecycle failures remain authoritative too, while
    // retaining any forwarding metadata for diagnostics.
    if matches!(
        summary.state,
        SubagentState::Failed | SubagentState::Cancelled
    ) {
        summary.forward = Some(evidence.metadata);
        return;
    }
    let transcript_state = summary.state;
    summary.state = state_for(evidence.overlay);
    if summary.state == SubagentState::Done && summary.done_evidence.is_none() {
        summary.done_evidence = Some(DoneEvidence::LegacyTranscript);
    }
    if summary.agent_type.as_deref() != Some(FORWARDER_TYPE) {
        summary.display_state = Some(transcript_state);
    }
    summary.forward = Some(evidence.metadata);
}

fn settled_forwarder(summary: &SubagentSummary) -> bool {
    summary.agent_type.as_deref() == Some(FORWARDER_TYPE)
        && (summary.done_evidence == Some(DoneEvidence::Lifecycle)
            || matches!(
                summary.state,
                SubagentState::Failed | SubagentState::Cancelled
            ))
}

fn lifecycle_pending_forwarder(summary: &SubagentSummary) -> bool {
    summary.agent_type.as_deref() == Some(FORWARDER_TYPE)
        && matches!(
            summary.state,
            SubagentState::ForwardWait | SubagentState::ForwardUnknown
        )
}

fn state_for(overlay: ForwardOverlay) -> SubagentState {
    match overlay {
        ForwardOverlay::Done => SubagentState::Done,
        ForwardOverlay::ForwardWait => SubagentState::ForwardWait,
        ForwardOverlay::ForwardFailed => SubagentState::ForwardFailed,
        ForwardOverlay::ForwardUnknown => SubagentState::ForwardUnknown,
    }
}
