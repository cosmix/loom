use super::*;
use serde_json::json;

fn plan_patch(v: Value) -> PlanPatch {
    PlanPatch { inner: v }
}

/// The one request `inner` builds.
fn single(inner: Value) -> AmendmentRequest {
    let mut requests = build_amendment_requests("s1", &plan_patch(inner), 1).unwrap();
    assert_eq!(requests.len(), 1);
    requests.remove(0)
}

#[test]
fn nested_shape_decodes() {
    let req = single(json!({
        "field": "acceptance",
        "patch": {"op": "delete", "index": 0},
        "reason": "wrong",
    }));
    assert_eq!(req.field, AmendmentField::Acceptance);
    assert_eq!(req.patch, AmendmentPatch::Delete { index: 0 });
    assert_eq!(req.reason.as_deref(), Some("wrong"));
    assert_eq!(req.dispute_id.as_deref(), Some("1.0"));
}

#[test]
fn flat_shape_decodes_to_identical_request() {
    let nested = single(json!({
        "field": "acceptance",
        "patch": {"op": "replace", "index": 2, "value": "loom knowledge check"},
        "reason": "criterion wrong",
    }));
    let flat = single(json!({
        "stage_id": "s1",
        "field": "acceptance",
        "op": "replace",
        "index": 2,
        "value": "loom knowledge check",
        "reason": "criterion wrong",
    }));
    assert_eq!(nested.stage_id, flat.stage_id);
    assert_eq!(nested.field, flat.field);
    assert_eq!(nested.patch, flat.patch);
    assert_eq!(nested.reason, flat.reason);
}

#[test]
fn flat_delete_without_value_decodes() {
    let req = single(json!({"field": "wiring", "op": "delete", "index": 3}));
    assert_eq!(req.field, AmendmentField::Wiring);
    assert_eq!(req.patch, AmendmentPatch::Delete { index: 3 });
}

#[test]
fn patches_apply_by_descending_index_with_one_dispute_id_each() {
    let inner = json!({
        "field": "acceptance",
        "patches": [
            {"op": "replace", "index": 13, "value": "cargo test --lib"},
            {"op": "delete", "index": 14},
        ],
        "reason": "both run only outside the sandbox",
    });
    let requests = build_amendment_requests("s1", &plan_patch(inner), 7).unwrap();

    let order: Vec<(usize, Option<&str>)> = requests
        .iter()
        .map(|r| (r.patch.index(), r.dispute_id.as_deref()))
        .collect();
    assert_eq!(order, vec![(14, Some("7.1")), (13, Some("7.0"))]);
    assert!(requests
        .iter()
        .all(|r| r.reason.as_deref() == Some("both run only outside the sandbox")));
}

#[test]
fn an_empty_or_malformed_patches_list_errors() {
    for (patches, expected) in [
        (json!([]), "is empty"),
        (json!({"op": "delete"}), "must be an array"),
        (
            json!([{"op": "delete", "index": 0}, {"op": "explode"}]),
            "patches[1]",
        ),
    ] {
        let inner = json!({"field": "acceptance", "patches": patches});
        let err = normalize(&inner).unwrap_err();
        assert!(err.contains(expected), "{expected}: {err}");
    }
}

#[test]
fn duplicate_index_names_the_repeated_index() {
    let patches = [
        AmendmentPatch::Delete { index: 14 },
        AmendmentPatch::Delete { index: 13 },
        AmendmentPatch::Replace {
            index: 14,
            value: "x".to_string(),
        },
    ];
    assert_eq!(duplicate_index(&patches), Some(14));
    assert_eq!(duplicate_index(&patches[..2]), None);
}

#[test]
fn neither_shape_present_errors_naming_both() {
    let err = normalize(&json!({"field": "acceptance"})).unwrap_err();
    assert!(err.contains("patches"), "error: {err}");
    assert!(err.contains("op"), "error: {err}");
}

#[test]
fn unknown_field_errors() {
    let inner = json!({"field": "bogus", "patch": {"op": "delete", "index": 0}});
    let err = normalize(&inner).unwrap_err();
    assert!(err.contains("bogus"), "error: {err}");
}

#[test]
fn wiring_tests_field_decodes_in_both_spellings() {
    for spelling in ["wiring-tests", "wiring_tests"] {
        let inner = json!({"field": spelling, "patch": {"op": "delete", "index": 0}});
        let (field, _, _) = normalize(&inner).unwrap();
        assert_eq!(field, AmendmentField::WiringTests, "spelling {spelling}");
        let canonical =
            canonical_patches_inner(field, &[AmendmentPatch::Delete { index: 0 }], None);
        assert_eq!(canonical["field"], "wiring-tests");
    }
}

#[test]
fn canonical_patches_inner_round_trips_through_normalize() {
    let field = AmendmentField::Acceptance;
    let patches = vec![
        AmendmentPatch::Replace {
            index: 1,
            value: "cargo test".to_string(),
        },
        AmendmentPatch::Delete { index: 2 },
    ];
    let inner = canonical_patches_inner(field, &patches, Some("because"));
    assert!(inner.get("patch").is_none(), "{inner}");
    let (out_field, out_patches, out_reason) = normalize(&inner).unwrap();
    assert_eq!(out_field, field);
    assert_eq!(out_patches, patches);
    assert_eq!(out_reason.as_deref(), Some("because"));
}
