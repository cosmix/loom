//! Contract `criterion-verdict-patches-only-filed-entries`: an accepted
//! criterion verdict may replace or delete only the entries its dispute
//! filed; an insert adds a new entry and is always admitted.
use crate::models::dispute::{CriterionField, DisputeKind, DisputeVerdict};
use crate::orchestrator::adjudication::verdict::{parse_and_validate_for, ValidationOutcome};
use serde_json::{json, Value};

/// Whether an accept carrying `patches` on `list` is accepted for a dispute
/// filed against entry 0 of `field`'s list.
fn accepted(field: CriterionField, list: &str, patches: Value) -> bool {
    let raw = json!({
        "verdict": "accept",
        "reasoning": "the disputed entry is wrong",
        "citations": [
            {"file": "src/a.rs", "line": 10, "excerpt": "fn foo()", "claim": "missing function"}
        ],
        "plan_patch": {"field": list, "patches": patches, "reason": "entry was wrong"},
    })
    .to_string();
    let kind = DisputeKind::Criterion {
        criterion_indices: vec![0],
        field,
        owner_stage: None,
    };
    matches!(
        parse_and_validate_for(&raw, &kind),
        ValidationOutcome::Verdict(DisputeVerdict::Accept { .. })
    )
}

#[test]
fn criterion_verdict_patches_only_filed_entries() {
    let cases = [
        (
            CriterionField::Artifacts,
            "artifacts",
            "doc/a.md",
            "doc/b.md",
        ),
        (
            CriterionField::Acceptance,
            "acceptance",
            "cargo test --lib",
            "cargo test --doc",
        ),
    ];
    for (field, list, replacement, inserted) in cases {
        assert!(
            !accepted(field, list, json!([{"op": "delete", "index": 1}])),
            "{list}: a delete of an unfiled index is not accepted"
        );
        assert!(
            accepted(
                field,
                list,
                json!([{"op": "replace", "index": 0, "value": replacement}])
            ),
            "{list}: a replace of the filed index is accepted"
        );
        assert!(
            accepted(
                field,
                list,
                json!([{"op": "insert", "index": 2, "value": inserted}])
            ),
            "{list}: an insert is accepted"
        );
    }
}
