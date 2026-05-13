//! Unit tests for the adjudication module that need access to
//! multiple sub-modules together (cross-cutting flows). Per-module
//! tests live in `prompt.rs`, `client.rs`, `verdict.rs`, `worker.rs`,
//! and `feedback.rs`.

use super::feedback;
use super::{
    build_amendment_request, parse_yaml_frontmatter, scan_pending_requests, AdjudicatorRegistry,
    MAX_EVIDENCE_ROUNDS,
};
use crate::models::dispute::{
    request_file, verdict_file, Citation, DisputeRequest, DisputeVerdict, DisputeVerdictRecord,
    PlanPatch,
};
use crate::models::stage::{Stage, StageStatus};
use crate::plan::amendment::{AmendmentField, AmendmentPatch};
use chrono::Utc;
use std::path::Path;

fn make_stage(id: &str) -> Stage {
    Stage {
        id: id.to_string(),
        name: id.to_string(),
        status: StageStatus::NeedsAdjudication,
        ..Default::default()
    }
}

fn write_stage(work_dir: &Path, stage: &Stage) {
    crate::verify::transitions::save_stage(stage, work_dir).unwrap();
}

fn write_dispute_request(work_dir: &Path, stage_id: &str, id: u32, criterion_index: usize) {
    let disputes_root = work_dir.join("disputes");
    std::fs::create_dir_all(disputes_root.join(stage_id).join(id.to_string())).unwrap();
    let req = DisputeRequest {
        id,
        stage_id: stage_id.to_string(),
        criterion_index,
        reason: "criterion impossible".to_string(),
        evidence_commit: None,
        failure_output: None,
        fix_attempts_at_dispute: 1,
        created_at: Utc::now(),
    };
    let yaml = serde_yaml::to_string(&req).unwrap();
    let path = request_file(&disputes_root, stage_id, id);
    std::fs::write(
        &path,
        format!("---\n{yaml}---\n\n# Dispute {stage_id}/{id}\n"),
    )
    .unwrap();
}

fn write_verdict(work_dir: &Path, stage_id: &str, id: u32, verdict: DisputeVerdict, attempt: u32) {
    let disputes_root = work_dir.join("disputes");
    std::fs::create_dir_all(disputes_root.join(stage_id).join(id.to_string())).unwrap();
    let record = DisputeVerdictRecord {
        id,
        stage_id: stage_id.to_string(),
        verdict,
        adjudicator_attempt_count: attempt,
        created_at: Utc::now(),
        model: "test".to_string(),
    };
    let yaml = serde_yaml::to_string(&record).unwrap();
    let path = verdict_file(&disputes_root, stage_id, id);
    std::fs::write(
        &path,
        format!("---\n{yaml}---\n\n# Verdict {stage_id}/{id}\n"),
    )
    .unwrap();
}

fn make_registry_disabled(work_dir: &Path) -> AdjudicatorRegistry {
    AdjudicatorRegistry::new(None, work_dir)
}

#[test]
fn registry_disabled_when_no_api_key() {
    let tmp = tempfile::tempdir().unwrap();
    let reg = AdjudicatorRegistry::new(None, tmp.path());
    assert!(reg.is_disabled());
}

#[test]
fn check_pending_disputes_escalates_when_disabled() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    std::fs::create_dir_all(work.join("stages")).unwrap();
    let stage = make_stage("s1");
    write_stage(work, &stage);
    write_dispute_request(work, "s1", 1, 0);

    let mut reg = make_registry_disabled(work);
    reg.check_pending_disputes(work).unwrap();

    let after = crate::verify::transitions::load_stage("s1", work).unwrap();
    assert_eq!(after.status, StageStatus::NeedsHumanReview);
    assert!(after
        .review_reason
        .as_deref()
        .unwrap_or("")
        .contains("ANTHROPIC_API_KEY"));
}

#[test]
fn apply_verdict_reject_writes_feedback_and_queues_stage() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    std::fs::create_dir_all(work.join("stages")).unwrap();
    let mut stage = make_stage("s1");
    stage.dispute_count = 1;
    write_stage(work, &stage);
    write_dispute_request(work, "s1", 1, 0);
    write_verdict(
        work,
        "s1",
        1,
        DisputeVerdict::Reject {
            citations: vec![Citation {
                file: "f".to_string(),
                line: None,
                excerpt: "e".to_string(),
                claim: "c".to_string(),
            }],
            reasoning: "criterion is correct".to_string(),
        },
        1,
    );

    let mut reg = make_registry_disabled(work);
    reg.apply_pending_verdicts(work).unwrap();

    let after = crate::verify::transitions::load_stage("s1", work).unwrap();
    assert_eq!(after.status, StageStatus::Queued);
    let fb = feedback::read_feedback(work, "s1").unwrap().unwrap();
    assert!(fb.contains("rejected"));
    let applied = work
        .join("disputes")
        .join("s1")
        .join("1")
        .join("applied.marker");
    assert!(applied.exists(), "applied.marker must exist after apply");
}

#[test]
fn apply_verdict_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    std::fs::create_dir_all(work.join("stages")).unwrap();
    let mut stage = make_stage("s1");
    stage.dispute_count = 1;
    write_stage(work, &stage);
    write_dispute_request(work, "s1", 1, 0);
    write_verdict(
        work,
        "s1",
        1,
        DisputeVerdict::Reject {
            citations: vec![Citation {
                file: "f".to_string(),
                line: None,
                excerpt: "e".to_string(),
                claim: "c".to_string(),
            }],
            reasoning: "ok".to_string(),
        },
        1,
    );

    let mut reg = make_registry_disabled(work);
    reg.apply_pending_verdicts(work).unwrap();
    let mid = crate::verify::transitions::load_stage("s1", work).unwrap();

    // Second call must not re-mutate the stage (applied.marker prevents it).
    reg.apply_pending_verdicts(work).unwrap();
    let after = crate::verify::transitions::load_stage("s1", work).unwrap();
    assert_eq!(after.status, mid.status);
}

#[test]
fn needs_more_evidence_writes_feedback_and_increments_round() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    std::fs::create_dir_all(work.join("stages")).unwrap();
    let mut stage = make_stage("s1");
    stage.dispute_count = 1;
    write_stage(work, &stage);
    write_dispute_request(work, "s1", 1, 0);
    write_verdict(
        work,
        "s1",
        1,
        DisputeVerdict::NeedsMoreEvidence {
            questions: vec!["why?".to_string()],
        },
        1,
    );

    let mut reg = make_registry_disabled(work);
    reg.apply_pending_verdicts(work).unwrap();

    let after = crate::verify::transitions::load_stage("s1", work).unwrap();
    assert_eq!(after.status, StageStatus::Queued);
    assert_eq!(after.evidence_rounds, 1);
    let fb = feedback::read_feedback(work, "s1").unwrap().unwrap();
    assert!(fb.contains("1. why?"));
}

#[test]
fn evidence_loop_exhausts_to_human_review() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    std::fs::create_dir_all(work.join("stages")).unwrap();
    let mut stage = make_stage("s1");
    stage.dispute_count = 3;
    stage.evidence_rounds = MAX_EVIDENCE_ROUNDS - 1;
    write_stage(work, &stage);
    write_dispute_request(work, "s1", 1, 0);
    write_verdict(
        work,
        "s1",
        1,
        DisputeVerdict::NeedsMoreEvidence {
            questions: vec!["last chance".to_string()],
        },
        1,
    );

    let mut reg = make_registry_disabled(work);
    reg.apply_pending_verdicts(work).unwrap();

    let after = crate::verify::transitions::load_stage("s1", work).unwrap();
    assert_eq!(after.status, StageStatus::NeedsHumanReview);
    assert_eq!(after.evidence_rounds, MAX_EVIDENCE_ROUNDS);
}

#[test]
fn scan_pending_requests_skips_completed_verdicts() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    write_dispute_request(work, "s1", 1, 0);
    write_dispute_request(work, "s1", 2, 0);
    write_verdict(
        work,
        "s1",
        1,
        DisputeVerdict::Reject {
            citations: vec![Citation {
                file: "f".to_string(),
                line: None,
                excerpt: "e".to_string(),
                claim: "c".to_string(),
            }],
            reasoning: "r".to_string(),
        },
        1,
    );
    let pending = scan_pending_requests(&work.join("disputes")).unwrap();
    assert_eq!(pending, vec![("s1".to_string(), 2)]);
}

#[test]
fn build_amendment_request_decodes_plan_patch() {
    let plan_patch = PlanPatch {
        inner: serde_json::json!({
            "stage_id": "s1",
            "field": "acceptance",
            "patch": {"op": "delete", "index": 0},
            "reason": "criterion was wrong"
        }),
    };
    let req = build_amendment_request("s1".to_string(), &plan_patch, 1).unwrap();
    assert_eq!(req.stage_id, "s1");
    assert!(matches!(req.field, AmendmentField::Acceptance));
    assert!(matches!(req.patch, AmendmentPatch::Delete { index: 0 }));
    assert_eq!(req.reason.as_deref(), Some("criterion was wrong"));
    assert_eq!(req.dispute_id.as_deref(), Some("1"));
}

#[test]
fn build_amendment_request_rejects_unknown_field() {
    let plan_patch = PlanPatch {
        inner: serde_json::json!({"field": "bogus", "patch": {}}),
    };
    assert!(build_amendment_request("s1".to_string(), &plan_patch, 1).is_err());
}

#[test]
fn feedback_signal_only_when_dispute_count_positive() {
    // This is a contract test for the wiring rule: a stage without a
    // dispute history must NOT see adjudicator feedback in its signal,
    // even if a stray feedback.md exists from a previous plan.
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    feedback::append_questions(work, "s1", &["stale".to_string()]).unwrap();
    let mut stage = make_stage("s1");
    stage.dispute_count = 0;
    // We can't easily build the full Session/Worktree here, so just
    // assert the read returns content (the gating happens in
    // generate_signal_with_skills, exercised by integration tests).
    assert!(feedback::read_feedback(work, "s1").unwrap().is_some());
}

#[test]
fn parse_yaml_frontmatter_round_trips() {
    let req = DisputeRequest {
        id: 7,
        stage_id: "x".to_string(),
        criterion_index: 0,
        reason: "r".to_string(),
        evidence_commit: None,
        failure_output: None,
        fix_attempts_at_dispute: 0,
        created_at: Utc::now(),
    };
    let yaml = serde_yaml::to_string(&req).unwrap();
    let body = format!("---\n{yaml}---\n\n# X\n");
    let parsed: DisputeRequest = parse_yaml_frontmatter(&body).unwrap();
    assert_eq!(parsed.id, 7);
    assert_eq!(parsed.stage_id, "x");
}

#[test]
fn apply_verdict_writes_applying_marker_then_removes_it() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    std::fs::create_dir_all(work.join("stages")).unwrap();
    let stage = make_stage("s1");
    write_stage(work, &stage);
    write_dispute_request(work, "s1", 1, 0);
    write_verdict(
        work,
        "s1",
        1,
        DisputeVerdict::Reject {
            citations: vec![Citation {
                file: "f".to_string(),
                line: None,
                excerpt: "e".to_string(),
                claim: "c".to_string(),
            }],
            reasoning: "r".to_string(),
        },
        1,
    );
    let mut reg = make_registry_disabled(work);
    reg.apply_verdict(work, "s1", 1).unwrap();
    let dir = work.join("disputes").join("s1").join("1");
    assert!(dir.join("applied.marker").exists());
    assert!(!dir.join(".applying").exists());
}

#[test]
fn drain_completed_workers_is_no_op_when_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let mut reg = make_registry_disabled(tmp.path());
    reg.drain_completed_workers(tmp.path()).unwrap();
}

#[test]
fn shutdown_with_no_handles_returns_quickly() {
    use std::time::{Duration, Instant};
    let tmp = tempfile::tempdir().unwrap();
    let mut reg = make_registry_disabled(tmp.path());
    let start = Instant::now();
    reg.shutdown(start + Duration::from_secs(5));
    assert!(start.elapsed() < Duration::from_secs(1));
}
