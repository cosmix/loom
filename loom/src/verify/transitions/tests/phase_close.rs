//! Where a terminal status change closes the open phase span.

use super::*;
use crate::models::stage::{Stage, StageStatus};

/// A stage carrying a closed `Executing` span and an open `Waiting` span that began
/// `wait_gap` after `completed_at`, with status `status` and `completed_at` two hours ago.
fn stage_with_open_wait(
    id: &str,
    status: StageStatus,
    wait_gap: chrono::Duration,
) -> (Stage, chrono::DateTime<chrono::Utc>) {
    use crate::models::stage::phases::{Phase, PhaseSpan};

    let completed = chrono::Utc::now() - chrono::Duration::hours(2);
    let stage = Stage {
        id: id.to_string(),
        status,
        completed_at: Some(completed),
        phases: vec![
            PhaseSpan {
                phase: Phase::Executing,
                started_at: completed - chrono::Duration::minutes(10),
                ended_at: Some(completed),
            },
            PhaseSpan {
                phase: Phase::Waiting,
                started_at: completed + wait_gap,
                ended_at: None,
            },
        ],
        ..Stage::default()
    };
    (stage, completed)
}

#[test]
fn merge_wait_after_completion_closes_at_the_time_of_release() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path();
    let (stage, _) = stage_with_open_wait(
        "held-merge",
        StageStatus::MergeBlocked,
        chrono::Duration::seconds(1),
    );
    create_stage(&stage, work_dir).unwrap();

    let before = chrono::Utc::now() - chrono::Duration::seconds(5);
    let written = update_stage("held-merge", work_dir, |current| {
        current.status = StageStatus::Completed;
        Ok(())
    })
    .unwrap();

    let wait = written.phases.last().unwrap();
    let ended = wait.ended_at.expect("the wait span is closed");
    assert!(
        ended >= before,
        "closed at {ended}, not at the stale completion"
    );
    assert!(ended > wait.started_at);
}

#[test]
fn terminal_close_during_the_open_span_stamps_the_completion_time() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path();
    // The open span began before `completed_at`: the completion time still bounds it.
    let (stage, completed) = stage_with_open_wait(
        "normal-close",
        StageStatus::MergeBlocked,
        chrono::Duration::seconds(-30),
    );
    create_stage(&stage, work_dir).unwrap();

    let written = update_stage("normal-close", work_dir, |current| {
        current.status = StageStatus::Completed;
        Ok(())
    })
    .unwrap();

    assert_eq!(written.phases.last().unwrap().ended_at, Some(completed));
}
