//! A followed job's report names the log of a criterion that has no result:
//! one running now, or one the job was running when it stopped.

use std::io::Write;
use std::time::Duration;

use super::store::{append_progress, job_dir, open_live_log, save_criterion_log, save_current};
use super::tests_wait::{fixture, Fx, OLD_JOB, STAGE};
use super::wait::{wait_or_request, WaitOutcome};
use super::{CriterionOutcome, JobState};

/// A pending outcome with `done` criteria done and none running.
pub(super) fn pending(done: usize) -> WaitOutcome {
    WaitOutcome::Pending {
        done,
        logs: Vec::new(),
    }
}

/// Call the wait with `budget`, relaying `request_id`.
fn call(fx: &Fx, budget: Duration, request_id: &str) -> (WaitOutcome, String) {
    let mut out = Vec::new();
    let mut relay = |_: &crate::relay::HostJobRequest| Ok(request_id.to_string());
    let outcome = wait_or_request(&fx.site(budget, false), &mut relay, &mut out).unwrap();
    (outcome, String::from_utf8(out).unwrap())
}

/// A job in `state` whose criterion 0 finished with its log and whose
/// criterion 1 has written to its live log and has no outcome.
fn job_mid_criterion(fx: &Fx, state: JobState) -> std::path::PathBuf {
    let (outcome, _) = call(fx, Duration::ZERO, "r1");
    assert!(
        matches!(outcome, WaitOutcome::Requested { .. }),
        "{outcome:?}"
    );
    save_current(&fx.work_dir, &fx.job(OLD_JOB, state, &["r1"])).unwrap();
    let log = save_criterion_log(&fx.work_dir, STAGE, OLD_JOB, "acceptance-0", b"ok").unwrap();
    let finished = CriterionOutcome {
        label: "acceptance-0".to_string(),
        passed: true,
        summary: "ok".to_string(),
        log: Some(log),
    };
    append_progress(&fx.work_dir, STAGE, OLD_JOB, &finished).unwrap();
    let mut live = open_live_log(&fx.work_dir, STAGE, OLD_JOB, "acceptance-1").unwrap();
    live.write_all(b"test slow has been running for over 60 seconds\n")
        .unwrap();
    let dir = job_dir(&fx.work_dir, STAGE, OLD_JOB).unwrap();
    std::path::absolute(dir.join("acceptance-1.log")).unwrap()
}

#[test]
fn a_pending_report_names_the_running_criterions_log() {
    let fx = fixture();
    let log = job_mid_criterion(&fx, JobState::Running);

    let (waited, text) = call(&fx, Duration::ZERO, "r9");

    assert_eq!(
        waited,
        WaitOutcome::Pending {
            done: 1,
            logs: vec![log.clone()]
        }
    );
    let line = format!("acceptance-1 is running; log so far: {}", log.display());
    assert!(text.contains(&line), "{text}");
    assert!(!text.contains("acceptance-0 is running"), "{text}");
}

#[test]
fn a_stopped_report_names_the_log_of_the_criterion_it_was_running() {
    let fx = fixture();
    let log = job_mid_criterion(&fx, JobState::Interrupted);

    let (_, text) = call(&fx, Duration::from_secs(5), "r9");

    assert!(text.starts_with("Host job: stopped (check, "), "{text}");
    let line = format!(
        "acceptance-1 was running when the job stopped; log so far: {}",
        log.display()
    );
    assert!(text.contains(&line), "{text}");
    assert!(!text.contains("acceptance-0 was running"), "{text}");
}
