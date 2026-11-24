//! Contract for E3: a current record left `Running` with no child, which no
//! runner holds, is settled as failed on the next tick.

use chrono::Utc;

use super::runner::HostJobRunner;
use super::store::{load_current, save_current};
use super::tests_runner::{queue, shell, state_root, PASS, STAGE};
use super::JobState;

#[test]
fn a_running_job_no_runner_holds_fails_on_the_next_tick() {
    let (_dir, work) = state_root();
    let mut record = queue(&work, STAGE, Utc::now());
    record.state = JobState::Running;
    record.started_at = Some(Utc::now());
    record.pid = None;
    save_current(&work, &record).unwrap();

    let mut runner = HostJobRunner::new(work.clone(), shell(PASS, 1));
    let settled = runner.tick().unwrap();
    let ticked = settled
        .iter()
        .find(|job| job.id == record.id)
        .unwrap_or_else(|| panic!("the tick did not settle the stranded job: {settled:?}"));
    assert_eq!(ticked.state, JobState::Failed, "{ticked:?}");

    let current = load_current(&work, STAGE)
        .unwrap()
        .expect("a current record");
    assert_eq!(current.id, record.id);
    assert_eq!(current.state, JobState::Failed, "{current:?}");
    assert_eq!(current.detail.as_deref(), Some("verify-host never started"));
}
