//! A relayed `complete` meets the completion gates before any run: a stage
//! the gates refuse queues no host job and the session reads the gate's
//! message at once, a stage they pass queues its run, and the application of
//! that run still checks the gates, since they can fail while it goes.

use std::fs;

use super::apply_complete;
use crate::fs::session_files::load_session_exact;
use crate::host_verify::apply::Applied;
use crate::host_verify::tests_complete_support::{git, Fixture, SESSION, STAGE};
use crate::host_verify::{digest, store, JobKind, JobRecord, JobResult, JobState};
use crate::models::stage::StageType;
use crate::orchestrator::core::inbox_drain::test_support::{fixture, FakeHost};
use crate::orchestrator::core::inbox_drain::Settle;
use crate::relay::HostJobRequest;

/// A host over the completion fixture's state root and repository. The drain
/// fixture only supplies the host's other fields; its own state root is unused.
fn host(fx: &Fixture) -> FakeHost {
    let mut host = fixture().host(true);
    host.work_dir = fx.work_dir.clone();
    host.repo_root = fx.repo.clone();
    host
}

/// Relay `complete` for the worktree's committed HEAD, as `loom stage
/// complete` does, and settle it.
fn relay_complete(fx: &Fixture) -> Settle {
    let session = load_session_exact(&fx.work_dir, SESSION)
        .expect("load the session")
        .expect("the session record");
    let request = HostJobRequest {
        head: fx.head(),
        tree_digest: digest::tree_digest(&fx.worktree).expect("the tree digest"),
        no_cache: false,
        criteria: None,
    };
    apply_complete(&host(fx), STAGE, &session, &request, "r1")
}

fn refusal(settle: Settle) -> String {
    match settle {
        Settle::Refused(reason) => reason,
        Settle::Applied(note) => panic!("expected a refusal, got Applied({note:?})"),
    }
}

fn assert_no_job(fx: &Fixture) {
    let current = store::load_current(&fx.work_dir, STAGE).expect("load the current record");
    assert!(current.is_none(), "no host job was queued: {current:?}");
}

/// Commit a two-test file on `main`, bring the stage branch up to it, then
/// drop one of the tests on the stage branch: a test-integrity event.
fn drop_a_base_test(fx: &Fixture) {
    let file = "src/calc_tests.rs";
    fs::create_dir_all(fx.repo.join("src")).expect("create src");
    let base = "#[test]\nfn adds() {}\n\n#[test]\nfn subtracts() {}\n";
    fs::write(fx.repo.join(file), base).expect("write the base tests");
    git(&fx.repo, &["add", file]);
    git(&fx.repo, &["commit", "-q", "-m", "tests"]);
    git(&fx.worktree, &["merge", "-q", "--ff-only", "main"]);
    fs::write(fx.worktree.join(file), "#[test]\nfn adds() {}\n").expect("drop a test");
    git(&fx.worktree, &["commit", "-q", "-am", "drop a test"]);
}

#[test]
fn a_stage_without_a_current_review_is_refused_before_any_job() {
    let fx = Fixture::new(StageType::Standard, 2);

    let reason = refusal(relay_complete(&fx));

    assert!(reason.starts_with("completion gates: "), "{reason}");
    assert!(reason.contains("review gate"), "{reason}");
    assert_no_job(&fx);

    fx.record_review(true);
    let stale = refusal(relay_complete(&fx));
    assert!(stale.contains("review gate"), "{stale}");
    assert_no_job(&fx);
}

#[test]
fn an_integrity_event_is_refused_before_any_job() {
    let fx = Fixture::new(StageType::Standard, 2);
    drop_a_base_test(&fx);
    fx.record_review(false);

    let reason = refusal(relay_complete(&fx));

    assert!(reason.starts_with("completion gates: "), "{reason}");
    assert!(reason.contains("test-integrity gate"), "{reason}");
    assert_no_job(&fx);
}

#[test]
fn a_stage_the_gates_pass_queues_its_completion() {
    let fx = Fixture::new(StageType::Standard, 2);
    fx.record_review(false);

    let settle = relay_complete(&fx);

    assert!(matches!(settle, Settle::Applied(_)), "{settle:?}");
    let job = store::load_current(&fx.work_dir, STAGE)
        .expect("load the current record")
        .expect("the queued job");
    assert_eq!((job.kind, job.state), (JobKind::Complete, JobState::Queued));
    assert_eq!(job.head, fx.head());
}

/// The run that passed, as the runner leaves it: the record finished, the
/// result bound to the stage's policy.
fn pass(fx: &Fixture, mut job: JobRecord) -> JobRecord {
    job.state = JobState::Passed;
    store::save_current(&fx.work_dir, &job).expect("finish the job");
    let result = JobResult {
        job_id: job.id.clone(),
        head: job.head.clone(),
        passed: true,
        criteria: Vec::new(),
        error: None,
        policy_hash: Some(fx.policy()),
    };
    store::save_result(&fx.work_dir, STAGE, &result).expect("save the result");
    job
}

#[test]
fn the_application_still_refuses_a_review_gone_stale_during_the_run() {
    let fx = Fixture::new(StageType::Standard, 2);
    fx.record_review(false);
    assert!(matches!(relay_complete(&fx), Settle::Applied(_)));
    let job = store::load_current(&fx.work_dir, STAGE)
        .expect("load the current record")
        .expect("the queued job");

    fx.record_review(true);
    let passed = pass(&fx, job);
    let applied = fx.apply(&passed).expect("apply the run");

    assert_eq!(applied, Applied::Recorded);
    let diagnostic = fx
        .assert_rejected(&passed)
        .diagnostic_first_line
        .expect("the rejection's diagnostic");
    assert!(diagnostic.starts_with("completion gates: "), "{diagnostic}");
}
