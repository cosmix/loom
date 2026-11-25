//! Contract `artifacts-amend-takes-a-verbatim-glob`: an `artifacts`
//! amendment takes a plain glob verbatim (trimmed, never parsed as YAML),
//! refuses an empty value or an invalid glob, patches only `artifacts`, and
//! its audit name round-trips.
use super::ParsedAmendmentValue;
use super::{field_from_name, AmendmentField, AmendmentPatch, AmendmentRequest};
use crate::plan::amendment_fields::{apply_patch_to_stage_def, parse_amendment_value};
use crate::plan::schema::StageDefinition;

fn replace(index: usize, value: &str) -> AmendmentRequest {
    AmendmentRequest {
        stage_id: "s1".to_string(),
        field: AmendmentField::Artifacts,
        patch: AmendmentPatch::Replace {
            index,
            value: value.to_string(),
        },
        reason: None,
        dispute_id: None,
    }
}

#[test]
fn artifacts_amend_takes_a_verbatim_glob() {
    let request = replace(0, "  docs/*.md  ");
    let parsed = parse_amendment_value(&request).expect("a plain glob parses");
    assert!(
        matches!(&parsed, ParsedAmendmentValue::Artifacts(g) if g == "docs/*.md"),
        "the glob is taken verbatim and trimmed"
    );
    assert!(
        parse_amendment_value(&replace(0, "")).is_err(),
        "an empty value is refused"
    );
    assert!(
        parse_amendment_value(&replace(0, "docs/[x.md")).is_err(),
        "an invalid glob is refused"
    );

    let mut stage = StageDefinition {
        artifacts: vec!["old.md".to_string(), "keep.md".to_string()],
        ..Default::default()
    };
    apply_patch_to_stage_def(
        &mut stage,
        AmendmentField::Artifacts,
        &request.patch,
        &parsed,
    )
    .expect("replace at index 0 applies");
    assert_eq!(stage.artifacts, ["docs/*.md", "keep.md"]);
    assert!(stage.harness.is_empty(), "no other list is written");

    let out_of_range = replace(2, "docs/*.md");
    assert!(
        apply_patch_to_stage_def(
            &mut stage,
            AmendmentField::Artifacts,
            &out_of_range.patch,
            &parsed
        )
        .is_err(),
        "a replace past the end is refused"
    );
    assert_eq!(
        field_from_name("artifacts"),
        Some(AmendmentField::Artifacts),
        "the audit name round-trips"
    );
}
