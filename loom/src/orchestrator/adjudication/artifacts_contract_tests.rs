//! Contract `artifacts-plan-patch-admitted`: a verdict may amend `artifacts`
//! on an artifacts dispute, and only there.
use super::plan_patch::{admits, admitted_fields, normalize};
use crate::models::dispute::CriterionField;
use crate::plan::amendment::{AmendmentField, AmendmentPatch};

#[test]
fn artifacts_plan_patch_admitted() {
    let inner = serde_json::json!({
        "field": "artifacts",
        "op": "replace",
        "index": 0,
        "value": "doc/a.md",
    });
    let (field, patches, _reason) = normalize(&inner).expect("an artifacts plan_patch normalizes");
    assert_eq!(field, AmendmentField::Artifacts);
    assert_eq!(
        patches,
        vec![AmendmentPatch::Replace {
            index: 0,
            value: "doc/a.md".to_string(),
        }]
    );

    assert!(admits(CriterionField::Artifacts, AmendmentField::Artifacts));
    assert!(!admits(
        CriterionField::Acceptance,
        AmendmentField::Artifacts
    ));
    assert!(!admits(
        CriterionField::Artifacts,
        AmendmentField::Acceptance
    ));
    assert_eq!(admitted_fields(CriterionField::Artifacts), "\"artifacts\"");
}
