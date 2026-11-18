//! Waiting checks survive busy slots and daemon restarts.
use super::runner::{fail_queued, HostJobRunner};
use super::submit::submit;
use super::tests_runner::{
    queue, running, shell, state_root, tick_until, write_stage, PASS, STAGE,
};
use super::tests_submit::{queued, subset, waiting};
use super::{signals, store, JobState};
use chrono::Utc;

#[test]
fn a_waiting_check_runs_after_the_running_check_ends() {
    let (_tmp, work) = state_root();
    let first = queued(submit(&work, &subset("r1", &[1]), Utc::now()).unwrap());
    let script = format!(r#"while [ ! -e "$LOOM_WORK_DIR/release" ]; do sleep 0.05; done; {PASS}"#);
    let mut runner = HostJobRunner::new(work.clone(), shell(&script, 2));
    tick_until(&mut runner, &work, STAGE, running);
    let (next, _) = waiting(submit(&work, &subset("r2", &[2]), Utc::now()).unwrap());
    std::fs::write(work.join("release"), "").unwrap();
    let ended = tick_until(&mut runner, &work, STAGE, |r| {
        r.id == next.id && r.state == JobState::Passed
    });
    assert_eq!(ended.criteria, Some(vec![2]));
    assert_eq!(
        store::load_job_record(&work, STAGE, &first.id)
            .unwrap()
            .unwrap()
            .state,
        JobState::Passed
    );
    assert_eq!(store::load_pending(&work, STAGE).unwrap(), None);
}

#[test]
fn a_waiting_check_becomes_current_while_every_slot_is_busy() {
    let (_tmp, work) = state_root();
    queued(submit(&work, &subset("r1", &[1]), Utc::now()).unwrap());
    let script =
        format!(r#"while [ ! -e "$LOOM_WORK_DIR/release-$1" ]; do sleep 0.05; done; {PASS}"#);
    let mut runner = HostJobRunner::new(work.clone(), shell(&script, 1));
    tick_until(&mut runner, &work, STAGE, running);
    write_stage(&work, "stage-b");
    queue(&work, "stage-b", Utc::now() - chrono::Duration::seconds(1));
    let (next, _) = waiting(submit(&work, &subset("r2", &[2]), Utc::now()).unwrap());
    std::fs::write(work.join("release-stage-a"), "").unwrap();
    tick_until(&mut runner, &work, "stage-b", running);
    let current = store::load_current(&work, STAGE).unwrap().unwrap();
    assert_eq!(current.id, next.id);
    assert_eq!(current.state, JobState::Queued);
    assert_eq!(store::load_pending(&work, STAGE).unwrap(), None);
    // Exercise promotion while a slot was already occupied before this tick.
    store::save_pending(&work, &next).unwrap();
    let mut ended = current;
    ended.id = "99999999999999999999999999999999".into();
    ended.state = JobState::Passed;
    store::save_current(&work, &ended).unwrap();
    runner.tick().unwrap();
    assert_eq!(
        store::load_current(&work, STAGE).unwrap().unwrap().id,
        next.id
    );
    assert_eq!(store::load_pending(&work, STAGE).unwrap(), None);
}

#[test]
fn a_waiting_check_survives_a_restart() {
    let (_tmp, work) = state_root();
    let mut first = queued(submit(&work, &subset("r1", &[1]), Utc::now()).unwrap());
    first.state = JobState::Running;
    first.started_at = Some(Utc::now());
    store::save_current(&work, &first).unwrap();
    let (next, _) = waiting(submit(&work, &subset("r2", &[2]), Utc::now()).unwrap());
    signals::recover_after_restart(&work).unwrap();
    assert_eq!(
        store::load_current(&work, STAGE).unwrap().unwrap().state,
        JobState::Interrupted
    );
    let mut runner = HostJobRunner::new(work.clone(), shell(PASS, 1));
    tick_until(&mut runner, &work, STAGE, |r| {
        r.id == next.id && r.state == JobState::Passed
    });
    assert_eq!(store::load_pending(&work, STAGE).unwrap(), None);
}

#[test]
fn with_no_runner_a_waiting_check_fails_with_the_reason() {
    let (_tmp, work) = state_root();
    let mut first = queued(submit(&work, &subset("r1", &[1]), Utc::now()).unwrap());
    let (next, _) = waiting(submit(&work, &subset("r2", &[2]), Utc::now()).unwrap());
    first.state = JobState::Passed;
    store::save_current(&work, &first).unwrap();
    let failed = fail_queued(&work, "no verify-host runner: x").unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].id, next.id);
    assert_eq!(failed[0].state, JobState::Failed);
    assert_eq!(
        failed[0].detail.as_deref(),
        Some("no verify-host runner: x")
    );
    assert_eq!(store::load_pending(&work, STAGE).unwrap(), None);
}
