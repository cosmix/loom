//! Decoding each request kind's payload.

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
    assert!(decode_payload(RequestKind::FreezeContracts, &stage_request_json("block")).is_err());
}

#[test]
fn decodes_file_dispute_and_refuses_it_under_dispute() {
    let file_dispute = serde_json::json!({
        "request": "file_dispute",
        "kind": {"kind": "contract", "contract_ids": ["rejects-x"]},
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

#[test]
fn a_contracts_check_is_bounded_as_a_dispute_replacement() {
    let file = |content: String| serde_json::json!({"file": "tests/t.rs", "content": content});
    let check = |extra: Value| {
        let mut payload = serde_json::json!({"head": "a", "tree_digest": "b", "no_cache": false});
        payload
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        decode_payload(RequestKind::Check, &payload)
    };
    let trial = check(serde_json::json!({"contracts": true, "replacements": [file("x".into())]}));
    match trial.unwrap() {
        RequestPayload::Check(request) => assert!(request.contracts),
        other => panic!("expected a check, got {other:?}"),
    }
    let refused = [
        serde_json::json!({"contracts": true, "criteria": [0]}),
        serde_json::json!({"replacements": [file("x".into())]}),
        serde_json::json!({"contracts": true, "replacements": [file("x".repeat(MAX_REPLACEMENT_BYTES + 1))]}),
        serde_json::json!({"contracts": true, "replacements": [file("x".into()), file("y".into())]}),
        serde_json::json!({"contracts": true, "replacements": [{"file": "../t.rs", "content": "x"}]}),
    ];
    for extra in refused {
        assert!(check(extra.clone()).is_err(), "{extra}");
    }
    let completion =
        serde_json::json!({"head": "a", "tree_digest": "b", "no_cache": false, "contracts": true});
    assert!(decode_payload(RequestKind::Complete, &completion).is_err());
}
