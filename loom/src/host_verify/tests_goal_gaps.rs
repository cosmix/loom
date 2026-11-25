//! A failed goal-backward check reaches the host job's outcomes as one failed
//! outcome per indexed gap, and the job's report then names the dispute
//! routes for them.

use std::time::Duration;

use anyhow::anyhow;

use super::child_complete::record_goal_gaps;
use super::child_progress::Progress;
use super::store::{save_current, save_result};
use super::tests_wait::{fixture, result, OLD_JOB, STAGE};
use super::{CriterionOutcome, JobState};

fn outcome(label: &str, passed: bool) -> CriterionOutcome {
    CriterionOutcome {
        label: label.to_string(),
        passed,
        summary: format!("{label} summary"),
        log: None,
    }
}

#[test]
fn the_host_child_records_each_indexed_gap_beside_a_failed_goal_backward() {
    let fx = fixture();
    let progress = Progress::new(&fx.work_dir, STAGE, OLD_JOB);
    let failed = Err(anyhow!(
        "Goal-backward verification failed for stage 's1': [artifact 1] No files match artifact \
         pattern: x.md; [wiring 0] pattern not found; [artifact 1] stub found"
    ));

    record_goal_gaps(&progress, "goal-backward", &failed);

    let recorded = progress.take_outcomes();
    let labels: Vec<&str> = recorded.iter().map(|o| o.label.as_str()).collect();
    assert_eq!(labels, ["artifact-1", "wiring-0"]);
    assert!(recorded.iter().all(|o| !o.passed), "{recorded:?}");
    assert_eq!(recorded[0].summary, "✗ artifact-1: see goal-backward");
    assert!(
        recorded
            .iter()
            .all(|o| !o.summary.contains("pattern not found")),
        "a gap does not repeat the goal-backward message: {recorded:?}"
    );
}

#[test]
fn only_a_failed_goal_backward_records_gaps() {
    let fx = fixture();
    let progress = Progress::new(&fx.work_dir, STAGE, OLD_JOB);
    let message = "failed: [artifact 1] missing";

    record_goal_gaps(&progress, "goal-backward", &Ok(()));
    record_goal_gaps(&progress, "contracts", &Err(anyhow!(message)));
    record_goal_gaps(
        &progress,
        "goal-backward",
        &Err(anyhow!(
            "Goal-backward verification failed: dead code found"
        )),
    );

    assert!(progress.take_outcomes().is_empty());
}

#[test]
fn a_failed_jobs_report_names_the_goal_gap_routes() {
    let fx = fixture();
    fx.request("r1");
    let failed = fx.job(OLD_JOB, JobState::Failed, &["r1"]);
    save_current(&fx.work_dir, &failed).unwrap();
    let criteria = vec![
        outcome("goal-backward", false),
        outcome("artifact-1", false),
    ];
    save_result(&fx.work_dir, STAGE, &result(&failed, criteria)).unwrap();

    let (_, text) = fx.call(Duration::from_secs(5), false, "r9");

    assert!(text.starts_with("Host job: failed (check, "), "{text}");
    assert!(
        text.contains("--field artifacts --criterion-index"),
        "{text}"
    );
    assert!(text.contains("([artifact n])"), "{text}");
}

#[test]
fn a_failed_job_without_a_goal_backward_failure_prints_no_goal_gap_routes() {
    let fx = fixture();
    fx.request("r1");
    let failed = fx.job(OLD_JOB, JobState::Failed, &["r1"]);
    save_current(&fx.work_dir, &failed).unwrap();
    let criteria = vec![
        outcome("acceptance-0", false),
        outcome("goal-backward", true),
    ];
    save_result(&fx.work_dir, STAGE, &result(&failed, criteria)).unwrap();

    let (_, text) = fx.call(Duration::from_secs(5), false, "r9");

    assert!(!text.contains("--field artifacts"), "{text}");
}
