//! Contract: the activity feed of a status payload never carries a stage id
//! or close reason holding a control character; the event log is written by
//! sessions, so it is untrusted at the collector boundary.

use crate::fs::stage_events::{append_event, StageEvent};
use crate::fs::work_dir::WorkDir;
use crate::models::stage::StageStatus;

use super::collect_status_data;

fn blocked(stage_id: &str, reason: &str) -> StageEvent {
    StageEvent::new(
        stage_id,
        StageStatus::Executing,
        StageStatus::Blocked,
        None,
        Some(reason),
    )
}

#[test]
fn a_stage_event_with_a_control_character_in_its_id_is_not_reported() {
    let tmp = tempfile::TempDir::new().unwrap();
    let work_dir = WorkDir::new(tmp.path()).unwrap();
    work_dir.initialize().unwrap();
    let hostile_reason = "x\u{202e}\u{1b}[2J";
    let hostile_id = "bad\u{1b}[2Jid";
    for event in [
        blocked("stage-1", hostile_reason),
        blocked(hostile_id, "r"),
        blocked("a.b", "r"),
    ] {
        append_event(work_dir.root(), &event).unwrap();
    }

    let activity = collect_status_data(&work_dir).unwrap().activity;

    let ids: Vec<&str> = activity.iter().map(|e| e.stage_id.as_str()).collect();
    assert!(
        ids.iter().all(|id| !id.chars().any(char::is_control)),
        "a reported stage id holds a control character: {ids:?}"
    );
    assert_eq!(
        ids,
        ["stage-1"],
        "only the well-formed stage id is reported"
    );
    let reason = activity[0].reason.as_deref().unwrap_or_default();
    assert!(
        !reason.contains('\u{202e}') && !reason.contains('\u{1b}'),
        "the reported close reason keeps a bidi override or ESC: {reason:?}"
    );
}
