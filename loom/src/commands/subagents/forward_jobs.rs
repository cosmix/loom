//! Read-only forwarding-job evidence for `loom subagents`.

use std::env;
use std::path::{Path, PathBuf};

use crate::models::forward_receipt::job_record::read_companion_job;
use crate::models::forward_receipt::locator::{
    default_companion_state_roots, default_task_output_roots, validate_locator,
};
use crate::models::forward_receipt::{
    load_receipts, receipts_path, ForwardBackend, ForwardIdentity, ForwardReceipt, ForwardState,
};

use super::forward_jobs_transcript::{self, ForwardTranscript};
use super::ledger;

#[path = "forward_jobs_status.rs"]
mod status;
pub(super) use status::*;

pub(super) const FORWARDER_TYPE: &str = "loom-codex-forwarder";

/// Receipt evidence scoped to one Loom stage and, when supplied, Loom session.
pub(super) struct ForwardIndex {
    work_dir: PathBuf,
    pub(super) stage_id: String,
    pub(super) loom_session_id: Option<String>,
    receipts: Vec<ForwardReceipt>,
    unreadable: bool,
    companion_roots: Vec<PathBuf>,
    task_output_roots: Vec<PathBuf>,
}

impl ForwardIndex {
    pub(super) fn receipt_by_id(&self, receipt_id: &str) -> Option<&ForwardReceipt> {
        self.receipts
            .iter()
            .find(|receipt| receipt.receipt_id == receipt_id)
    }
}

/// A forwarded backend's effect on an agent's ordinary transcript state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ForwardOverlay {
    ForwardWait,
    Done,
    ForwardFailed,
    ForwardUnknown,
}

/// Load only this stage's receipt file.  A damaged index is intentionally an
/// unresolved observation instead of a partial success claim.
pub(super) fn load_forward_index(
    work_dir: &Path,
    stage_id: &str,
    loom_session_id: Option<&str>,
) -> ForwardIndex {
    let home = dirs::home_dir().unwrap_or_default();
    let plugin_data = env::var_os("CLAUDE_PLUGIN_DATA").map(PathBuf::from);
    load_index_with_roots(
        work_dir,
        stage_id,
        loom_session_id,
        default_companion_state_roots(&home, plugin_data.as_deref()),
        default_task_output_roots(),
    )
}

pub(super) fn load_index_with_roots(
    work_dir: &Path,
    stage_id: &str,
    loom_session_id: Option<&str>,
    companion_roots: Vec<PathBuf>,
    task_output_roots: Vec<PathBuf>,
) -> ForwardIndex {
    let loaded = receipts_path(work_dir, stage_id)
        .ok()
        .and_then(|path| load_receipts(&path).ok());
    let (receipts, unreadable) = match loaded {
        Some(load) if !load.truncated && load.malformed == 0 => {
            let receipts: Vec<&ForwardReceipt> = match loom_session_id {
                Some(session) => load.for_session(session).collect(),
                None => load.receipts.iter().collect(),
            };
            (
                receipts
                    .into_iter()
                    .filter(|receipt| receipt.identity.stage_id == stage_id)
                    .cloned()
                    .collect(),
                false,
            )
        }
        Some(_) | None => (Vec::new(), true),
    };
    ForwardIndex {
        work_dir: work_dir.to_path_buf(),
        stage_id: stage_id.to_owned(),
        loom_session_id: loom_session_id.map(str::to_owned),
        receipts,
        unreadable,
        companion_roots,
        task_output_roots,
    }
}

/// Overlay one forwarder's lifecycle without modifying the receipt index or
/// the transcript.  `None` means this agent has no forwarding evidence.
pub(super) fn overlay_for_agent(
    index: &ForwardIndex,
    agent_id: &str,
    transcript: &Path,
) -> Option<ForwardOverlay> {
    let transcript = forward_jobs_transcript::read(transcript);
    let known_forwarder =
        ledger::agent_type(Some(&index.work_dir), agent_id).as_deref() == Some(FORWARDER_TYPE);
    let receipts = matching_receipts(index, agent_id, &transcript);
    let expected = known_forwarder || !receipts.is_empty() || transcript.has_forwarding_use();
    if !expected {
        return None;
    }
    if index.unreadable || transcript.unreadable {
        return Some(ForwardOverlay::ForwardUnknown);
    }

    let mut states: Vec<_> = receipts
        .iter()
        .map(|receipt| overlay_from_state(receipt_state(index, receipt), transcript.done))
        .collect();
    for tool in transcript.forwarding_uses() {
        if receipt_for_tool(index, agent_id, &tool.id, transcript.parent_session_id()).is_none() {
            states.push(marker_overlay(index, agent_id, tool, &transcript));
        }
    }
    if states.is_empty() {
        states.push(ForwardOverlay::ForwardUnknown);
    }
    Some(combine_overlays(states))
}

fn receipt_for_tool<'a>(
    index: &'a ForwardIndex,
    agent_id: &str,
    tool_use_id: &str,
    parent_session_id: Option<&str>,
) -> Option<&'a ForwardReceipt> {
    let identity = ForwardIdentity::new(
        parent_session_id?,
        agent_id,
        tool_use_id,
        &index.stage_id,
        index.loom_session_id.as_deref()?,
    )
    .ok()?;
    index
        .receipts
        .iter()
        .find(|receipt| receipt.receipt_id == identity.receipt_id())
}

pub(super) fn matching_receipts<'a>(
    index: &'a ForwardIndex,
    agent_id: &str,
    transcript: &ForwardTranscript,
) -> Vec<&'a ForwardReceipt> {
    let receipts: Vec<_> = index
        .receipts
        .iter()
        .filter(|receipt| receipt.identity.agent_id == agent_id)
        .collect();
    if !transcript.has_forwarding_use() {
        return receipts;
    }
    receipts
        .into_iter()
        .filter(|receipt| {
            transcript.forwarding_uses().any(|tool| {
                receipt_for_tool(index, agent_id, &tool.id, transcript.parent_session_id())
                    .is_some_and(|found| found.receipt_id == receipt.receipt_id)
            })
        })
        .collect()
}

fn marker_overlay(
    index: &ForwardIndex,
    agent_id: &str,
    tool: &super::forward_jobs_transcript::ForwardToolUse,
    transcript: &ForwardTranscript,
) -> ForwardOverlay {
    let marker = transcript.marker_for(tool, &index.task_output_roots);
    let Some(marker) = marker else {
        return ForwardOverlay::ForwardUnknown;
    };
    if !transcript.has_identity(index, agent_id, &tool.id) {
        return ForwardOverlay::ForwardUnknown;
    }
    match marker {
        ForwardState::Queued | ForwardState::Running => ForwardOverlay::ForwardWait,
        ForwardState::Succeeded if transcript.done => ForwardOverlay::Done,
        ForwardState::Succeeded => ForwardOverlay::ForwardWait,
        ForwardState::Failed | ForwardState::Canceled | ForwardState::TimedOut => {
            ForwardOverlay::ForwardFailed
        }
        ForwardState::Unknown => ForwardOverlay::ForwardUnknown,
    }
}

pub(super) fn receipt_state(index: &ForwardIndex, receipt: &ForwardReceipt) -> ForwardState {
    match receipt.backend {
        ForwardBackend::Direct => receipt.state,
        ForwardBackend::Companion => receipt
            .locator
            .as_deref()
            .and_then(|locator| {
                validate_locator(
                    Path::new(locator),
                    &index.companion_roots,
                    &receipt.backend_id,
                )
                .ok()
            })
            .and_then(|locator| read_companion_job(&locator, &receipt.backend_id).ok())
            .map_or(ForwardState::Unknown, |job| job.state()),
    }
}

fn overlay_from_state(state: ForwardState, transcript_done: bool) -> ForwardOverlay {
    match state {
        ForwardState::Queued | ForwardState::Running => ForwardOverlay::ForwardWait,
        ForwardState::Succeeded if transcript_done => ForwardOverlay::Done,
        ForwardState::Succeeded => ForwardOverlay::ForwardWait,
        ForwardState::Failed | ForwardState::Canceled | ForwardState::TimedOut => {
            ForwardOverlay::ForwardFailed
        }
        ForwardState::Unknown => ForwardOverlay::ForwardUnknown,
    }
}

fn combine_overlays(states: Vec<ForwardOverlay>) -> ForwardOverlay {
    if states.contains(&ForwardOverlay::ForwardFailed) {
        ForwardOverlay::ForwardFailed
    } else if states.contains(&ForwardOverlay::ForwardUnknown) {
        ForwardOverlay::ForwardUnknown
    } else if states.contains(&ForwardOverlay::ForwardWait) {
        ForwardOverlay::ForwardWait
    } else {
        ForwardOverlay::Done
    }
}

#[cfg(test)]
#[path = "forward_jobs_tests.rs"]
mod tests;
