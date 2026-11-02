//! Summary-facing views of a stage-scoped forwarding receipt index.

use std::collections::BTreeSet;

use serde::Serialize;

use super::{
    combine_overlays, matching_receipts, overlay_from_observation, overlay_from_state,
    receipt_state, ForwardIndex, ForwardOverlay,
};
use crate::models::forward_receipt::{ForwardReceipt, ForwardState};

/// JSON-only backend details for an expected forwarding call.
#[derive(Debug, Clone, Serialize)]
pub(in crate::commands::subagents) struct ForwardMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::subagents) receipt_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::subagents) backend_id: Option<String>,
    pub(in crate::commands::subagents) state: ForwardState,
}

pub(in crate::commands::subagents) struct ForwardEvidence {
    pub(in crate::commands::subagents) overlay: ForwardOverlay,
    pub(in crate::commands::subagents) metadata: ForwardMetadata,
}

pub(in crate::commands::subagents) fn evidence_for_agent(
    index: &ForwardIndex,
    agent_id: &str,
    transcript: &super::super::forward_jobs_transcript::ForwardTranscript,
) -> Option<ForwardEvidence> {
    let receipts = matching_receipts(index, agent_id, transcript);
    let states = observe_receipts(index, &receipts);
    let overlay = overlay_from_observation(index, agent_id, transcript, &receipts, &states)?;
    Some(ForwardEvidence {
        overlay,
        metadata: receipt_metadata(index, &receipts, &states, overlay),
    })
}

pub(in crate::commands::subagents) fn evidence_for_missing_agent(
    index: &ForwardIndex,
    agent_id: &str,
) -> Option<ForwardEvidence> {
    let receipts: Vec<_> = index
        .receipts
        .iter()
        .filter(|receipt| receipt.identity.agent_id == agent_id)
        .collect();
    if receipts.is_empty() {
        return None;
    }
    let states = observe_receipts(index, &receipts);
    let overlay = if index.unreadable {
        ForwardOverlay::ForwardUnknown
    } else {
        combine_overlays(
            states
                .iter()
                .map(|state| overlay_from_state(*state, false))
                .collect(),
        )
    };
    Some(ForwardEvidence {
        overlay,
        metadata: receipt_metadata(index, &receipts, &states, overlay),
    })
}

pub(in crate::commands::subagents) fn expected_agent_ids(index: &ForwardIndex) -> Vec<String> {
    index
        .receipts
        .iter()
        .map(|receipt| receipt.identity.agent_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn observe_receipts(index: &ForwardIndex, receipts: &[&ForwardReceipt]) -> Vec<ForwardState> {
    if index.unreadable {
        return vec![ForwardState::Unknown; receipts.len()];
    }
    receipts
        .iter()
        .map(|receipt| receipt_state(index, receipt))
        .collect()
}

fn receipt_metadata(
    index: &ForwardIndex,
    receipts: &[&ForwardReceipt],
    states: &[ForwardState],
    overlay: ForwardOverlay,
) -> ForwardMetadata {
    if !index.unreadable && receipts.len() == 1 {
        let receipt = receipts[0];
        return ForwardMetadata {
            receipt_id: Some(receipt.receipt_id.clone()),
            backend_id: Some(receipt.backend_id.clone()),
            state: states[0],
        };
    }
    ForwardMetadata {
        receipt_id: None,
        backend_id: None,
        state: overlay_state(overlay),
    }
}

fn overlay_state(overlay: ForwardOverlay) -> ForwardState {
    match overlay {
        ForwardOverlay::ForwardWait => ForwardState::Running,
        ForwardOverlay::Done => ForwardState::Succeeded,
        ForwardOverlay::ForwardFailed => ForwardState::Failed,
        ForwardOverlay::ForwardUnknown => ForwardState::Unknown,
    }
}
