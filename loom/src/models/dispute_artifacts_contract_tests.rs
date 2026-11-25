//! Contract `artifacts-dispute-field-maps-end-to-end`: the artifacts dispute
//! field spells `artifacts`, labels gaps `artifact`, amends the artifacts
//! list, and round-trips through serde.
use crate::models::dispute::CriterionField;
use crate::plan::amendment::AmendmentField;

#[test]
fn artifacts_dispute_field_maps_end_to_end() {
    let field = CriterionField::Artifacts;
    assert_eq!(field.as_str(), "artifacts");
    assert_eq!(field.gap_label(), "artifact");
    assert_eq!(field.amendment_field(), AmendmentField::Artifacts);

    let json = serde_json::to_string(&field).expect("serializes");
    assert_eq!(json, "\"artifacts\"");
    let back: CriterionField = serde_json::from_str(&json).expect("deserializes");
    assert_eq!(back, CriterionField::Artifacts);
}
