//! Tests for the host child's `Complete` run (`child::run_job` on a
//! `JobKind::Complete` record). Each one starts loom's real confinement and
//! skips where it cannot start; every one is serial, because the spawn
//! observer is one process-wide slot.

use std::fs;

use serial_test::serial;

use super::child::run_job;
use super::tests_complete_support::{Fixture, STAGE};
use super::{store, JobKind, JobState};
use crate::models::stage::StageType;
use crate::process::sandbox_probe::{confined_commands_runnable, skip_unless};

fn confinement_unavailable(test_name: &str) -> bool {
    skip_unless(
        confined_commands_runnable(),
        test_name,
        "loom's confinement cannot start here",
    )
}

#[test]
#[serial]
fn a_complete_job_on_a_committed_tree_passes() {
    if confinement_unavailable("a_complete_job_on_a_committed_tree_passes") {
        return;
    }
    let fx = Fixture::new(StageType::Standard, 1);
    let record = fx.record(JobKind::Complete, JobState::Running);
    let result = run_job(&fx.work_dir, STAGE, &record.id).expect("run_job");
    assert!(result.passed, "{result:?}");
    assert_eq!(result.error, None);
    assert_eq!(result.head, record.head);
    assert_eq!(
        result.policy_hash,
        Some(fx.policy()),
        "the result carries the bound policy"
    );
    assert!(
        result
            .criteria
            .iter()
            .any(|outcome| outcome.label == "acceptance-0" && outcome.passed),
        "{result:?}"
    );
    assert!(
        result.criteria.iter().all(|outcome| outcome.passed),
        "{result:?}"
    );
    let saved = store::load_result(&fx.work_dir, STAGE, &record.id).expect("load result");
    assert_eq!(
        saved.as_ref(),
        Some(&result),
        "result.json matches the return"
    );
}

#[test]
#[serial]
fn a_complete_job_on_a_dirty_tree_fails_at_capture() {
    if confinement_unavailable("a_complete_job_on_a_dirty_tree_fails_at_capture") {
        return;
    }
    let fx = Fixture::new(StageType::Standard, 1);
    // A tracked file that differs from HEAD; the record is taken after, so
    // the request's tree digest matches and the failure is the snapshot's.
    let ignore = fx.worktree.join(".gitignore");
    let mut content = fs::read_to_string(&ignore).expect("read .gitignore");
    content.push_str("dirty\n");
    fs::write(&ignore, content).expect("write .gitignore");
    let record = fx.record(JobKind::Complete, JobState::Running);
    let result = run_job(&fx.work_dir, STAGE, &record.id).expect("run_job");
    assert!(!result.passed);
    let error = result.error.as_deref().expect("an error");
    assert!(error.contains("not committed"), "{error}");
    assert!(result.criteria.is_empty(), "nothing ran: {result:?}");
    assert_eq!(result.policy_hash, None, "no snapshot, no policy");
}
