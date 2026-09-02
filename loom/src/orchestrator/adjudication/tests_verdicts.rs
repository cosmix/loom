//! Verdict-application tests for the adjudication module: escalation,
//! idempotency, evidence-loop handling, and the requeue/hold logic for
//! multiple unanswered disputes on one stage. Split out of `tests.rs` to
//! keep that file under the maintainability limit.

use super::tests::{make_stage, reject_verdict, write_dispute_request, write_stage, write_verdict};
use super::{feedback, AdjudicatorRegistry, MAX_EVIDENCE_ROUNDS};
use crate::models::dispute::DisputeVerdict;
use crate::models::stage::StageStatus;

/// A Reject is a deadlock, not a retry: the agent called the criterion
/// impossible and the adjudicator upheld it, so re-queueing would loop the
/// same disagreement forever.
#[test]
fn apply_verdict_reject_escalates_to_human_review() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    std::fs::create_dir_all(work.join("stages")).unwrap();
    let mut stage = make_stage("s1");
    stage.dispute_count = 1;
    write_stage(work, &stage);
    write_dispute_request(work, "s1", 1, 0);
    write_verdict(work, "s1", 1, reject_verdict(), 1);

    let reg = AdjudicatorRegistry::new();
    reg.apply_pending_verdicts(work).unwrap();

    let after = crate::verify::transitions::load_stage("s1", work).unwrap();
    assert_eq!(after.status, StageStatus::NeedsHumanReview);
    let reason = after.review_reason.as_deref().unwrap_or("");
    assert!(reason.contains("upheld the disputed acceptance criterion"));
    // The reason must point at the real verdict file (this repo's layout, not
    // a hard-coded `.loom/work/...` guess) and name the operator's next step.
    let verdict_path = work
        .join("disputes")
        .join("s1")
        .join("1")
        .join("verdict.md");
    assert!(
        reason.contains(&verdict_path.display().to_string()),
        "reason: {reason}"
    );
    assert!(
        reason.contains("loom stage human-review s1"),
        "reason: {reason}"
    );
    // The reasoning is still written where the agent (and the human) read it.
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
    write_verdict(work, "s1", 1, reject_verdict(), 1);

    let reg = AdjudicatorRegistry::new();
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

    let reg = AdjudicatorRegistry::new();
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

    let reg = AdjudicatorRegistry::new();
    reg.apply_pending_verdicts(work).unwrap();

    let after = crate::verify::transitions::load_stage("s1", work).unwrap();
    assert_eq!(after.status, StageStatus::NeedsHumanReview);
    assert_eq!(after.evidence_rounds, MAX_EVIDENCE_ROUNDS);
}

/// Two unanswered disputes on one stage: applying the verdict for the first
/// must not re-queue the stage while the second still has no verdict, or
/// `job_for_dispute` (which only schedules a `NeedsAdjudication` stage) would
/// never be able to schedule the second dispute's adjudicator.
#[test]
fn a_verdict_holds_the_stage_while_another_dispute_is_unanswered() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    std::fs::create_dir_all(work.join("stages")).unwrap();
    write_stage(work, &make_stage("s1"));
    write_dispute_request(work, "s1", 1, 0);
    write_dispute_request(work, "s1", 2, 0);
    write_verdict(
        work,
        "s1",
        1,
        DisputeVerdict::NeedsMoreEvidence {
            questions: vec!["why?".to_string()],
        },
        1,
    );

    let reg = AdjudicatorRegistry::new();
    reg.apply_verdict(work, "s1", 1).unwrap();

    let after = crate::verify::transitions::load_stage("s1", work).unwrap();
    assert_eq!(after.status, StageStatus::NeedsAdjudication);
    let applied = work
        .join("disputes")
        .join("s1")
        .join("1")
        .join("applied.marker");
    assert!(applied.exists(), "applied.marker must exist after apply");

    let jobs = reg.disputes_awaiting_session(work).unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].request.id, 2);
}

/// Once the last unanswered dispute on a stage gets its verdict, the stage
/// re-queues.
#[test]
fn the_last_verdict_on_a_stage_requeues_it() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    std::fs::create_dir_all(work.join("stages")).unwrap();
    write_stage(work, &make_stage("s1"));
    write_dispute_request(work, "s1", 1, 0);
    write_dispute_request(work, "s1", 2, 0);
    write_verdict(
        work,
        "s1",
        1,
        DisputeVerdict::NeedsMoreEvidence {
            questions: vec!["why?".to_string()],
        },
        1,
    );

    let reg = AdjudicatorRegistry::new();
    reg.apply_verdict(work, "s1", 1).unwrap();
    let mid = crate::verify::transitions::load_stage("s1", work).unwrap();
    assert_eq!(mid.status, StageStatus::NeedsAdjudication);

    // Only now does dispute 2 get its verdict — writing it earlier would mean
    // it was never counted as unanswered, and the `mid` assertion above would
    // not exercise the "hold" branch at all.
    write_verdict(
        work,
        "s1",
        2,
        DisputeVerdict::NeedsMoreEvidence {
            questions: vec!["also why?".to_string()],
        },
        1,
    );
    reg.apply_verdict(work, "s1", 2).unwrap();
    let after = crate::verify::transitions::load_stage("s1", work).unwrap();
    assert_eq!(after.status, StageStatus::Queued);
}

#[test]
fn apply_verdict_writes_applying_marker_then_removes_it() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    std::fs::create_dir_all(work.join("stages")).unwrap();
    write_stage(work, &make_stage("s1"));
    write_dispute_request(work, "s1", 1, 0);
    write_verdict(work, "s1", 1, reject_verdict(), 1);

    let reg = AdjudicatorRegistry::new();
    reg.apply_verdict(work, "s1", 1).unwrap();
    let dir = work.join("disputes").join("s1").join("1");
    assert!(dir.join("applied.marker").exists());
    assert!(!dir.join(".applying").exists());
}

/// A Reject verdict on one dispute puts the stage in `NeedsHumanReview`; a
/// later verdict on a SIBLING dispute must not force it back to `Queued` and
/// erase that escalation.
#[test]
fn a_reject_verdict_is_not_undone_by_a_later_verdict_on_a_sibling_dispute() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    std::fs::create_dir_all(work.join("stages")).unwrap();
    write_stage(work, &make_stage("s1"));
    write_dispute_request(work, "s1", 1, 0);
    write_dispute_request(work, "s1", 2, 0);
    write_verdict(work, "s1", 1, reject_verdict(), 1);

    let reg = AdjudicatorRegistry::new();
    reg.apply_verdict(work, "s1", 1).unwrap();
    let after_reject = crate::verify::transitions::load_stage("s1", work).unwrap();
    assert_eq!(after_reject.status, StageStatus::NeedsHumanReview);

    write_verdict(
        work,
        "s1",
        2,
        DisputeVerdict::NeedsMoreEvidence {
            questions: vec!["why?".to_string()],
        },
        1,
    );
    reg.apply_verdict(work, "s1", 2).unwrap();

    let after = crate::verify::transitions::load_stage("s1", work).unwrap();
    assert_eq!(after.status, StageStatus::NeedsHumanReview);
    assert!(after
        .review_reason
        .as_deref()
        .unwrap_or("")
        .contains("dispute 1"));
}
