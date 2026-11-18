//! The verdict completion beyond its frozen contracts: which record is the
//! latest completion request, which runs cannot stand for a session, the
//! gates and bindings that hand a passing run back, the deadline the runner
//! gives the job, and the retry owner giving up on its application. Then the
//! daemon's sweep over a held stage with a real worktree.

use chrono::Utc;

use super::{latest_completion_head, submit_verdict_completion, VerdictStart, STAGE_BOUND_SESSION};
use crate::fs::work_dir::write_terminal_config;
use crate::host_verify::apply::{self, Applied, APPLIED_MARKER};
use crate::host_verify::tests_complete_support::{ignored, Fixture, STAGE};
use crate::host_verify::{
    apply_retry, runner, store, CriterionOutcome, JobKind, JobRecord, JobResult, JobState,
};
use crate::models::session::{SessionBackendKind, TerminalConfig};
use crate::models::stage::{StageStatus, StageType};
use crate::orchestrator::adjudication::feedback;
use crate::orchestrator::core::{Orchestrator, OrchestratorConfig};
use crate::plan::ExecutionGraph;
use crate::verify::transitions::update_stage;

/// A fixture whose stage a verdict held: `NeedsAdjudication`, no session.
fn held(stage_type: StageType, plan_version: u32) -> Fixture {
    let fx = Fixture::new(stage_type, plan_version);
    update_stage(STAGE, &fx.work_dir, |s| {
        s.status = StageStatus::NeedsAdjudication;
        s.session = None;
        Ok(())
    })
    .expect("hold the stage");
    fx
}

/// A finished VerdictComplete job bound to the stage, as the sweep submits it.
fn verdict_job(fx: &Fixture, state: JobState, outcomes: &[(&str, bool)]) -> JobRecord {
    let mut record = fx.finished(JobKind::VerdictComplete, state, outcomes);
    record.session_id = STAGE_BOUND_SESSION.to_string();
    store::save_current(&fx.work_dir, &record).expect("save the stage-bound record");
    record
}

/// The submission refused the stage and queued no VerdictComplete job.
fn assert_ineligible(fx: &Fixture) {
    let start = submit_verdict_completion(&fx.work_dir, &fx.load_stage(), Utc::now())
        .expect("submit the verdict completion");
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

fn feedback_of(fx: &Fixture) -> String {
    feedback::read_feedback(&fx.work_dir, STAGE)
        .expect("read the feedback")
        .expect("the hand-back wrote feedback")
}

/// A job of `kind` at the current HEAD with submission order `order`.
fn ordered(fx: &Fixture, kind: JobKind, order: u64) -> JobRecord {
    let mut record = fx.record(kind, JobState::Passed);
    record.submission_order = order;
    store::save_current(&fx.work_dir, &record).expect("save the ordered record");
    record
}

#[test]
fn the_latest_completion_request_is_the_complete_record_submitted_last() {
    let fx = Fixture::new(StageType::Standard, 2);
    assert_eq!(latest_completion_head(&fx.work_dir, STAGE).unwrap(), None);

    let latest = ordered(&fx, JobKind::Complete, 2);
    fx.commit_empty();
    // Queued later but submitted earlier: submission order decides.
    ordered(&fx, JobKind::Complete, 1);
    fx.commit_empty();
    ordered(&fx, JobKind::Check, 3);
    fx.commit_empty();
    ordered(&fx, JobKind::VerdictComplete, 4);

    assert_eq!(
        latest_completion_head(&fx.work_dir, STAGE).unwrap(),
        Some(latest.head)
    );
}

#[test]
fn a_run_failing_more_than_its_impact_tests_queues_nothing() {
    let fx = held(StageType::Standard, 2);
    fx.finished(
        JobKind::Complete,
        JobState::Failed,
        &[("contracts", false), ("impact-tests", false)],
    );

    assert_ineligible(&fx);
}

#[test]
fn a_run_stopped_by_a_pipeline_error_queues_nothing() {
    let fx = held(StageType::Standard, 2);
    let record = fx.record(JobKind::Complete, JobState::Failed);
    let result = JobResult {
        job_id: record.id.clone(),
        head: record.head.clone(),
        passed: false,
        criteria: vec![CriterionOutcome {
            label: "acceptance-0".to_string(),
            passed: true,
            summary: "✓ acceptance-0".to_string(),
            log: None,
        }],
        error: Some("the pipeline stopped".to_string()),
        policy_hash: Some(fx.policy()),
    };
    store::save_result(&fx.work_dir, STAGE, &result).expect("save the result");

    assert_ineligible(&fx);
}

#[test]
fn a_passed_record_without_a_result_queues_nothing() {
    let fx = held(StageType::Standard, 2);
    fx.record(JobKind::Complete, JobState::Passed);

    assert_ineligible(&fx);
}

#[test]
fn a_plan_v1_or_knowledge_stage_queues_nothing() {
    for fx in [held(StageType::Standard, 1), held(StageType::Knowledge, 2)] {
        fx.passed();
        assert_ineligible(&fx);
    }
}

#[test]
fn a_pass_the_completion_gates_refuse_goes_to_a_fresh_session() {
    let fx = held(StageType::Standard, 2);
    let record = verdict_job(&fx, JobState::Passed, &[("acceptance-0", true)]);

    let applied = apply::apply_finished(&fx.work_dir, &record).expect("apply the job");

    assert!(matches!(applied, Applied::HandedBack(_)), "{applied:?}");
    assert_eq!(fx.load_stage().status, StageStatus::Queued);
    let notes = feedback_of(&fx);
    assert!(notes.contains("completion gates"), "{notes}");
}

#[test]
fn a_pass_whose_head_moved_goes_to_a_fresh_session() {
    let fx = held(StageType::Standard, 1);
    let record = verdict_job(&fx, JobState::Passed, &[("acceptance-0", true)]);
    let moved = fx.commit_empty();

    let applied = apply::apply_finished(&fx.work_dir, &record).expect("apply the job");

    assert!(matches!(applied, Applied::HandedBack(_)), "{applied:?}");
    let stage = fx.load_stage();
    assert_eq!(stage.status, StageStatus::Queued);
    assert_eq!(stage.completed_commit, None);
    let notes = feedback_of(&fx);
    assert!(notes.contains(&moved), "{notes}");
}

#[test]
fn a_cancelled_run_goes_to_a_fresh_session_without_failures() {
    let fx = held(StageType::Standard, 1);
    let mut record = fx.record(JobKind::VerdictComplete, JobState::Cancelled);
    record.session_id = STAGE_BOUND_SESSION.to_string();
    store::save_current(&fx.work_dir, &record).expect("save the stage-bound record");

    let applied = apply::apply_finished(&fx.work_dir, &record).expect("apply the job");

    assert_eq!(
        applied,
        Applied::HandedBack("the run was cancelled".to_string())
    );
    assert_eq!(fx.load_stage().status, StageStatus::Queued);
    assert_eq!(feedback::read_feedback(&fx.work_dir, STAGE).unwrap(), None);
}

#[test]
fn a_pass_while_another_job_decides_the_stage_is_ignored() {
    let fx = held(StageType::Standard, 1);
    let record = verdict_job(&fx, JobState::Passed, &[("acceptance-0", true)]);
    let other = fx.record(JobKind::Check, JobState::Running);

    let applied = apply::apply_finished(&fx.work_dir, &record).expect("apply the job");

    let reason = ignored(applied);
    assert!(reason.contains(&other.id), "{reason}");
    assert_eq!(fx.load_stage().status, StageStatus::NeedsAdjudication);
}

#[test]
fn a_completion_applied_again_is_already_completed() {
    let fx = held(StageType::Standard, 1);
    let record = verdict_job(&fx, JobState::Passed, &[("acceptance-0", true)]);
    let first = apply::apply_finished(&fx.work_dir, &record).expect("apply the job");
    assert!(matches!(first, Applied::Completed { .. }), "{first:?}");
    assert!(fx.marked(&record));
    let marker = store::job_dir(&fx.work_dir, STAGE, &record.id)
        .expect("the job directory")
        .join(APPLIED_MARKER);
    std::fs::remove_file(marker).expect("remove the applied marker");

    let again = apply::apply_finished(&fx.work_dir, &record).expect("apply the job again");

    assert_eq!(ignored(again), "already completed");
    assert_eq!(fx.load_stage().status, StageStatus::Completed);
}

#[test]
fn a_verdict_completion_gets_the_deadline_of_a_completion() {
    let fx = Fixture::new(StageType::Standard, 1);
    let plan = fx.work_dir.join("PLAN-host.md");
    let original = std::fs::read_to_string(&plan).unwrap();
    let with_after_stage = original.replace(
        "        - \"true\"\n",
        "        - \"true\"\n      after_stage:\n        - command: \"true\"\n          timeout_secs: 3600\n",
    );
    assert_ne!(original, with_after_stage);
    std::fs::write(&plan, with_after_stage).unwrap();
    let complete = fx.record(JobKind::Complete, JobState::Queued);
    let budget = |kind| {
        let record = JobRecord {
            kind,
            ..complete.clone()
        };
        runner::budget_for_job(&record, &fx.work_dir).unwrap().0
    };

    assert_eq!(budget(JobKind::VerdictComplete), budget(JobKind::Complete));
    assert!(
        budget(JobKind::VerdictComplete) > budget(JobKind::Check),
        "a completion's deadline counts its after-stage checks"
    );
}

#[test]
fn giving_up_on_the_application_hands_the_stage_back() {
    let fx = held(StageType::Standard, 1);
    let record = verdict_job(&fx, JobState::Passed, &[("acceptance-0", true)]);
    store::save_job_file(&fx.work_dir, STAGE, &record.id, "apply-attempts", b"10")
        .expect("exhaust the application attempts");

    apply_retry::apply_record(&fx.work_dir, &record);

    assert!(fx.marked(&record), "the job is marked once handed back");
    let stage = fx.load_stage();
    assert_eq!(stage.status, StageStatus::Queued);
    assert_eq!(stage.completed_commit, None);
    let notes = feedback_of(&fx);
    assert!(
        notes.contains("could not apply its completion run"),
        "{notes}"
    );
}

/// An orchestrator over the fixture on the tmux lane. Its terminal section
/// goes into `config.toml`, which the completion policy digests, so build it
/// before saving a run whose policy must still match.
fn orchestrator(fx: &Fixture) -> Orchestrator {
    let tmux = TerminalConfig {
        backend: SessionBackendKind::Tmux,
    };
    write_terminal_config(&fx.work_dir, &tmux).expect("write the terminal config");
    let config = OrchestratorConfig {
        work_dir: fx.work_dir.clone(),
        repo_root: fx.repo.clone(),
        enable_skill_routing: false,
        ..Default::default()
    };
    let graph = ExecutionGraph::build(Vec::new()).expect("an empty graph");
    Orchestrator::new(config, graph).expect("build the orchestrator")
}

/// Run the daemon's verdict pass: with no verdict pending, it is the sweep of
/// the held stages alone.
fn sweep(daemon: &mut Orchestrator) {
    daemon
        .apply_pending_verdicts()
        .expect("run the verdict pass");
}

#[test]
fn the_sweep_queues_the_completion_of_an_eligible_held_stage() {
    let fx = held(StageType::Standard, 2);
    let mut daemon = orchestrator(&fx);
    fx.record_review(false);
    fx.finished(
        JobKind::Complete,
        JobState::Failed,
        &[("acceptance-0", true), ("impact-tests", false)],
    );

    sweep(&mut daemon);

    let current = store::load_current(&fx.work_dir, STAGE)
        .expect("load the current record")
        .expect("a current record");
    assert_eq!(current.kind, JobKind::VerdictComplete);
    assert_eq!(current.state, JobState::Queued);
    assert_eq!(current.session_id, STAGE_BOUND_SESSION);
    assert_eq!(current.head, fx.head());
    assert_eq!(fx.load_stage().status, StageStatus::NeedsAdjudication);
}

#[test]
fn a_held_stage_reset_since_the_sweep_listed_it_queues_nothing() {
    let fx = held(StageType::Standard, 2);
    fx.passed();
    update_stage(STAGE, &fx.work_dir, |s| {
        s.status = StageStatus::WaitingForDeps;
        Ok(())
    })
    .expect("reset the stage");

    let start = submit_verdict_completion(&fx.work_dir, &fx.load_stage(), Utc::now())
        .expect("submit the verdict completion");

    assert_eq!(start, VerdictStart::NotHeld);
    let current = store::load_current(&fx.work_dir, STAGE).expect("load the current record");
    assert!(
        current.is_none_or(|record| record.kind != JobKind::VerdictComplete),
        "a reset stage gets no VerdictComplete job"
    );
}

#[test]
fn an_unreadable_completion_result_hands_the_stage_back_with_the_error() {
    let fx = held(StageType::Standard, 2);
    let mut daemon = orchestrator(&fx);
    let record = fx.passed();
    let result = store::job_dir(&fx.work_dir, STAGE, &record.id)
        .expect("the job directory")
        .join("result.json");
    std::fs::write(result, "{ not json").expect("corrupt the result");

    sweep(&mut daemon);

    assert_eq!(fx.load_stage().status, StageStatus::Queued);
    let notes = feedback_of(&fx);
    assert!(notes.contains("malformed"), "{notes}");
}

#[test]
fn the_sweep_completes_a_held_stage_from_its_passing_run() {
    let fx = held(StageType::Standard, 2);
    let mut daemon = orchestrator(&fx);
    fx.record_review(false);
    let passed = fx.passed();

    sweep(&mut daemon);

    let stage = fx.load_stage();
    assert_eq!(stage.status, StageStatus::Completed);
    assert_eq!(stage.completed_commit, Some(passed.head));
    let current = store::load_current(&fx.work_dir, STAGE).expect("load the current record");
    assert_eq!(current.map(|record| record.id), Some(passed.id));
}

#[test]
fn the_sweep_hands_back_a_stage_the_gates_refuse_without_a_job() {
    let fx = held(StageType::Standard, 2);
    let mut daemon = orchestrator(&fx);
    let passed = fx.passed();

    sweep(&mut daemon);

    assert_eq!(fx.load_stage().status, StageStatus::Queued);
    assert!(feedback_of(&fx).contains("completion gates"));
    let current = store::load_current(&fx.work_dir, STAGE).expect("load the current record");
    assert_eq!(current.map(|record| record.id), Some(passed.id));
}
