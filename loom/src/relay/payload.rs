//! Typed decoding of a ticket's `payload` field, by [`RequestKind`].

use super::kind::RequestKind;
use crate::fs::memory::MemoryEntry;
use crate::fs::stage_request::StageRequest;
use crate::models::dispute::{FileReplacement, MAX_REPLACEMENT_BYTES};
use crate::plan::schema::ContractChoices;
use crate::telemetry::TelemetryEvent;
use crate::validation::validate_id;
use crate::verify::contracts::choices::MAX_CHOICE_IDS;
use crate::verify::contracts::store::validate_relative;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::Value;

/// `{trigger, message}` for a `handoff` request. The daemon builds the actual
/// handoff document from the session's own state; this only carries the
/// trigger and optional note the CLI observed.
#[derive(Debug, Clone, PartialEq, serde::Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandoffRequest {
    pub trigger: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// `{dispute_id, verdict}` for a `verdict` request. Validated again
/// daemon-side by the stage-aware `verdict::parse_and_validate_in`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerdictRequest {
    pub dispute_id: u32,
    pub verdict: String,
}

/// `{message, expected_head, expected_tree}` for a `commit` request: the
/// message the session's hooks accepted, and the HEAD and staged tree it saw.
/// The daemon commits that tree only while both still hold
/// (`git::stage_commit`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommitPayload {
    pub message: String,
    pub expected_head: String,
    pub expected_tree: String,
}

/// `{head, tree_digest, no_cache, criteria?}` for a host job request (`check`
/// and `complete`): the HEAD and tree digest the session saw, whether it asked
/// to bypass the acceptance cache, and for a check the 0-based criteria
/// `loom check --criterion` selected. The child re-checks the tree on the host.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostJobRequest {
    pub head: String,
    pub tree_digest: String,
    pub no_cache: bool,
    /// Absent for every criterion; a completion never carries one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub criteria: Option<Vec<usize>>,
    /// A check of the stage's frozen contracts instead of its acceptance
    /// criteria (`loom check --contracts`); only a check sets it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub contracts: bool,
    /// The replacement files a contracts check trials over frozen files,
    /// each bounded as a dispute's replacement.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replacements: Vec<FileReplacement>,
}

/// A ticket payload, decoded to the type its `kind` promises.
#[derive(Debug, Clone, PartialEq)]
pub enum RequestPayload {
    Memory(MemoryEntry),
    Block(StageRequest),
    Dispute(StageRequest),
    Handoff(HandoffRequest),
    MergeResolved,
    Verdict(VerdictRequest),
    Telemetry(TelemetryEvent),
    FreezeContracts(StageRequest),
    FileDispute(StageRequest),
    Commit(CommitPayload),
    Check(HostJobRequest),
    Complete(HostJobRequest),
    WorkerControl(super::payload_native::WorkerControlPayload),
}

/// Decode `payload` under the shape `kind` promises. `block`, `dispute`,
/// `freeze-contracts` and `file-dispute` share `StageRequest`'s wire shape, so
/// a payload tagged for another one is refused even though it would otherwise
/// parse.
pub fn decode_payload(kind: RequestKind, payload: &Value) -> Result<RequestPayload> {
    match kind {
        RequestKind::Memory => Ok(RequestPayload::Memory(decode(payload, "memory")?)),
        RequestKind::Block => Ok(RequestPayload::Block(decode_stage_request(
            payload, "block",
        )?)),
        RequestKind::Dispute => Ok(RequestPayload::Dispute(decode_stage_request(
            payload, "dispute",
        )?)),
        RequestKind::Handoff => Ok(RequestPayload::Handoff(decode(payload, "handoff")?)),
        RequestKind::MergeResolved => {
            decode_empty_object(payload)?;
            Ok(RequestPayload::MergeResolved)
        }
        RequestKind::Verdict => Ok(RequestPayload::Verdict(decode(payload, "verdict")?)),
        RequestKind::Telemetry => Ok(RequestPayload::Telemetry(decode_telemetry(payload)?)),
        RequestKind::FreezeContracts => Ok(RequestPayload::FreezeContracts(decode_stage_request(
            payload,
            "freeze_contracts",
        )?)),
        RequestKind::FileDispute => Ok(RequestPayload::FileDispute(decode_stage_request(
            payload,
            "file_dispute",
        )?)),
        RequestKind::Commit => Ok(RequestPayload::Commit(decode(payload, "commit")?)),
        RequestKind::Check => Ok(RequestPayload::Check(decode_check(payload)?)),
        RequestKind::Complete => Ok(RequestPayload::Complete(decode_completion(payload)?)),
        RequestKind::WorkerControl => decode(payload, "worker").map(RequestPayload::WorkerControl),
    }
}

fn decode<T: for<'de> Deserialize<'de>>(payload: &Value, kind: &str) -> Result<T> {
    serde_json::from_value(payload.clone())
        .with_context(|| format!("{kind} request payload did not match its expected shape"))
}

/// A check's selection names at least one criterion: an empty one would pass
/// having run nothing.
fn decode_check(payload: &Value) -> Result<HostJobRequest> {
    let request: HostJobRequest = decode(payload, "check")?;
    if request.criteria.as_ref().is_some_and(Vec::is_empty) {
        bail!("a check request's criteria must name at least one criterion");
    }
    if request.contracts && request.criteria.is_some() {
        bail!("a contracts check runs no acceptance criteria");
    }
    if !request.contracts && !request.replacements.is_empty() {
        bail!("only a contracts check trials replacement files");
    }
    bound_replacements(&request.replacements)?;
    Ok(request)
}

/// Each replacement names a distinct plain relative frozen file and holds at
/// most [`MAX_REPLACEMENT_BYTES`], as a dispute's replacement does.
fn bound_replacements(replacements: &[FileReplacement]) -> Result<()> {
    for (index, replacement) in replacements.iter().enumerate() {
        validate_relative(&replacement.file)?;
        if replacement.content.len() > MAX_REPLACEMENT_BYTES {
            bail!(
                "the replacement of '{}' exceeds the {MAX_REPLACEMENT_BYTES}-byte limit",
                replacement.file
            );
        }
        if replacement.base_sha256.is_some() {
            bail!("a trial's replacement carries no base_sha256");
        }
        if replacements[..index]
            .iter()
            .any(|r| r.file == replacement.file)
        {
            bail!("the frozen file '{}' is replaced twice", replacement.file);
        }
    }
    Ok(())
}

/// A completion runs every criterion, so it carries no selection.
fn decode_completion(payload: &Value) -> Result<HostJobRequest> {
    let request: HostJobRequest = decode(payload, "complete")?;
    if request.criteria.is_some() {
        bail!("a completion runs every criterion and takes no criteria");
    }
    if request.contracts || !request.replacements.is_empty() {
        bail!("a completion runs every check and takes no contracts selection");
    }
    Ok(request)
}

fn decode_stage_request(payload: &Value, expected: &str) -> Result<StageRequest> {
    let request: StageRequest = decode(payload, expected)?;
    if request.kind() != expected {
        bail!(
            "expected a {expected} request, found a {} request",
            request.kind()
        );
    }
    if let StageRequest::FreezeContracts { choices, .. } = &request {
        bound_choices(choices)?;
    }
    Ok(request)
}

/// The cheap wire bound on a freeze's choices; the daemon checks the ids
/// against the stage's contracts again.
fn bound_choices(choices: &ContractChoices) -> Result<()> {
    if choices.len() > MAX_CHOICE_IDS {
        bail!("choices keys must be contract ids: at most {MAX_CHOICE_IDS} entries are allowed");
    }
    if choices.keys().any(|id| validate_id(id).is_err()) {
        bail!("choices keys must be contract ids");
    }
    Ok(())
}

fn decode_empty_object(payload: &Value) -> Result<()> {
    match payload.as_object() {
        Some(object) if object.is_empty() => Ok(()),
        Some(_) => bail!("merge-resolved request payload must be an empty object"),
        None => bail!("merge-resolved request payload must be an object"),
    }
}

fn decode_telemetry(payload: &Value) -> Result<TelemetryEvent> {
    let event: TelemetryEvent = decode(payload, "telemetry")?;
    if !matches!(event, TelemetryEvent::ContextPulled { .. }) {
        bail!("telemetry request payload must be a context-pulled event");
    }
    Ok(event)
}

#[cfg(test)]
#[path = "payload_tests.rs"]
mod tests;
