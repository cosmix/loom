use super::*;
use crate::models::stage::StageStatus;
use chrono::Utc;

fn blocked_event(stage_id: &str) -> StageEvent {
    StageEvent {
        at: Utc::now(),
        stage_id: stage_id.into(),
        from: StageStatus::Executing,
        to: StageStatus::Blocked,
        session: Some("s\u{1b}[31m1".into()),
        session_type: None,
        reason: Some("\u{202e}x\u{1b}[2J".into()),
    }
}

#[test]
fn a_stage_event_reason_and_session_are_neutralised() {
    let event = sanitized_stage_event(blocked_event("stage-1")).unwrap();
    assert!(!event.reason.unwrap().contains(['\u{202e}', '\u{1b}']));
    assert!(!event.session.unwrap().contains(['\u{202e}', '\u{1b}']));
}

#[test]
fn a_stage_event_with_a_control_character_in_its_id_is_dropped() {
    assert!(sanitized_stage_event(blocked_event("stage-\u{1b}[2J")).is_none());
    assert!(sanitized_stage_event(blocked_event("a.b")).is_none());
    assert!(sanitized_stage_event(blocked_event("stage-1")).is_some());
}
