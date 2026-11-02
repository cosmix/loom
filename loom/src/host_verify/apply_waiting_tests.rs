//! A freeze job's result reaches a writer parked on `AskUserQuestion`: the
//! stage is `WaitingForInput`, the writer still owns it, and the freeze path
//! admits that state.

use serial_test::serial;

use super::super::apply_retry::record_freeze_failure;
use super::super::tests_freeze::{confinement_unavailable, Fixture, STAGE};
use super::*;
use crate::verify::contracts::refusal::daemon_refusal;

fn park_on_input(fx: &Fixture) {
    update_stage(STAGE, &fx.work_dir, |stage| {
        stage.status = StageStatus::WaitingForInput;
        Ok(())
    })
    .unwrap();
}

#[test]
#[serial]
fn a_passing_run_freezes_a_stage_waiting_for_input_and_resumes_it() {
    if confinement_unavailable("a_passing_run_freezes_a_stage_waiting_for_input") {
        return;
    }
    let fx = Fixture::new("exit 1");
    let record = fx.passed_run();
    park_on_input(&fx);

    let applied = apply_finished(&fx.work_dir, &record).unwrap();

    assert!(
        matches!(applied, Applied::FreezeAccepted { .. }),
        "{applied:?}"
    );
    assert!(fx.frozen());
    assert_eq!(fx.status(), StageStatus::Executing);
}

/// The result of a run is not thrown away when the writer is parked: the
/// refusal reaches its record.
#[test]
#[serial]
fn a_failed_run_is_recorded_for_a_stage_waiting_for_input() {
    let fx = Fixture::new("exit 1");
    let mut record = fx.job(JobState::Failed);
    record.detail = Some("the run failed".to_string());
    park_on_input(&fx);

    let applied = apply_finished(&fx.work_dir, &record).unwrap();

    assert!(matches!(applied, Applied::FreezeRefused(_)), "{applied:?}");
    let refusal = daemon_refusal(&fx.work_dir, STAGE).unwrap().unwrap();
    assert_eq!(refusal.session_id, fx.session_id);
}

/// The last application attempt failed: its infrastructure refusal is what
/// the parked writer sees, not silence.
#[test]
#[serial]
fn an_unappliable_run_is_recorded_for_a_stage_waiting_for_input() {
    let fx = Fixture::new("exit 1");
    let record = fx.job(JobState::Failed);
    park_on_input(&fx);

    record_freeze_failure(&fx.work_dir, &record, "disk full").unwrap();

    let refusal = daemon_refusal(&fx.work_dir, STAGE).unwrap().unwrap();
    assert_eq!(refusal.session_id, fx.session_id);
}

/// The positive control for ownership: no other status gets this far.
#[test]
#[serial]
fn a_stage_in_any_other_status_ignores_the_result() {
    let fx = Fixture::new("exit 1");
    let record = fx.job(JobState::Failed);
    update_stage(STAGE, &fx.work_dir, |stage| {
        stage.status = StageStatus::Blocked;
        Ok(())
    })
    .unwrap();

    let applied = apply_finished(&fx.work_dir, &record).unwrap();

    assert!(matches!(applied, Applied::Ignored(_)), "{applied:?}");
    assert_eq!(daemon_refusal(&fx.work_dir, STAGE).unwrap(), None);
}
