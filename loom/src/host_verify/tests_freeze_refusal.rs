//! A freeze job that refuses: its application records the refusal in
//! `contracts/<stage>/refused.json` for the tick step to file, and the stage
//! keeps executing. Uses the `tests_freeze` fixture.

use std::os::unix::fs::PermissionsExt;

use serial_test::serial;

use super::apply::Applied;
use super::freeze::{apply_freeze_result, REPORTS_FILE};
use super::runner::DAEMON_STOPPED;
use super::tests_freeze::{
    confinement_unavailable, refused_with, verdicts, Fixture, HARNESS, STAGE,
};
use super::{store, JobState};
use crate::models::stage::StageStatus;
use crate::verify::contracts::refusal::{daemon_refusal, RefusalSource, INFRASTRUCTURE_PREFIX};
use crate::verify::contracts::test_support::CONTRACT_FILE;
use crate::verify::transitions::{load_stage, update_stage};

/// The refusal was recorded for the job's session with a problem naming
/// `naming`, and the stage keeps executing with nothing frozen. Returns the
/// recorded problems.
fn assert_recorded(fx: &Fixture, applied: Applied, naming: &str) -> Vec<String> {
    let message = refused_with(applied);
    assert!(message.contains(naming), "{message}");
    assert_eq!(
        fx.status(),
        StageStatus::Executing,
        "a refusal parks nothing"
    );
    let stage = load_stage(STAGE, &fx.work_dir).unwrap();
    assert_eq!(stage.review_reason, None);
    assert!(!fx.frozen());
    let record = daemon_refusal(&fx.work_dir, STAGE).unwrap().unwrap();
    assert_eq!(record.session_id, fx.session_id);
    assert_eq!(record.source, RefusalSource::Daemon);
    assert_eq!(record.declined, None);
    assert!(
        record
            .problems
            .iter()
            .any(|problem| problem.contains(naming)),
        "{:?}",
        record.problems
    );
    record.problems
}

fn assert_infrastructure(problems: &[String]) {
    assert!(
        problems[0].starts_with(INFRASTRUCTURE_PREFIX),
        "{problems:?}"
    );
}

fn assert_not_infrastructure(problems: &[String]) {
    assert!(
        problems
            .iter()
            .all(|p| !p.starts_with(INFRASTRUCTURE_PREFIX)),
        "{problems:?}"
    );
}

#[test]
#[serial]
fn a_green_contract_fails_and_records_a_refusal_naming_it() {
    if confinement_unavailable("a_green_contract_fails_and_records_a_refusal_naming_it") {
        return;
    }
    let fx = Fixture::new("true");
    let (record, result) = fx.run(fx.job(JobState::Running));
    assert_eq!(record.state, JobState::Failed);
    assert_eq!(verdicts(&result)[0], ("contract:rejects-x", false));
    let problems = assert_recorded(&fx, fx.apply(&record), "rejects-x");
    assert_not_infrastructure(&problems);
}

#[test]
#[serial]
fn contract_files_changed_after_a_passing_run_record_a_refusal() {
    if confinement_unavailable("contract_files_changed_after_a_passing_run_record_a_refusal") {
        return;
    }
    let fx = Fixture::new("exit 1");
    let record = fx.passed_run();
    std::fs::write(fx.worktree.join(CONTRACT_FILE), "#[test]\nfn y() {}\n").unwrap();
    let problems = assert_recorded(&fx, fx.apply(&record), CONTRACT_FILE);
    assert_not_infrastructure(&problems);
}

#[test]
#[serial]
fn a_commit_after_a_passing_run_records_a_refusal() {
    if confinement_unavailable("a_commit_after_a_passing_run_records_a_refusal") {
        return;
    }
    let fx = Fixture::new("exit 1");
    fx.git(&["add", "-A"]);
    fx.git(&["commit", "-q", "-m", "contracts"]);
    let record = fx.passed_run();
    fx.git(&["commit", "-q", "--allow-empty", "-m", "later"]);
    assert_recorded(&fx, fx.apply(&record), "HEAD moved");
}

#[test]
#[serial]
fn a_contract_command_amended_after_a_passing_run_records_a_refusal() {
    if confinement_unavailable("a_contract_command_amended_after_a_passing_run_records_a_refusal") {
        return;
    }
    let fx = Fixture::new("exit 1");
    let record = fx.passed_run();
    update_stage(STAGE, &fx.work_dir, |stage| {
        stage.contracts[0].test = "exit 2".to_string();
        Ok(())
    })
    .unwrap();
    assert_recorded(&fx, fx.apply(&record), "contract definitions changed");
}

#[test]
#[serial]
fn a_job_cancelled_by_the_daemon_shutdown_records_an_infrastructure_refusal() {
    let fx = Fixture::new("exit 1");
    let mut record = fx.job(JobState::Cancelled);
    record.detail = Some(DAEMON_STOPPED.to_string());
    let problems = assert_recorded(
        &fx,
        fx.apply(&record),
        "the daemon stopped during the contract run",
    );
    assert_infrastructure(&problems);
}

#[test]
#[serial]
fn an_interrupted_run_records_an_infrastructure_refusal() {
    let fx = Fixture::new("exit 1");
    let mut record = fx.job(JobState::Interrupted);
    record.detail = Some("the daemon restarted during the run".to_string());
    let problems = assert_recorded(&fx, fx.apply(&record), "daemon restarted");
    assert_infrastructure(&problems);
}

#[test]
#[serial]
fn a_failed_run_with_its_detail_only_is_no_infrastructure_refusal() {
    let fx = Fixture::new("exit 1");
    let mut record = fx.job(JobState::Failed);
    record.detail = Some("deadline of 1 s passed".to_string());
    let problems = assert_recorded(&fx, fx.apply(&record), "deadline of");
    assert_not_infrastructure(&problems);
}

#[test]
#[serial]
fn a_passed_run_whose_reports_cannot_be_read_records_an_infrastructure_refusal() {
    let fx = Fixture::new("exit 1");
    let record = fx.job(JobState::Passed);
    store::save_job_file(&fx.work_dir, STAGE, &record.id, REPORTS_FILE, b"{not json").unwrap();
    let problems = assert_recorded(&fx, fx.apply(&record), "reports cannot be read");
    assert_infrastructure(&problems);
}

/// The record is the retried step: a refusal that cannot be written is an
/// `Err`, which the host-job tick applies again.
#[test]
#[serial]
fn a_refusal_that_cannot_be_recorded_is_an_error() {
    // SAFETY: `geteuid` has no preconditions.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let fx = Fixture::new("exit 1");
    let mut record = fx.job(JobState::Failed);
    record.detail = Some(format!("{HARNESS} vanished"));
    let dir = fx.work_dir.join("contracts").join(STAGE);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();

    let applied = apply_freeze_result(&fx.work_dir, &record);

    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(applied.is_err(), "{applied:?}");
    assert_eq!(daemon_refusal(&fx.work_dir, STAGE).unwrap(), None);
    assert_eq!(fx.status(), StageStatus::Executing);
    assert_recorded(&fx, fx.apply(&record), "vanished");
}
