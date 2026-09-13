//! Summary-facing views of a stage-scoped forwarding receipt index.

use std::collections::BTreeSet;
use std::path::Path;

use serde::Serialize;

use super::forward_jobs_transcript;
use super::{
    combine_overlays, matching_receipts, overlay_for_agent, overlay_from_state, receipt_state,
    ForwardIndex, ForwardOverlay, FORWARDER_TYPE,
};
use crate::commands::subagents::ledger;
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
    transcript_path: &Path,
) -> Option<ForwardEvidence> {
    let overlay = overlay_for_agent(index, agent_id, transcript_path)?;
    let transcript = forward_jobs_transcript::read(transcript_path);
    let receipts = matching_receipts(index, agent_id, &transcript);
    let expected =
        known_forwarder(index, agent_id) || !receipts.is_empty() || transcript.has_forwarding_use();
    expected.then(|| ForwardEvidence {
        overlay,
        metadata: receipt_metadata(index, &receipts, overlay),
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
    let overlay = if index.unreadable {
        ForwardOverlay::ForwardUnknown
    } else {
        combine_overlays(
            receipts
                .iter()
                .map(|receipt| overlay_from_state(receipt_state(index, receipt), false))
                .collect(),
        )
    };
    Some(ForwardEvidence {
        overlay,
        metadata: receipt_metadata(index, &receipts, overlay),
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

fn known_forwarder(index: &ForwardIndex, agent_id: &str) -> bool {
    ledger::agent_type(Some(&index.work_dir), agent_id).as_deref() == Some(FORWARDER_TYPE)
}

fn receipt_metadata(
    index: &ForwardIndex,
    receipts: &[&ForwardReceipt],
    overlay: ForwardOverlay,
) -> ForwardMetadata {
    if !index.unreadable && receipts.len() == 1 {
        let receipt = receipts[0];
        return ForwardMetadata {
            receipt_id: Some(receipt.receipt_id.clone()),
            backend_id: Some(receipt.backend_id.clone()),
            state: receipt_state(index, receipt),
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
