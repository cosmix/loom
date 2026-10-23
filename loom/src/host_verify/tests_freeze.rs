//! Tests for the freeze job and its application (`freeze`), against a real
//! repository with the stage worktree where loom puts it. Each contract is a
//! shell command (no test-runner adapter), so the run's verdict is its exit
//! code. Every test that runs a contract starts loom's real confinement and
//! skips where it cannot start.

use std::path::PathBuf;
use std::process::Command;

use chrono::Utc;
use serial_test::serial;
use tempfile::TempDir;

use super::apply::Applied;
use super::freeze::{apply_freeze_result, run_freeze_job};
use super::runner::DAEMON_STOPPED;
use super::{digest, new_job_id, store, JobKind, JobRecord, JobResult, JobState};
use crate::fs::session_files::save_session;
use crate::fs::work_dir::WorkDir;
use crate::models::session::{Session, SessionStatus, SessionType};
use crate::models::stage::StageStatus;
use crate::process::sandbox_probe::{confined_commands_runnable, skip_unless};
use crate::stage_service::contracts::freeze_with_reports;
use crate::stage_service::{handle_freeze_contracts, Response};
use crate::verify::contracts::store::load_freeze;
use crate::verify::contracts::test_support::{
    contract_stage, red_reports, shell_contract_worktree, write_test_freeze, CONTRACT_FILE,
};
use crate::verify::transitions::{load_stage, save_stage, update_stage};

const STAGE: &str = "s1";
const HARNESS: &str = "harness.txt";

fn confinement_unavailable(test_name: &str) -> bool {
    skip_unless(
        confined_commands_runnable(),
        test_name,
        "loom's confinement cannot start here",
    )
}

/// The stage worktree holding the contract file and a harness file, the
/// stage's running contract session, and one contract whose test is the
/// shell `command`.
struct Fixture {
    _tmp: TempDir,
    work_dir: PathBuf,
    worktree: PathBuf,
    session_id: String,
}

impl Fixture {
    fn new(command: &str) -> Self {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().canonicalize().unwrap().join("repo");
        let worktree = shell_contract_worktree(&repo, STAGE);
        std::fs::write(worktree.join(HARNESS), "harness\n").unwrap();
        let workspace = WorkDir::new(&repo).unwrap();
        workspace.adopt_existing().unwrap();
        let work_dir = workspace.root().to_path_buf();
        let mut session = Session::new();
        session.session_type = SessionType::Contract;
        session.assign_to_stage(STAGE.to_string());
        session.status = SessionStatus::Running;
        save_session(&session, &work_dir).unwrap();
        let mut stage = contract_stage(STAGE, &session.id);
        stage.contracts[0].runner = None;
        stage.contracts[0].test = command.to_string();
        stage.harness = vec![HARNESS.to_string()];
        save_stage(&stage, &work_dir).unwrap();
        Self {
            _tmp: tmp,
            work_dir,
            worktree,
            session_id: session.id,
        }
    }

    /// A current freeze job in `state` for the worktree as it stands now.
    fn job(&self, state: JobState) -> JobRecord {
        let record = JobRecord {
            id: new_job_id(),
            stage_id: STAGE.to_string(),
            session_id: self.session_id.clone(),
            kind: JobKind::Freeze,
            head: digest::head(&self.worktree).unwrap(),
            tree_digest: digest::tree_digest(&self.worktree).unwrap(),
            no_cache: false,
            criteria: None,
            state,
            requests: vec!["r1".to_string()],
            queued_at: Utc::now(),
            started_at: Some(Utc::now()),
            finished_at: None,
            pid: None,
            detail: None,
        };
        store::save_current(&self.work_dir, &record).unwrap();
        record
    }

    /// Run `record` and end it as the runner would from its result.
    fn run(&self, mut record: JobRecord) -> (JobRecord, JobResult) {
        let result = run_freeze_job(&self.work_dir, &record).unwrap();
        record.state = if result.passed {
            JobState::Passed
        } else {
            JobState::Failed
        };
        record.finished_at = Some(Utc::now());
        store::save_current(&self.work_dir, &record).unwrap();
        (record, result)
    }

    fn apply(&self, record: &JobRecord) -> Applied {
        apply_freeze_result(&self.work_dir, record).unwrap()
    }

    fn status(&self) -> StageStatus {
        load_stage(STAGE, &self.work_dir).unwrap().status
    }

    fn frozen(&self) -> bool {
        load_freeze(&self.work_dir, STAGE).unwrap().is_some()
    }

    fn git(&self, args: &[&str]) {
        let output = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args([
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .args(args)
            .current_dir(&self.worktree)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {output:?}");
    }

    /// A passing run, not yet applied.
    fn passed_run(&self) -> JobRecord {
        let (record, result) = self.run(self.job(JobState::Running));
        assert!(result.passed, "{result:?}");
        record
    }
}

fn verdicts(result: &JobResult) -> Vec<(&str, bool)> {
    let outcomes = result.criteria.iter();
    outcomes.map(|o| (o.label.as_str(), o.passed)).collect()
}

fn refused_with(applied: Applied) -> String {
    match applied {
        Applied::FreezeRefused(message) => message,
        other => panic!("expected a refused freeze, got {other:?}"),
    }
}

fn assert_parked(fx: &Fixture, applied: Applied, naming: &str) {
    let message = refused_with(applied);
    assert!(message.contains(naming), "{message}");
    assert_eq!(fx.status(), StageStatus::WaitingForInput);
    let stage = load_stage(STAGE, &fx.work_dir).unwrap();
    let reason = stage.review_reason.unwrap_or_default();
    assert!(reason.contains(naming), "{reason}");
    assert!(!fx.frozen());
}

#[test]
#[serial]
fn red_contracts_pass_and_are_frozen() {
    if confinement_unavailable("red_contracts_pass_and_are_frozen") {
        return;
    }
    let fx = Fixture::new("exit 1");
    let (record, result) = fx.run(fx.job(JobState::Running));
    assert_eq!(
        verdicts(&result),
        [("contract:rejects-x", true), ("format", true)]
    );
    assert_eq!(record.state, JobState::Passed);
    let applied = fx.apply(&record);
    assert!(
        matches!(applied, Applied::FreezeAccepted { files: 2 }),
        "{applied:?}"
    );
    assert!(fx.frozen());
}

#[test]
#[serial]
fn a_green_contract_fails_and_parks_the_stage_naming_it() {
    if confinement_unavailable("a_green_contract_fails_and_parks_the_stage_naming_it") {
        return;
    }
    let fx = Fixture::new("true");
    let (record, result) = fx.run(fx.job(JobState::Running));
    assert_eq!(record.state, JobState::Failed);
    assert_eq!(verdicts(&result)[0], ("contract:rejects-x", false));
    assert_parked(&fx, fx.apply(&record), "rejects-x");
}

#[test]
#[serial]
fn a_tree_changed_before_the_run_is_an_error() {
    let fx = Fixture::new("exit 1");
    let record = fx.job(JobState::Running);
    std::fs::write(fx.worktree.join(CONTRACT_FILE), "#[test]\nfn x() {}\n").unwrap();
    let result = run_freeze_job(&fx.work_dir, &record).unwrap();
    let error = result.error.clone().unwrap_or_default();
    assert!(error.contains("does not match the request"), "{error}");
    assert!(result.criteria.is_empty(), "no contract ran: {result:?}");
}

#[test]
#[serial]
fn a_contract_that_edits_a_harness_file_while_it_runs_is_an_error() {
    if confinement_unavailable("a_contract_that_edits_a_harness_file_while_it_runs_is_an_error") {
        return;
    }
    let fx = Fixture::new(&format!("echo more >> {HARNESS}; exit 1"));
    let (record, result) = fx.run(fx.job(JobState::Running));
    let error = result.error.clone().unwrap_or_default();
    assert!(error.contains("worktree changed during the run"), "{error}");
    assert!(error.contains(HARNESS), "{error}");
    assert_eq!(record.state, JobState::Failed);
}

#[test]
#[serial]
fn contract_files_changed_after_a_passing_run_park_at_apply() {
    if confinement_unavailable("contract_files_changed_after_a_passing_run_park_at_apply") {
        return;
    }
    let fx = Fixture::new("exit 1");
    let record = fx.passed_run();
    std::fs::write(fx.worktree.join(CONTRACT_FILE), "#[test]\nfn y() {}\n").unwrap();
    assert_parked(&fx, fx.apply(&record), CONTRACT_FILE);
}

#[test]
#[serial]
fn a_commit_after_a_passing_run_parks_at_apply() {
    if confinement_unavailable("a_commit_after_a_passing_run_parks_at_apply") {
        return;
    }
    let fx = Fixture::new("exit 1");
    fx.git(&["add", "-A"]);
    fx.git(&["commit", "-q", "-m", "contracts"]);
    let record = fx.passed_run();
    fx.git(&["commit", "-q", "--allow-empty", "-m", "later"]);
    assert_parked(&fx, fx.apply(&record), "HEAD moved");
}

#[test]
#[serial]
fn a_contract_command_amended_after_a_passing_run_parks_at_apply() {
    if confinement_unavailable("a_contract_command_amended_after_a_passing_run_parks_at_apply") {
        return;
    }
    let fx = Fixture::new("exit 1");
    let record = fx.passed_run();
    update_stage(STAGE, &fx.work_dir, |stage| {
        stage.contracts[0].test = "exit 2".to_string();
        Ok(())
    })
    .unwrap();
    assert_parked(&fx, fx.apply(&record), "contract definitions changed");
}

#[test]
#[serial]
fn files_that_differ_from_the_verified_hashes_are_not_frozen() {
    let fx = Fixture::new("exit 1");
    let verified = vec![
        (CONTRACT_FILE.to_string(), "0".repeat(64)),
        (HARNESS.to_string(), "0".repeat(64)),
    ];
    let response = freeze_with_reports(
        &fx.work_dir,
        STAGE,
        &fx.session_id,
        &red_reports(),
        &verified,
    )
    .unwrap();
    let Response::Error { message } = response else {
        panic!("expected a refusal, got {response:?}");
    };
    assert!(
        message.contains("differ from the ones the contract run verified"),
        "{message}"
    );
    assert!(message.contains(CONTRACT_FILE), "{message}");
    assert!(!fx.frozen());
}

#[test]
#[serial]
fn a_superseded_job_is_ignored_and_parks_nothing() {
    let fx = Fixture::new("exit 1");
    let mut record = fx.job(JobState::Cancelled);
    record.detail = Some("superseded by abc".to_string());
    let applied = fx.apply(&record);
    assert!(
        matches!(&applied, Applied::Ignored(why) if why == "cancelled: superseded by abc"),
        "{applied:?}"
    );
    assert_eq!(fx.status(), StageStatus::Executing);
}

#[test]
#[serial]
fn a_job_cancelled_by_the_daemon_shutdown_parks_the_stage() {
    let fx = Fixture::new("exit 1");
    let mut record = fx.job(JobState::Cancelled);
    record.detail = Some(DAEMON_STOPPED.to_string());
    let applied = fx.apply(&record);
    assert_parked(&fx, applied, "the daemon stopped during the contract run");
}

#[test]
#[serial]
fn a_frozen_stage_is_ignored_and_never_parked() {
    let fx = Fixture::new("exit 1");
    write_test_freeze(&fx.work_dir, STAGE, &fx.session_id);
    let mut record = fx.job(JobState::Failed);
    record.detail = Some("deadline of 1 s passed".to_string());
    let applied = fx.apply(&record);
    assert!(
        matches!(&applied, Applied::Ignored(why) if why == "already frozen"),
        "{applied:?}"
    );
    assert_eq!(fx.status(), StageStatus::Executing);
}

#[test]
#[serial]
fn only_the_contract_writer_queues_a_freeze_job() {
    let fx = Fixture::new("exit 1");
    let head = digest::head(&fx.worktree).unwrap();
    let tree = digest::tree_digest(&fx.worktree).unwrap();
    let request = |session: &str, id: &str| {
        handle_freeze_contracts(&fx.work_dir, STAGE, session, &head, &tree, id).unwrap()
    };

    let refused = request("someone-else", "r0");
    assert!(matches!(refused, Response::Error { .. }), "{refused:?}");
    assert!(store::load_current(&fx.work_dir, STAGE).unwrap().is_none());

    assert_eq!(request(&fx.session_id, "r1"), Response::Ok);
    assert_eq!(request(&fx.session_id, "r2"), Response::Ok);
    let current = store::load_current(&fx.work_dir, STAGE).unwrap().unwrap();
    assert_eq!(current.kind, JobKind::Freeze);
    assert_eq!(current.state, JobState::Queued);
    assert_eq!(current.requests, ["r1", "r2"], "the second request joins");
}
