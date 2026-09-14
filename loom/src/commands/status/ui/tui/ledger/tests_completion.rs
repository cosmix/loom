use super::cells::activity_cell;
use super::tests::{make_blocker, make_stage};
use crate::commands::status::data::{ActivityStatus, CompletionBlockerState, SessionExitReason};
use crate::models::stage::StageStatus;

#[test]
fn blocker_activity_beats_stale_and_incoherent_execution() {
    let mut stage = make_stage("blocked-work", StageStatus::Executing);
    stage.activity_status = ActivityStatus::Stale;
    stage.completion_blocker = Some(make_blocker(
        CompletionBlockerState::Pending,
        "boundary failed",
    ));
    let expected = stage.completion_blocker.as_ref().unwrap().activity_text();
    assert_eq!(activity_cell(&stage, 80).text, expected);

    stage.incoherence = Some("wrong session type".to_owned());
    assert_eq!(activity_cell(&stage, 80).text, expected);
}

#[test]
fn blocked_review_shows_completion_blocker() {
    let mut stage = make_stage("review", StageStatus::NeedsHumanReview);
    stage.completion_blocker = Some(make_blocker(
        CompletionBlockerState::Blocked,
        "still failing",
    ));
    assert_eq!(
        activity_cell(&stage, 80).text,
        stage.completion_blocker.unwrap().activity_text()
    );
}

#[test]
fn requeued_stage_distinguishes_stall_from_context_ceiling() {
    let mut stage = make_stage("retry", StageStatus::Queued);
    stage.outgoing_session_exit_reason = Some(SessionExitReason::Stalled);
    let stalled = activity_cell(&stage, 80).text;
    stage.outgoing_session_exit_reason = Some(SessionExitReason::ContextCeiling);
    let ceiling = activity_cell(&stage, 80).text;
    assert_eq!(
        stalled,
        format!(
            "ready · last: {}",
            SessionExitReason::Stalled.status_label()
        )
    );
    assert_eq!(
        ceiling,
        format!(
            "ready · last: {}",
            SessionExitReason::ContextCeiling.status_label()
        )
    );
    assert_ne!(stalled, ceiling);
}
