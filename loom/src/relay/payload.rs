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
/// daemon-side by `verdict::parse_and_validate`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerdictRequest {
    pub dispute_id: u32,
    pub verdict: String,
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
}

/// Decode `payload` under the shape `kind` promises. `block`, `dispute` and
/// `freeze-contracts` share `StageRequest`'s wire shape, so a payload tagged
/// for another one is refused even though it would otherwise parse.
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
    }
}

fn decode<T: for<'de> Deserialize<'de>>(payload: &Value, kind: &str) -> Result<T> {
    serde_json::from_value(payload.clone())
        .with_context(|| format!("{kind} request payload did not match its expected shape"))
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
                "criterion_index": 0,
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
        let freeze = serde_json::json!({
            "request": "freeze_contracts",
            "reports": [{
                "contract_id": "rejects-x",
                "adapter": "cargo-test",
                "outcome": "failed",
                "exit_code": 101,
            }],
        });
        assert!(matches!(
            decode_payload(RequestKind::FreezeContracts, &freeze).unwrap(),
            RequestPayload::FreezeContracts(StageRequest::FreezeContracts { reports })
                if reports.len() == 1
        ));
        assert!(decode_payload(RequestKind::Dispute, &freeze).is_err());
        assert!(
            decode_payload(RequestKind::FreezeContracts, &stage_request_json("block")).is_err()
        );
    }

    #[test]
    fn rejects_a_handoff_payload_with_an_unknown_field() {
        let payload = serde_json::json!({"trigger": "ceiling", "extra": true});
        assert!(decode_payload(RequestKind::Handoff, &payload).is_err());
    }
}
