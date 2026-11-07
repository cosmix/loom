//! Contracts for the stage-bound completion job a verdict that changes
//! nothing leaves behind (`JobKind::VerdictComplete`): its application on a
//! held stage, and which held stages the sweep submits one for.
//!
//! A held stage is `NeedsAdjudication` with no session: the verdict pass
//! retired the disputing agent and kept the stage for the daemon's run.

use std::fs;

use chrono::Utc;

use super::apply::{self, Applied};
use super::tests_complete_support::{ignored, Fixture, STAGE};
use super::verdict_complete::{submit_verdict_completion, VerdictStart, STAGE_BOUND_SESSION};
use super::{store, JobKind, JobRecord, JobState};
use crate::models::dispute::{request_file, DisputeKind, DisputeRequest};
use crate::models::session::SessionStatus;
use crate::models::stage::{StageStatus, StageType};
use crate::orchestrator::adjudication::feedback;
use crate::verify::transitions::update_stage;

/// A fixture whose stage the verdict pass held: `NeedsAdjudication`, no session.
fn held(plan_version: u32) -> Fixture {
    let fx = Fixture::new(StageType::Standard, plan_version);
    update_stage(STAGE, &fx.work_dir, |stage| {
        stage.status = StageStatus::NeedsAdjudication;
        stage.session = None;
        Ok(())
    })
    .expect("hold the stage");
    fx
}

/// A finished VerdictComplete job, bound to the stage as the sweep submits it.
fn verdict_job(fx: &Fixture, state: JobState, outcomes: &[(&str, bool)]) -> JobRecord {
    let mut record = fx.finished(JobKind::VerdictComplete, state, outcomes);
    record.session_id = STAGE_BOUND_SESSION.to_string();
    store::save_current(&fx.work_dir, &record).expect("save the stage-bound record");
    record
}

fn submit(fx: &Fixture) -> VerdictStart {
    submit_verdict_completion(&fx.work_dir, &fx.load_stage(), Utc::now())
        .expect("submit the verdict completion")
}

/// The submission refused the stage and queued no VerdictComplete job.
fn assert_ineligible(fx: &Fixture) {
    let start = submit(fx);
    assert!(
        matches!(start, VerdictStart::Ineligible(_)),
        "expected Ineligible, got {start:?}"
    );
    let current = store::load_current(&fx.work_dir, STAGE).expect("load the current record");
    assert!(
        current.is_none_or(|record| record.kind != JobKind::VerdictComplete),
        "an ineligible stage gets no VerdictComplete job"
    );
}

/// The submission queued a stage-bound VerdictComplete job at the current
/// HEAD; returns its id.
fn assert_submitted(fx: &Fixture) -> String {
    let start = submit(fx);
    let VerdictStart::Submitted(id) = start else {
        panic!("expected Submitted, got {start:?}");
    };
    let current = store::load_current(&fx.work_dir, STAGE)
        .expect("load the current record")
        .expect("a current record");
    assert_eq!(current.id, id);
    assert_eq!(current.kind, JobKind::VerdictComplete);
    assert_eq!(current.state, JobState::Queued);
    assert_eq!(current.session_id, STAGE_BOUND_SESSION);
    assert_eq!(current.head, fx.head());
    id
}

#[test]
fn verdict_pass_completes_from_adjudication() {
    let fx = held(1);
    let record = verdict_job(&fx, JobState::Passed, &[("acceptance-0", true)]);

    let applied = apply::apply_finished(&fx.work_dir, &record).expect("apply the job");

    assert_eq!(
        applied,
        Applied::Completed {
            commit: record.head.clone()
        }
    );
    let stage = fx.load_stage();
    assert_eq!(stage.status, StageStatus::Completed);
    assert_eq!(
        stage.completed_commit.as_deref(),
        Some(record.head.as_str())
    );
    assert!(stage.completed_at.is_some(), "the completion is stamped");
}

#[test]
fn verdict_pass_leaves_the_retired_session_alone() {
    let fx = held(1);
    assert_eq!(fx.session_status(), SessionStatus::Running);
    assert!(fx.checkpoint().is_none(), "no checkpoint before the run");
    let record = verdict_job(&fx, JobState::Passed, &[("acceptance-0", true)]);

    let applied = apply::apply_finished(&fx.work_dir, &record).expect("apply the job");

    assert!(matches!(applied, Applied::Completed { .. }), "{applied:?}");
    assert_eq!(fx.load_stage().status, StageStatus::Completed);
    assert!(
        fx.checkpoint().is_none(),
        "the retired session's checkpoint holds no attempt or receipt"
    );
    assert_eq!(
        fx.session_status(),
        SessionStatus::Running,
        "the retired session record is not marked Completed"
    );
}

/// Write `request.md` for dispute `id` of the fixture stage, with no verdict.
fn write_open_dispute(fx: &Fixture, id: u32) {
    let root = fx.work_dir.join("disputes");
    fs::create_dir_all(root.join(STAGE).join(id.to_string())).expect("dispute dir");
    let request = DisputeRequest {
        id,
        stage_id: STAGE.to_string(),
        kind: DisputeKind::Integrity {
            event_ids: Vec::new(),
            evidence: Vec::new(),
        },
        reason: "filed while the completion ran".to_string(),
        evidence_commit: None,
        failure_output: None,
        fix_attempts_at_dispute: 1,
        created_at: Utc::now(),
    };
    let yaml = crate::yaml::to_string(&request).expect("encode the request");
    let body = format!("---\n{yaml}---\n\n# Dispute {STAGE}/{id}\n");
    fs::write(request_file(&root, STAGE, id), body).expect("write the request");
}

#[test]
fn open_dispute_blocks_the_verdict_completion() {
    let fx = held(1);
    let record = verdict_job(&fx, JobState::Passed, &[("acceptance-0", true)]);
    write_open_dispute(&fx, 2);

    let applied = apply::apply_finished(&fx.work_dir, &record).expect("apply the job");

    ignored(applied);
    let stage = fx.load_stage();
    assert_ne!(stage.status, StageStatus::Completed);
    assert_eq!(stage.completed_commit, None);
}

#[test]
fn failed_verdict_run_hands_back_with_its_failures() {
    let fx = held(1);
    let record = verdict_job(&fx, JobState::Failed, &[("acceptance-0", false)]);

    let applied = apply::apply_finished(&fx.work_dir, &record).expect("apply the job");

    assert!(matches!(applied, Applied::HandedBack(_)), "{applied:?}");
    let stage = fx.load_stage();
    assert_eq!(stage.status, StageStatus::Queued);
    assert_eq!(stage.completed_commit, None);
    let notes = feedback::read_feedback(&fx.work_dir, STAGE)
        .expect("read the feedback")
        .expect("the hand-back wrote feedback");
    assert!(notes.contains("acceptance-0"), "{notes}");
}

#[test]
fn interrupted_verdict_run_requeues() {
    let fx = held(1);
    let mut record = fx.record(JobKind::VerdictComplete, JobState::Interrupted);
    record.session_id = STAGE_BOUND_SESSION.to_string();
    store::save_current(&fx.work_dir, &record).expect("save the stage-bound record");

    let applied = apply::apply_finished(&fx.work_dir, &record).expect("apply the job");

    assert!(matches!(applied, Applied::HandedBack(_)), "{applied:?}");
    let stage = fx.load_stage();
    assert_eq!(stage.status, StageStatus::Queued);
    assert_eq!(stage.completed_commit, None);
}

#[test]
fn eligible_held_stage_gets_one_stage_bound_job() {
    assert_eq!(JobKind::VerdictComplete.name(), "verdict_complete");
    let fx = held(2);
    fx.finished(
        JobKind::Complete,
        JobState::Passed,
        &[("acceptance-0", true)],
    );

    let id = assert_submitted(&fx);

    assert_eq!(submit(&fx), VerdictStart::Waiting(id.clone()));
    let current = store::load_current(&fx.work_dir, STAGE)
        .expect("load the current record")
        .expect("a current record");
    assert_eq!(current.id, id, "the second sweep queues no second job");
    assert_eq!(current.state, JobState::Queued);
}

#[test]
fn moved_head_falls_back_to_a_fresh_session() {
    let fx = held(2);
    fx.finished(
        JobKind::Complete,
        JobState::Passed,
        &[("acceptance-0", true)],
    );
    fx.commit_empty();

    assert_ineligible(&fx);
}

#[test]
fn dirty_tree_falls_back_to_a_fresh_session() {
    let fx = held(2);
    fx.finished(
        JobKind::Complete,
        JobState::Passed,
        &[("acceptance-0", true)],
    );
    let ignore = fx.worktree.join(".gitignore");
    let mut content = fs::read_to_string(&ignore).expect("read .gitignore");
    content.push_str("target/\n");
    fs::write(&ignore, content).expect("edit .gitignore");

    assert_ineligible(&fx);
}

#[test]
fn failed_agent_completion_is_not_rerun() {
    let fx = held(2);
    fx.finished(
        JobKind::Complete,
        JobState::Failed,
        &[("acceptance-0", false)],
    );

    assert_ineligible(&fx);
}

#[test]
fn impact_only_failure_is_rerun() {
    let impact_only = held(2);
    impact_only.finished(
        JobKind::Complete,
        JobState::Failed,
        &[("acceptance-0", true), ("impact-tests", false)],
    );
    assert_submitted(&impact_only);

    let both = held(2);
    both.finished(
        JobKind::Complete,
        JobState::Failed,
        &[("acceptance-0", false), ("impact-tests", false)],
    );
    assert_ineligible(&both);
}

#[test]
fn stale_policy_falls_back_to_a_fresh_session() {
    let fx = held(2);
    fx.finished(
        JobKind::Complete,
        JobState::Passed,
        &[("acceptance-0", true)],
    );
    fx.amend_policy();

    assert_ineligible(&fx);
}
