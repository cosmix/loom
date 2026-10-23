//! Unit tests for amendment-module internals. Integration tests that
//! exercise the full `apply_amendment` flow live under
//! `plan::tests::amendment`.
use super::*;
use crate::models::stage::Stage;
use crate::plan::amendment_fields::{
    apply_patch_to_stage_def, current_field_len, parse_amendment_value, sync_stage_from_definition,
};
use crate::plan::schema::StageDefinition;

#[test]
fn audit_row_round_trip_via_markdown() {
    let row = AuditRow {
        version: 7,
        stage_id: "feature-x".to_string(),
        field: AmendmentField::Acceptance,
        patch_op: "replace",
        index: 2,
        applied_at: DateTime::parse_from_rfc3339("2026-05-13T07:30:00+00:00")
            .unwrap()
            .with_timezone(&Utc),
        dispute_id: Some("d-42".to_string()),
        reason: Some("agent demonstrated env mismatch".to_string()),
    };
    let md = format!("{}{}", AUDIT_HEADER, row.to_markdown_row());
    // Parse back through the same path read_audit_rows uses by
    // splitting on '|' and trimming.
    let last_line = md.lines().last().unwrap();
    let cells: Vec<&str> = last_line.split('|').map(|s| s.trim()).collect();
    assert_eq!(cells[1], "7");
    assert_eq!(cells[2], "feature-x");
    assert_eq!(cells[3], "acceptance");
    assert_eq!(cells[4], "replace");
    assert_eq!(cells[5], "2");
    assert_eq!(cells[7], "d-42");
    assert_eq!(cells[8], "agent demonstrated env mismatch");
}

#[test]
fn audit_row_sanitises_pipe_and_newline_in_reason() {
    let row = AuditRow {
        version: 1,
        stage_id: "s".to_string(),
        field: AmendmentField::Wiring,
        patch_op: "delete",
        index: 0,
        applied_at: Utc::now(),
        dispute_id: None,
        reason: Some("contains | a pipe and\nnewline".to_string()),
    };
    let line = row.to_markdown_row();
    // Exactly one row-terminator newline, no embedded newlines.
    assert_eq!(line.matches('\n').count(), 1);
    // Pipes inside the reason cell have been replaced with '/' so the
    // table structure is preserved; the row parses to exactly the
    // documented number of cells.
    let cells: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
    assert_eq!(cells.len(), 10, "got {cells:?}");
    assert_eq!(cells[8], "contains / a pipe and newline");
}

#[test]
fn current_field_len_reads_definition() {
    let mut def = StageDefinition {
        id: "s".to_string(),
        name: "S".to_string(),
        acceptance: vec![
            AcceptanceCriterion::Simple("cargo test".to_string()),
            AcceptanceCriterion::Simple("cargo clippy".to_string()),
        ],
        working_dir: ".".to_string(),
        ..Default::default()
    };
    assert_eq!(current_field_len(&def, AmendmentField::Acceptance), 2);
    assert_eq!(current_field_len(&def, AmendmentField::Wiring), 0);
    def.wiring.push(WiringCheck {
        source: "x".to_string(),
        pattern: "y".to_string(),
        description: "z".to_string(),
        literal: false,
    });
    assert_eq!(current_field_len(&def, AmendmentField::Wiring), 1);
}

#[test]
fn apply_patch_vec_bounds_check_on_replace() {
    let mut v: Vec<i32> = vec![1, 2, 3];
    let p = AmendmentPatch::Replace {
        index: 5,
        value: String::new(),
    };
    assert!(crate::plan::amendment_fields::apply_patch_vec(&mut v, &p, Some(99)).is_err());
}

#[test]
fn apply_patch_vec_insert_at_end_is_ok() {
    let mut v: Vec<i32> = vec![1, 2, 3];
    let p = AmendmentPatch::Insert {
        index: 3,
        value: String::new(),
    };
    crate::plan::amendment_fields::apply_patch_vec(&mut v, &p, Some(4)).unwrap();
    assert_eq!(v, vec![1, 2, 3, 4]);
}

#[test]
fn apply_patch_vec_delete_shifts() {
    let mut v: Vec<i32> = vec![1, 2, 3];
    let p = AmendmentPatch::Delete { index: 1 };
    crate::plan::amendment_fields::apply_patch_vec::<i32>(&mut v, &p, None).unwrap();
    assert_eq!(v, vec![1, 3]);
}

fn harness_request(patch: AmendmentPatch) -> AmendmentRequest {
    AmendmentRequest {
        stage_id: "s".to_string(),
        field: AmendmentField::Harness,
        patch,
        reason: None,
        dispute_id: None,
    }
}

/// Parse, apply to the definition, then copy to a stage: the path `apply_amendment` takes.
fn amend_harness(def: &mut StageDefinition, patch: AmendmentPatch) -> Result<Stage> {
    let request = harness_request(patch);
    let value = parse_amendment_value(&request)?;
    apply_patch_to_stage_def(def, AmendmentField::Harness, &request.patch, &value)?;
    let mut stage = Stage::new("S".to_string(), None);
    sync_stage_from_definition(&mut stage, def, AmendmentField::Harness);
    Ok(stage)
}

#[test]
fn harness_insert_replace_delete_reach_the_stage_record() {
    let mut def = StageDefinition::default();
    let insert = |index, value: &str| AmendmentPatch::Insert {
        index,
        value: value.to_string(),
    };
    amend_harness(&mut def, insert(0, "tests/a")).unwrap();
    let stage = amend_harness(&mut def, insert(1, "tests/b")).unwrap();
    assert_eq!(stage.harness, vec!["tests/a", "tests/b"]);
    assert_eq!(current_field_len(&def, AmendmentField::Harness), 2);

    let replace = AmendmentPatch::Replace {
        index: 0,
        value: "tests/c".to_string(),
    };
    let stage = amend_harness(&mut def, replace).unwrap();
    assert_eq!(stage.harness, vec!["tests/c", "tests/b"]);
    assert_eq!(def.harness, stage.harness);

    let stage = amend_harness(&mut def, AmendmentPatch::Delete { index: 1 }).unwrap();
    assert_eq!(stage.harness, vec!["tests/c"]);
}

#[test]
fn harness_globs_round_trip_verbatim() {
    for glob in ["**/fixtures/*.json", "tests/x #1"] {
        let mut def = StageDefinition::default();
        let patch = AmendmentPatch::Insert {
            index: 0,
            value: format!("  {glob}  "),
        };
        let stage = amend_harness(&mut def, patch).unwrap();
        assert_eq!(stage.harness, vec![glob.to_string()]);
    }
}

#[test]
fn harness_empty_value_is_refused() {
    let mut def = StageDefinition::default();
    let patch = AmendmentPatch::Insert {
        index: 0,
        value: "   ".to_string(),
    };
    assert!(amend_harness(&mut def, patch).is_err());
    assert!(def.harness.is_empty());
}

#[test]
fn harness_parent_dir_component_fails_plan_validation() {
    let mut def = StageDefinition {
        id: "s".to_string(),
        name: "S".to_string(),
        working_dir: ".".to_string(),
        ..Default::default()
    };
    let patch = AmendmentPatch::Insert {
        index: 0,
        value: "../outside/**".to_string(),
    };
    amend_harness(&mut def, patch).unwrap();
    let metadata = LoomMetadata {
        loom: crate::plan::schema::LoomConfig {
            version: 2,
            stages: vec![def],
            ..Default::default()
        },
    };
    let errors = crate::plan::schema::validate(&metadata).expect_err("`..` must be refused");
    assert!(
        errors.iter().any(|e| e.message.contains("harness entry")),
        "got {errors:?}"
    );
}

#[test]
fn harness_audit_row_reads_back() {
    let row = AuditRow {
        version: 1,
        stage_id: "s".to_string(),
        field: AmendmentField::Harness,
        patch_op: "insert",
        index: 0,
        applied_at: Utc::now(),
        dispute_id: None,
        reason: None,
    };
    let line = row.to_markdown_row();
    let cells: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
    assert_eq!(cells[3], "harness");
    assert_eq!(field_from_name(cells[3]), Some(AmendmentField::Harness));
}
