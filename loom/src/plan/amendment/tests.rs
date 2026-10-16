//! Unit tests for amendment-module internals. Integration tests that
//! exercise the full `apply_amendment` flow live under
//! `plan::tests::amendment`.
use super::*;
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
