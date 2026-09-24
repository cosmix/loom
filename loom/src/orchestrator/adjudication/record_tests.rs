use super::*;
use crate::models::dispute::{request_file, DisputeRequest, DisputeVerdictRecord};
use crate::models::stage::Stage;
use crate::plan::schema::AcceptanceCriterion;
use chrono::Utc;
use std::path::PathBuf;

fn setup(status: StageStatus) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path().to_path_buf();
    std::fs::create_dir_all(work.join("stages")).unwrap();
    let stage = Stage {
        id: "s1".to_string(),
        name: "s1".to_string(),
        status,
        acceptance: vec![AcceptanceCriterion::Simple("cargo test".to_string())],
        ..Stage::default()
    };
    crate::verify::transitions::save_stage(&stage, &work).unwrap();
    (tmp, work)
}

fn write_request(work: &Path, dispute_id: u32) {
    let disputes_root = work.join("disputes");
    std::fs::create_dir_all(disputes_root.join("s1").join(dispute_id.to_string())).unwrap();
    let req = DisputeRequest {
        id: dispute_id,
        stage_id: "s1".to_string(),
        kind: DisputeKind::Criterion { criterion_index: 0 },
        reason: "impossible".to_string(),
        evidence_commit: None,
        failure_output: None,
        fix_attempts_at_dispute: 1,
        created_at: Utc::now(),
    };
    let yaml = serde_yaml::to_string(&req).unwrap();
    std::fs::write(
        request_file(&disputes_root, "s1", dispute_id),
        format!("---\n{yaml}---\n\n# Dispute\n"),
    )
    .unwrap();
}

fn write_json(work: &Path, body: &serde_json::Value) -> PathBuf {
    let path = work.join("verdict.json");
    std::fs::write(&path, body.to_string()).unwrap();
    path
}

fn reject_json() -> serde_json::Value {
    serde_json::json!({
        "verdict": "reject",
        "reasoning": "the criterion is correct",
        "citations": [{"file": "src/a.rs", "excerpt": "fn a", "claim": "exists"}]
    })
}

#[test]
fn records_a_valid_verdict() {
    let (_tmp, work) = setup(StageStatus::NeedsAdjudication);
    write_request(&work, 1);
    let json = write_json(&work, &reject_json());

    assert_eq!(
        record_verdict(&work, "s1", 1, &json, Some("session-test".to_string())).unwrap(),
        AdjudicateOutcome::Recorded
    );

    let path = verdict_file(&work.join("disputes"), "s1", 1);
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("reject"));
    // The record must parse back as the daemon reads it.
    let record: DisputeVerdictRecord =
        serde_yaml::from_str(content.split("---").nth(1).unwrap()).unwrap();
    assert_eq!(record.stage_id, "s1");
    assert_eq!(record.adjudicator_attempt_count, 1);
    assert_eq!(record.session_id.as_deref(), Some("session-test"));
}

#[test]
fn records_a_relayed_verdict_text_under_the_same_guards() {
    let (_tmp, work) = setup(StageStatus::NeedsAdjudication);
    write_request(&work, 1);
    let raw = reject_json().to_string();

    assert_eq!(
        record_verdict_text(&work, "s1", 1, &raw, Some("session-judge".to_string())).unwrap(),
        AdjudicateOutcome::Recorded
    );
    let err = record_verdict_text(&work, "s1", 1, &raw, None).unwrap_err();
    assert!(format!("{err:#}").contains("already been recorded"));
}

#[test]
fn refuses_when_the_stage_is_not_under_adjudication() {
    let (_tmp, work) = setup(StageStatus::Executing);
    write_request(&work, 1);
    let json = write_json(&work, &reject_json());
    let err = record_verdict(&work, "s1", 1, &json, None).unwrap_err();
    assert!(format!("{err:#}").contains("not NeedsAdjudication"));
    assert!(!verdict_file(&work.join("disputes"), "s1", 1).exists());
}

#[test]
fn refuses_a_verdict_for_a_dispute_that_was_never_filed() {
    let (_tmp, work) = setup(StageStatus::NeedsAdjudication);
    let json = write_json(&work, &reject_json());
    let err = record_verdict(&work, "s1", 7, &json, None).unwrap_err();
    assert!(format!("{err:#}").contains("No readable dispute 7"));
}

#[test]
fn refuses_to_replace_a_recorded_verdict() {
    let (_tmp, work) = setup(StageStatus::NeedsAdjudication);
    write_request(&work, 1);
    let json = write_json(&work, &reject_json());
    record_verdict(&work, "s1", 1, &json, None).unwrap();

    let err = record_verdict(&work, "s1", 1, &json, None).unwrap_err();
    assert!(format!("{err:#}").contains("already been recorded"));
}

#[test]
fn degenerate_verdict_escalates_instead_of_recording() {
    let (_tmp, work) = setup(StageStatus::NeedsAdjudication);
    write_request(&work, 1);
    // needs-more-evidence with no questions is the one shape that would
    // loop forever if recorded.
    let json = write_json(
        &work,
        &serde_json::json!({"verdict": "needs-more-evidence", "questions": []}),
    );

    match record_verdict(&work, "s1", 1, &json, None).unwrap() {
        AdjudicateOutcome::Escalated(reason) => assert!(!reason.is_empty()),
        other => panic!("expected escalation, got {other:?}"),
    }
    assert!(!verdict_file(&work.join("disputes"), "s1", 1).exists());
    let after = crate::verify::transitions::load_stage("s1", &work).unwrap();
    assert_eq!(after.status, StageStatus::NeedsHumanReview);
}

#[test]
fn unparseable_output_is_recorded_as_needs_more_evidence() {
    let (_tmp, work) = setup(StageStatus::NeedsAdjudication);
    write_request(&work, 1);
    let path = work.join("verdict.json");
    std::fs::write(&path, "I could not decide.").unwrap();

    assert_eq!(
        record_verdict(&work, "s1", 1, &path, None).unwrap(),
        AdjudicateOutcome::Recorded
    );
    let content = std::fs::read_to_string(verdict_file(&work.join("disputes"), "s1", 1)).unwrap();
    assert!(content.contains("needs-more-evidence"));
}

#[test]
fn a_stage_worktree_session_may_not_record_a_verdict() {
    assert!(refuse_worktree_session(None).is_ok());
    assert!(refuse_worktree_session(Some("  ")).is_ok());
    let err = refuse_worktree_session(Some("/repo/.worktrees/s1")).unwrap_err();
    assert!(format!("{err:#}").contains("may not judge its own disputed criterion"));
}
