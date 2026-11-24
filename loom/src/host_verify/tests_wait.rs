//! The session-side wait protocol, against a real checkout and a temp state
//! root, with job records written through the store.

use std::{cell::RefCell, io::Write, path::PathBuf, rc::Rc, time::Duration};

use chrono::Utc;
use tempfile::TempDir;

use super::digest::{head, tree_digest};
use super::store::{append_progress, save_criterion_log, save_current, save_result};
use super::wait::{exit_code, pending_request_id, wait_or_request, WaitOutcome, WaitSite};
use super::{CriterionOutcome, JobKind, JobRecord, JobResult, JobState};
use crate::fs::inbox::{append_ledger, LedgerOutcome, LedgerRecord};
use crate::models::dispute::FileReplacement;
use crate::relay::{HostJobRequest, RequestKind};
use crate::test_git::{git, TestRepo};

pub(super) const STAGE: &str = "s1";
const SESSION: &str = "session-1";
pub(super) const OLD_JOB: &str = "11111111111111111111111111111111";

pub(super) struct Fx {
    _tmp: TempDir,
    pub(super) checkout: PathBuf,
    pub(super) work_dir: PathBuf,
    scratch: PathBuf,
}

pub(super) fn fixture() -> Fx {
    let tmp = tempfile::tempdir().unwrap();
    let checkout = tmp.path().join("checkout");
    let work_dir = tmp.path().join(".loom").join("work");
    let scratch = tmp.path().join("scratch");
    for dir in [&checkout, &work_dir, &scratch] {
        std::fs::create_dir_all(dir).unwrap();
    }
    TestRepo::builder().no_seed().init_in(&checkout);
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
            criteria: None,
            contracts: None,
            scratch_dir: &self.scratch,
            budget,
            at_base: false,
        }
    }

    pub(super) fn state_file(&self) -> PathBuf {
        self.state_file_for(false)
    }

    pub(super) fn state_file_for(&self, no_cache: bool) -> PathBuf {
        super::wait_state::state_path(&self.site(Duration::ZERO, no_cache)).unwrap()
    }

    pub(super) fn job(&self, id: &str, state: JobState, requests: &[&str]) -> JobRecord {
        JobRecord {
            id: id.to_string(),
            submission_order: 0,
            stage_id: STAGE.to_string(),
            session_id: SESSION.to_string(),
            kind: JobKind::Check,
            head: head(&self.checkout).unwrap(),
            tree_digest: tree_digest(&self.checkout).unwrap(),
            no_cache: false,
            criteria: None,
            contracts: false,
            state,
            requests: requests.iter().map(|id| id.to_string()).collect(),
            queued_at: Utc::now(),
            started_at: None,
            finished_at: None,
            pid: None,
            detail: None,
            at_base: false,
        }
    }

    pub(super) fn ledger(&self, id: &str, outcome: LedgerOutcome, reason: &str) {
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
    pub(super) fn request(&self, request_id: &str) {
        let outcome = self.call(Duration::ZERO, false, request_id).0;
        assert_eq!(
            outcome,
            WaitOutcome::Requested {
                request_id: request_id.into()
            }
        );
    }

    pub(super) fn call(
        &self,
        budget: Duration,
        no_cache: bool,
        request_id: &str,
    ) -> (WaitOutcome, String) {
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

pub(super) fn result(job: &JobRecord, criteria: Vec<CriterionOutcome>) -> JobResult {
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
    assert_eq!(
        outcome,
        WaitOutcome::Requested {
            request_id: "r1".into()
        }
    );
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
    assert_eq!(waited, super::tests_wait_live_log::pending(1));
    assert_eq!(exit_code(&waited), 0);
    assert!(text.starts_with("Host job: pending (check, "), "{text}");
    assert!(text.contains("1 criteria done"), "{text}");
    assert!(fx.state_file().is_file());
}

fn replacement(content: &str) -> [FileReplacement; 1] {
    [FileReplacement {
        file: "a.txt".into(),
        content: content.into(),
        base_sha256: None,
    }]
}

fn rerun(
    first_site: &WaitSite<'_>,
    second_site: &WaitSite<'_>,
    between_calls: impl FnOnce(),
) -> (usize, WaitOutcome) {
    let mut calls = 0;
    let mut relay = |_: &HostJobRequest| {
        calls += 1;
        Ok(format!("r{calls}"))
    };
    let mut out = Vec::new();
    let first = wait_or_request(first_site, &mut relay, &mut out).unwrap();
    assert_eq!(
        first,
        WaitOutcome::Requested {
            request_id: "r1".into()
        }
    );
    between_calls();
    let second = wait_or_request(second_site, &mut relay, &mut out).unwrap();
    (calls, second)
}

#[test]
fn a_trial_rerun_follows_after_the_overlay_changed_the_tree() {
    let fx = fixture();
    let trial = replacement("two\n");
    let site = WaitSite {
        contracts: Some(&trial),
        ..fx.site(Duration::ZERO, false)
    };
    let (calls, second) = rerun(&site, &site, || {
        std::fs::write(fx.checkout.join("a.txt"), &trial[0].content).unwrap();
    });
    assert_eq!(calls, 1);
    assert!(matches!(second, WaitOutcome::Pending { .. }), "{second:?}");
    assert_eq!(pending_request_id(&site).unwrap().as_deref(), Some("r1"));
}

#[test]
fn a_trial_with_other_replacement_bytes_is_a_new_request() {
    let fx = fixture();
    let first = replacement("two\n");
    let second = replacement("three\n");
    let first_site = WaitSite {
        contracts: Some(&first),
        ..fx.site(Duration::ZERO, false)
    };
    let second_site = WaitSite {
        contracts: Some(&second),
        ..fx.site(Duration::ZERO, false)
    };
    assert_ne!(
        super::wait_state::state_path(&first_site).unwrap(),
        super::wait_state::state_path(&second_site).unwrap(),
    );
    let (calls, outcome) = rerun(&first_site, &second_site, || {});
    assert_eq!(calls, 2);
    assert!(
        matches!(outcome, WaitOutcome::Requested { .. }),
        "{outcome:?}"
    );
}

fn assert_edited_tree_starts_a_new_request(contracts: Option<&[FileReplacement]>) {
    let fx = fixture();
    let site = WaitSite {
        contracts,
        ..fx.site(Duration::ZERO, false)
    };
    let (calls, outcome) = rerun(&site, &site, || {
        std::fs::write(fx.checkout.join("a.txt"), "two\n").unwrap();
    });
    assert_eq!(calls, 2);
    assert!(
        matches!(outcome, WaitOutcome::Requested { .. }),
        "{outcome:?}"
    );
}

#[test]
fn a_plain_check_of_an_edited_tree_is_a_new_request() {
    assert_edited_tree_starts_a_new_request(None);
}

#[test]
fn a_plain_contracts_check_keeps_the_digest_rule() {
    assert_edited_tree_starts_a_new_request(Some(&[]));
}
