//! Following the daemon's application of a finished freeze job.

use std::path::PathBuf;
use std::time::Duration;

use chrono::Utc;
use tempfile::TempDir;

use super::*;
use crate::host_verify::wait::wait_or_request;
use crate::host_verify::JobState;
use crate::relay::HostJobRequest;
use crate::verify::contracts::test_support::contract_worktree;

const JOB: &str = "55555555555555555555555555555555";

struct Fx {
    _temp: TempDir,
    work_dir: PathBuf,
    checkout: PathBuf,
    scratch: PathBuf,
}

/// A real checkout and a freeze request "r1" waiting on it.
fn fixture() -> Fx {
    let temp = TempDir::new().unwrap();
    let repo_root = temp.path().join("repo");
    let checkout = contract_worktree(&repo_root, "s1");
    let scratch = temp.path().join("scratch");
    std::fs::create_dir_all(&scratch).unwrap();
    let fx = Fx {
        work_dir: repo_root.join(".loom").join("work"),
        checkout,
        scratch,
        _temp: temp,
    };
    let mut relay = |_: &HostJobRequest| Ok("r1".to_string());
    wait_or_request(&fx.site(), &mut relay, &mut Vec::new()).unwrap();
    assert!(fx.state_file().is_file());
    fx
}

impl Fx {
    fn site(&self) -> WaitSite<'_> {
        WaitSite {
            work_dir: &self.work_dir,
            stage_id: "s1",
            session_id: "session-1",
            checkout: &self.checkout,
            knowledge: false,
            kind: JobKind::Freeze,
            no_cache: false,
            criteria: None,
            scratch_dir: &self.scratch,
            budget: Duration::ZERO,
        }
    }

    fn state_file(&self) -> PathBuf {
        self.scratch.join("host-verify").join("s1-freeze.json")
    }

    fn record(&self) -> JobRecord {
        JobRecord {
            id: JOB.to_string(),
            submission_order: 0,
            stage_id: "s1".to_string(),
            session_id: "session-1".to_string(),
            kind: JobKind::Freeze,
            head: crate::host_verify::digest::head(&self.checkout).unwrap(),
            tree_digest: String::new(),
            no_cache: false,
            criteria: None,
            state: JobState::Failed,
            requests: vec!["r1".to_string()],
            queued_at: Utc::now(),
            started_at: None,
            finished_at: None,
            pid: None,
            detail: None,
        }
    }

    fn applied(&self, note: &str) {
        store::save_job_file(&self.work_dir, "s1", JOB, APPLIED_MARKER, note.as_bytes()).unwrap();
    }

    /// Follow with no time to wait, returning the exit code and the printed lines.
    fn follow(&self) -> (i32, String) {
        let mut out = Vec::new();
        let code =
            follow_application(&self.site(), &self.record(), Duration::ZERO, &mut out).unwrap();
        (code, String::from_utf8(out).unwrap())
    }
}

#[test]
fn a_freeze_the_daemon_applied_ends_the_session_and_releases_the_request() {
    let fx = fixture();
    fx.applied("froze the contracts (2 files)");

    let (code, text) = fx.follow();
    assert_eq!(code, 0);
    assert!(
        text.starts_with("froze the contracts (2 files)\n"),
        "{text}"
    );
    assert!(text.contains("Stop now; loom ends this session."), "{text}");
    assert!(!fx.state_file().exists());
}

#[test]
fn a_refusal_returned_to_the_writer_exits_one_and_files_nothing() {
    let fx = fixture();
    fx.applied("refused the contract run: returned to the contract writer (1 of 3): fmt\nfix it");

    let (code, text) = fx.follow();
    assert_eq!(code, 1);
    assert_eq!(
        text,
        "refused the contract run: returned to the contract writer (1 of 3): fmt fix it\n"
    );
    assert!(!fx.state_file().exists());
}

#[test]
fn any_other_note_is_a_filed_refusal() {
    let fx = fixture();
    fx.applied("refused the contract run: the tree moved");

    let (code, text) = fx.follow();
    assert_eq!(code, 1);
    assert!(text.contains("the tree moved"), "{text}");
    assert!(text.contains("filed for an adjudicator"), "{text}");
    assert!(!fx.state_file().exists());
}

#[test]
fn an_ignored_run_files_nothing_and_says_so() {
    for note in ["already frozen", "a newer freeze refusal is recorded"] {
        let fx = fixture();
        fx.applied(note);

        let (code, text) = fx.follow();
        assert_eq!(code, 1, "{note}");
        assert!(text.starts_with(&format!("{note}\n")), "{text}");
        assert!(text.contains("filed nothing"), "{text}");
        assert!(!text.contains("filed for an adjudicator"), "{text}");
        assert!(!fx.state_file().exists());
    }
}

#[test]
fn without_an_applied_marker_the_wait_ends_pending_and_keeps_the_request() {
    let fx = fixture();

    let (code, text) = fx.follow();
    assert_eq!(code, 0);
    assert!(text.starts_with("Host job: pending (freeze, "), "{text}");
    assert!(text.contains("the daemon has not applied it yet"), "{text}");
    assert!(fx.state_file().is_file());
}
