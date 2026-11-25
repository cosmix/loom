//! The `artifacts` amendment field: its audit name, how it reaches the runtime
//! stage record, and the unmerged-stage rule it shares with `harness`.
use super::*;
use crate::models::stage::Stage;
use crate::plan::amendment_fields::{stage_field_matches, sync_stage_from_definition};
use crate::plan::schema::StageDefinition;

#[test]
fn artifacts_audit_row_reads_back() {
    let row = AuditRow {
        version: 1,
        stage_id: "s".to_string(),
        field: AmendmentField::Artifacts,
        patch_op: "replace",
        index: 0,
        applied_at: Utc::now(),
        dispute_id: None,
        reason: None,
    };
    assert_eq!(row.field_name(), "artifacts");
    let line = row.to_markdown_row();
    let cells: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
    assert_eq!(cells[3], "artifacts");
    assert_eq!(field_from_name(cells[3]), Some(AmendmentField::Artifacts));
}

#[test]
fn an_artifacts_amendment_clears_the_commit_of_an_unmerged_stage_only() {
    let def = StageDefinition {
        artifacts: vec!["docs/new.md".to_string()],
        ..StageDefinition::default()
    };
    let mut merged = Stage::new("S".to_string(), None);
    merged.completed_commit = Some("abc123".to_string());
    merged.merged = true;
    sync_stage_from_definition(&mut merged, &def, AmendmentField::Artifacts);
    assert_eq!(merged.completed_commit.as_deref(), Some("abc123"));
    assert_eq!(merged.artifacts, vec!["docs/new.md"]);

    let mut unmerged = Stage::new("S".to_string(), None);
    unmerged.completed_commit = Some("abc123".to_string());
    sync_stage_from_definition(&mut unmerged, &def, AmendmentField::Artifacts);
    assert_eq!(unmerged.completed_commit, None);
    assert_eq!(unmerged.artifacts, vec!["docs/new.md"]);
}

#[test]
fn artifacts_sync_and_match_leave_harness_alone() {
    let def = StageDefinition {
        artifacts: vec!["docs/new.md".to_string()],
        harness: vec!["tests/def".to_string()],
        ..StageDefinition::default()
    };
    let mut stage = Stage::new("S".to_string(), None);
    stage.harness = vec!["tests/stage".to_string()];
    assert!(!stage_field_matches(
        &stage,
        &def,
        AmendmentField::Artifacts
    ));

    sync_stage_from_definition(&mut stage, &def, AmendmentField::Artifacts);

    assert!(stage_field_matches(&stage, &def, AmendmentField::Artifacts));
    assert_eq!(stage.artifacts, vec!["docs/new.md"]);
    assert_eq!(stage.harness, vec!["tests/stage"]);
}
