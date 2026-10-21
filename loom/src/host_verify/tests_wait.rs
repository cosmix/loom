//! The session-side wait protocol, against a real checkout and a temp state
//! root, with job records written through the store.

use std::cell::RefCell;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::time::Duration;

use chrono::Utc;
use tempfile::TempDir;

use super::digest::{head, tree_digest};
use super::store::{append_progress, save_criterion_log, save_current, save_result, stage_dir};
use super::wait::{exit_code, wait_or_request, WaitOutcome, WaitSite};
use super::{CriterionOutcome, JobKind, JobRecord, JobResult, JobState};
use crate::fs::inbox::{append_ledger, LedgerOutcome, LedgerRecord};
use crate::relay::RequestKind;

const STAGE: &str = "s1";
const SESSION: &str = "session-1";
const OLD_JOB: &str = "11111111111111111111111111111111";
const NEW_JOB: &str = "22222222222222222222222222222222";

pub(super) struct Fx {
    _tmp: TempDir,
    pub(super) checkout: PathBuf,
    work_dir: PathBuf,
    scratch: PathBuf,
}

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(["-c", "commit.gpgsign=false", "-c", "user.name=t"])
        .args(["-c", "user.email=t@t"])
        .args(args)
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
}

pub(super) fn fixture() -> Fx {
    let tmp = tempfile::tempdir().unwrap();
    let checkout = tmp.path().join("checkout");
    let work_dir = tmp.path().join(".loom").join("work");
    let scratch = tmp.path().join("scratch");
    for dir in [&checkout, &work_dir, &scratch] {
        std::fs::create_dir_all(dir).unwrap();
    }
    git(&checkout, &["init", "-q", "-b", "main"]);
    std::fs::write(checkout.join("a.txt"), "one\n").unwrap();
    git(&checkout, &["add", "."]);
    git(&checkout, &["commit", "-q", "-m", "base"]);
    Fx {
        _tmp: tmp,
        checkout,
        work_dir,
        scratch,
    }
}

impl Fx {
    pub(super) fn site(&self, budget: Duration, no_cache: bool) -> WaitSite<'_> {
        WaitSite {
            work_dir: &self.work_dir,
            stage_id: STAGE,
            session_id: SESSION,
            checkout: &self.checkout,
            knowledge: false,
            kind: JobKind::Check,
            no_cache,
            scratch_dir: &self.scratch,
            budget,
        }
    }

    pub(super) fn state_file(&self) -> PathBuf {
        self.scratch.join("host-verify").join("s1-check.json")
    }

    fn job(&self, id: &str, state: JobState, requests: &[&str]) -> JobRecord {
        JobRecord {
            id: id.to_string(),
            stage_id: STAGE.to_string(),
            session_id: SESSION.to_string(),
            kind: JobKind::Check,
            head: head(&self.checkout).unwrap(),
            tree_digest: tree_digest(&self.checkout).unwrap(),
            no_cache: false,
            state,
            requests: requests.iter().map(|id| id.to_string()).collect(),
            queued_at: Utc::now(),
            started_at: None,
            finished_at: None,
            pid: None,
            detail: None,
        }
    }

    fn ledger(&self, id: &str, outcome: LedgerOutcome, reason: &str) {
        let record = LedgerRecord {
            id: id.to_string(),
            kind: RequestKind::Check,
            state: None,
            outcome: Some(outcome),
            reason: Some(reason.to_string()),
            at: Utc::now(),
        };
        append_ledger(&self.work_dir, SESSION, &record).unwrap();
    }

    /// Run a first call that relays `request_id`, leaving the state file.
    fn request(&self, request_id: &str) {
        let outcome = self.call(Duration::ZERO, false, request_id).0;
        assert_eq!(outcome, WaitOutcome::Requested);
    }

    fn call(&self, budget: Duration, no_cache: bool, request_id: &str) -> (WaitOutcome, String) {
        let mut out = Vec::new();
        let mut relay = |_: &crate::relay::HostJobRequest| Ok(request_id.to_string());
        let outcome = wait_or_request(&self.site(budget, no_cache), &mut relay, &mut out).unwrap();
        (outcome, String::from_utf8(out).unwrap())
    }
}

fn outcome(label: &str, passed: bool, summary: &str, log: Option<String>) -> CriterionOutcome {
    CriterionOutcome {
        label: label.to_string(),
        passed,
        summary: summary.to_string(),
        log,
    }
}

fn result(job: &JobRecord, criteria: Vec<CriterionOutcome>) -> JobResult {
    JobResult {
        job_id: job.id.clone(),
        head: job.head.clone(),
        passed: criteria.iter().all(|c| c.passed),
        criteria,
        error: None,
        policy_hash: None,
    }
}

#[derive(Clone, Default)]
struct Shared(Rc<RefCell<Vec<u8>>>);

impl Write for Shared {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn with_no_state_the_request_lines_print_once_the_request_is_recorded() {
    let fx = fixture();
    let out = Shared::default();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let mut calls = 0;
    let mut relay = |_: &crate::relay::HostJobRequest| {
        calls += 1;
        *seen.borrow_mut() = out.0.borrow().clone();
        Ok("r1".to_string())
    };
    let mut sink = out.clone();
    let outcome = wait_or_request(&fx.site(Duration::ZERO, false), &mut relay, &mut sink).unwrap();
    assert_eq!(outcome, WaitOutcome::Requested);
    assert_eq!(exit_code(&outcome), 0);
    assert_eq!(calls, 1);
    assert!(
        seen.borrow().is_empty(),
        "nothing prints before the request"
    );
    let printed = String::from_utf8(out.0.borrow().clone()).unwrap();
    assert!(
        printed.starts_with("Host job: requested (check, "),
        "{printed}"
    );
    assert!(printed.contains("This is not a result."), "{printed}");
    assert!(fx.state_file().is_file());
}

#[test]
fn a_running_job_that_passes_mid_wait_finishes_and_clears_the_state() {
    let fx = fixture();
    fx.request("r1");
    save_current(&fx.work_dir, &fx.job(OLD_JOB, JobState::Running, &["r1"])).unwrap();
    let passed = fx.job(OLD_JOB, JobState::Passed, &["r1"]);
    let work_dir = fx.work_dir.clone();
    let finisher = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        let done = vec![outcome("acceptance-0", true, "ok", None)];
        save_result(&work_dir, STAGE, &result(&passed, done)).unwrap();
        save_current(&work_dir, &passed).unwrap();
    });
    let (waited, text) = fx.call(Duration::from_secs(10), false, "r9");
    finisher.join().unwrap();
    assert!(
        matches!(&waited, WaitOutcome::Finished(job, Some(_)) if job.state == JobState::Passed)
    );
    assert_eq!(exit_code(&waited), 0);
    assert!(text.starts_with("Host job: passed (check, "), "{text}");
    assert!(text.contains("✓ acceptance-0"), "{text}");
    assert!(!fx.state_file().exists());
}

#[test]
fn a_failed_job_exits_one_and_prints_the_failed_summary_with_its_log() {
    let fx = fixture();
    fx.request("r1");
    let failed = fx.job(OLD_JOB, JobState::Failed, &["r1"]);
    save_current(&fx.work_dir, &failed).unwrap();
    let log = save_criterion_log(&fx.work_dir, STAGE, OLD_JOB, "acceptance-1", b"boom").unwrap();
    let criteria = vec![
        outcome("acceptance-0", true, "fine", None),
        outcome("acceptance-1", false, "assertion failed", Some(log)),
    ];
    save_result(&fx.work_dir, STAGE, &result(&failed, criteria)).unwrap();
    let (waited, text) = fx.call(Duration::from_secs(5), false, "r9");
    assert_eq!(exit_code(&waited), 1);
    assert!(text.starts_with("Host job: failed (check, "), "{text}");
    assert!(text.contains("assertion failed"), "{text}");
    assert!(text.contains("full log: /"), "{text}");
    assert!(text.contains("acceptance-1.log"), "{text}");
    assert!(text.contains("✗ acceptance-1"), "{text}");
    assert!(!fx.state_file().exists());
}

#[test]
fn a_cancelled_job_exits_four() {
    let fx = fixture();
    fx.request("r1");
    let mut cancelled = fx.job(OLD_JOB, JobState::Cancelled, &["r1"]);
    cancelled.detail = Some("takedown".to_string());
    save_current(&fx.work_dir, &cancelled).unwrap();
    let (waited, text) = fx.call(Duration::from_secs(5), false, "r9");
    assert_eq!(exit_code(&waited), 4);
    assert!(text.starts_with("Host job: stopped (check, "), "{text}");
    assert!(text.contains("takedown"), "{text}");
    assert!(
        text.contains("Run the same command again to request a new run."),
        "{text}"
    );
}

#[test]
fn an_elapsed_budget_is_pending_and_exits_zero() {
    let fx = fixture();
    fx.request("r1");
    save_current(&fx.work_dir, &fx.job(OLD_JOB, JobState::Running, &["r1"])).unwrap();
    append_progress(
        &fx.work_dir,
        STAGE,
        OLD_JOB,
        &outcome("acceptance-0", true, "ok", None),
    )
    .unwrap();
    let (waited, text) = fx.call(Duration::ZERO, false, "r9");
    assert_eq!(waited, WaitOutcome::Pending { done: 1 });
    assert_eq!(exit_code(&waited), 0);
    assert!(text.starts_with("Host job: pending (check, "), "{text}");
    assert!(text.contains("1 criteria done"), "{text}");
    assert!(fx.state_file().is_file());
}

#[test]
fn a_superseded_job_is_followed_through_its_own_record_and_reported_stopped() {
    let fx = fixture();
    fx.request("r1");
    fx.ledger(
        "r1",
        LedgerOutcome::Applied,
        &format!("host job {OLD_JOB} queued"),
    );
    let mut old = fx.job(OLD_JOB, JobState::Cancelled, &["r1"]);
    old.detail = Some(format!("superseded by {NEW_JOB}"));
    save_current(&fx.work_dir, &old).unwrap();
    save_current(&fx.work_dir, &fx.job(NEW_JOB, JobState::Queued, &["r2"])).unwrap();
    let (waited, text) = fx.call(Duration::from_secs(5), false, "r9");
    assert!(matches!(&waited, WaitOutcome::Finished(job, _) if job.id == OLD_JOB));
    assert_eq!(exit_code(&waited), 4);
    assert!(text.contains(NEW_JOB), "{text}");
}

#[test]
fn a_job_whose_directory_was_pruned_is_stopped() {
    let fx = fixture();
    fx.request("r1");
    fx.ledger(
        "r1",
        LedgerOutcome::Applied,
        &format!("host job {OLD_JOB} queued"),
    );
    std::fs::create_dir_all(stage_dir(&fx.work_dir, STAGE).unwrap()).unwrap();
    let (waited, _) = fx.call(Duration::from_secs(5), false, "r9");
    assert_eq!(exit_code(&waited), 4);
}

#[test]
fn a_refused_ledger_entry_is_reported_and_clears_the_state() {
    let fx = fixture();
    fx.request("r1");
    fx.ledger("r1", LedgerOutcome::Refused, "not the active session");
    let (waited, text) = fx.call(Duration::from_secs(5), false, "r9");
    assert_eq!(
        waited,
        WaitOutcome::Refused("not the active session".to_string())
    );
    assert_eq!(exit_code(&waited), 1);
    assert!(text.starts_with("Host job: refused (check, "), "{text}");
    assert!(text.contains("not the active session"), "{text}");
    assert!(!fx.state_file().exists());
}

#[test]
fn a_changed_no_cache_or_tree_starts_a_new_request() {
    let fx = fixture();
    fx.request("r1");
    let (waited, text) = fx.call(Duration::from_secs(5), true, "r2");
    assert_eq!(waited, WaitOutcome::Requested);
    assert!(text.starts_with("Host job: requested"), "{text}");
    assert!(std::fs::read_to_string(fx.state_file())
        .unwrap()
        .contains("r2"));

    std::fs::write(fx.checkout.join("new.txt"), "x\n").unwrap();
    let (waited, _) = fx.call(Duration::from_secs(5), true, "r3");
    assert_eq!(waited, WaitOutcome::Requested);
    assert!(std::fs::read_to_string(fx.state_file())
        .unwrap()
        .contains("r3"));
}

#[test]
fn a_terminal_outcome_leaves_a_newer_request_the_other_waiter_wrote() {
    let fx = fixture();
    fx.request("r1");
    save_current(&fx.work_dir, &fx.job(OLD_JOB, JobState::Running, &["r1"])).unwrap();
    let passed = fx.job(OLD_JOB, JobState::Passed, &["r1"]);
    let state = std::fs::read_to_string(fx.state_file())
        .unwrap()
        .replace("r1", "r2");
    let (work_dir, state_file) = (fx.work_dir.clone(), fx.state_file());
    let other_waiter = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        std::fs::write(state_file, state).unwrap();
        save_result(&work_dir, STAGE, &result(&passed, Vec::new())).unwrap();
        save_current(&work_dir, &passed).unwrap();
    });
    let (waited, _) = fx.call(Duration::from_secs(10), false, "r9");
    other_waiter.join().unwrap();
    assert!(matches!(waited, WaitOutcome::Finished(..)));
    assert!(std::fs::read_to_string(fx.state_file())
        .unwrap()
        .contains("r2"));
}
