//! Typed decoding of a ticket's `payload` field, by [`RequestKind`].

use super::kind::RequestKind;
use crate::fs::memory::MemoryEntry;
use crate::fs::stage_request::StageRequest;
use crate::telemetry::TelemetryEvent;
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
/// daemon-side by `verdict::parse_and_validate_for`.
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
    Ok(request)
}

/// A completion runs every criterion, so it carries no selection.
fn decode_completion(payload: &Value) -> Result<HostJobRequest> {
    let request: HostJobRequest = decode(payload, "complete")?;
    if request.criteria.is_some() {
        bail!("a completion runs every criterion and takes no criteria");
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
    Ok(request)
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
mod tests {
    use super::*;
    use crate::fs::memory::MemoryEntryType;

    fn stage_request_json(request: &str) -> Value {
        match request {
            "block" => serde_json::json!({"request": "block", "reason": "stuck"}),
            "dispute" => serde_json::json!({
                "request": "dispute",
                "field": "acceptance",
                "criterion_indices": [0],
                "reason": "flaky",
            }),
            other => panic!("unexpected request: {other}"),
        }
    }

    #[test]
    fn decodes_a_memory_entry() {
        let entry = MemoryEntry::new(MemoryEntryType::Note, "found something".to_string());
        let payload = serde_json::to_value(&entry).unwrap();
        let decoded = decode_payload(RequestKind::Memory, &payload).unwrap();
        assert_eq!(decoded, RequestPayload::Memory(entry));
    }

    #[test]
    fn decodes_block_and_dispute_and_refuses_the_swapped_kind() {
        let block = stage_request_json("block");
        assert!(matches!(
            decode_payload(RequestKind::Block, &block).unwrap(),
            RequestPayload::Block(StageRequest::Block { .. })
        ));
        assert!(decode_payload(RequestKind::Dispute, &block).is_err());

        let dispute = stage_request_json("dispute");
        assert!(matches!(
            decode_payload(RequestKind::Dispute, &dispute).unwrap(),
            RequestPayload::Dispute(StageRequest::Dispute { .. })
        ));
        assert!(decode_payload(RequestKind::Block, &dispute).is_err());
    }

    #[test]
    fn decodes_handoff_and_verdict() {
        let handoff = serde_json::json!({"trigger": "ceiling"});
        assert_eq!(
            decode_payload(RequestKind::Handoff, &handoff).unwrap(),
            RequestPayload::Handoff(HandoffRequest {
                trigger: "ceiling".to_string(),
                message: None,
            })
        );

        let verdict = serde_json::json!({"dispute_id": 2, "verdict": "uphold"});
        assert_eq!(
            decode_payload(RequestKind::Verdict, &verdict).unwrap(),
            RequestPayload::Verdict(VerdictRequest {
                dispute_id: 2,
                verdict: "uphold".to_string(),
            })
        );
    }

    #[test]
    fn merge_resolved_requires_an_empty_object() {
        assert_eq!(
            decode_payload(RequestKind::MergeResolved, &serde_json::json!({})).unwrap(),
            RequestPayload::MergeResolved
        );
        assert!(decode_payload(RequestKind::MergeResolved, &serde_json::json!({"x": 1})).is_err());
        assert!(decode_payload(RequestKind::MergeResolved, &serde_json::json!([])).is_err());
    }

    #[test]
    fn telemetry_accepts_only_context_pulled() {
        let pulled = serde_json::json!({
            "kind": "context-pulled",
            "stage_id": null,
            "session_id": null,
            "query_chars": 10,
            "budget_tokens": 100,
            "items": 1,
            "estimated_tokens": 20,
            "unmet_required": 0,
        });
        assert!(decode_payload(RequestKind::Telemetry, &pulled).is_ok());

        let delivered = serde_json::json!({
            "kind": "context-delivered",
            "stage_id": "stage-a",
            "session_id": "session-1",
            "context_epoch": "abc",
            "items": 1,
        });
        assert!(decode_payload(RequestKind::Telemetry, &delivered).is_err());
    }

    #[test]
    fn decodes_freeze_contracts_and_refuses_it_under_dispute() {
        let head = "a".repeat(40);
        let freeze = serde_json::json!({
            "request": "freeze_contracts",
            "head": head,
            "tree_digest": "b".repeat(64),
        });
        assert!(matches!(
            decode_payload(RequestKind::FreezeContracts, &freeze).unwrap(),
            RequestPayload::FreezeContracts(StageRequest::FreezeContracts { head: decoded, .. })
                if decoded == head
        ));
        let mut with_reports = freeze.clone();
        with_reports["reports"] = serde_json::json!([]);
        assert!(decode_payload(RequestKind::FreezeContracts, &with_reports).is_err());
        assert!(decode_payload(RequestKind::Dispute, &freeze).is_err());
        assert!(
            decode_payload(RequestKind::FreezeContracts, &stage_request_json("block")).is_err()
        );
    }

    #[test]
    fn decodes_file_dispute_and_refuses_it_under_dispute() {
        let file_dispute = serde_json::json!({
            "request": "file_dispute",
            "kind": {"kind": "contract", "contract_id": "rejects-x"},
            "reason": "the contract tests the wrong error",
        });
        assert!(matches!(
            decode_payload(RequestKind::FileDispute, &file_dispute).unwrap(),
            RequestPayload::FileDispute(StageRequest::FileDispute { .. })
        ));
        assert!(decode_payload(RequestKind::Dispute, &file_dispute).is_err());
        assert!(decode_payload(RequestKind::FileDispute, &stage_request_json("dispute")).is_err());
    }

    #[test]
    fn decodes_a_commit_payload_and_refuses_an_unknown_field() {
        let head = "a".repeat(40);
        let tree = "b".repeat(40);
        let payload = serde_json::json!({
            "message": "feat(relay): commit",
            "expected_head": head,
            "expected_tree": tree,
        });
        assert_eq!(
            decode_payload(RequestKind::Commit, &payload).unwrap(),
            RequestPayload::Commit(CommitPayload {
                message: "feat(relay): commit".to_string(),
                expected_head: head.clone(),
                expected_tree: tree.clone(),
            })
        );

        let extra = serde_json::json!({
            "message": "feat(relay): commit",
            "expected_head": head,
            "expected_tree": tree,
            "amend": true,
        });
        assert!(decode_payload(RequestKind::Commit, &extra).is_err());
        assert!(decode_payload(RequestKind::Commit, &serde_json::json!({})).is_err());
    }

    #[test]
    fn decodes_a_check_payload_and_refuses_an_unknown_field() {
        let payload = serde_json::json!({
            "head": "a".repeat(40),
            "tree_digest": "b".repeat(64),
            "no_cache": true,
        });
        assert!(matches!(
            decode_payload(RequestKind::Check, &payload).unwrap(),
            RequestPayload::Check(HostJobRequest {
                no_cache: true,
                criteria: None,
                ..
            })
        ));
        let mut subset = payload.clone();
        subset["criteria"] = serde_json::json!([1, 3]);
        assert!(matches!(
            decode_payload(RequestKind::Check, &subset).unwrap(),
            RequestPayload::Check(HostJobRequest { criteria: Some(c), .. }) if c == [1, 3]
        ));
        subset["criteria"] = serde_json::json!([]);
        assert!(decode_payload(RequestKind::Check, &subset).is_err());
        let mut extra = payload.clone();
        extra["reports"] = serde_json::json!([]);
        assert!(decode_payload(RequestKind::Check, &extra).is_err());
        assert!(decode_payload(RequestKind::Check, &serde_json::json!({})).is_err());
    }

    #[test]
    fn decodes_a_complete_payload_and_refuses_an_unknown_field() {
        let payload = serde_json::json!({
            "head": "a".repeat(40),
            "tree_digest": "b".repeat(64),
            "no_cache": false,
        });
        assert!(matches!(
            decode_payload(RequestKind::Complete, &payload).unwrap(),
            RequestPayload::Complete(HostJobRequest {
                no_cache: false,
                ..
            })
        ));
        let mut extra = payload.clone();
        extra["evidence"] = serde_json::json!({});
        assert!(decode_payload(RequestKind::Complete, &extra).is_err());
        let mut subset = payload.clone();
        subset["criteria"] = serde_json::json!([0]);
        assert!(decode_payload(RequestKind::Complete, &subset).is_err());
    }

    #[test]
    fn rejects_a_handoff_payload_with_an_unknown_field() {
        let payload = serde_json::json!({"trigger": "ceiling", "extra": true});
        assert!(decode_payload(RequestKind::Handoff, &payload).is_err());
    }
}
