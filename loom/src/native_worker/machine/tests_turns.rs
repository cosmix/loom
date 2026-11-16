use super::test_support::*;
use super::*;
use serde_json::json;

#[test]
fn completion_before_turn_started_learns_and_completes_once() {
    let mut machine = thread_ready();
    let actions = machine.step(turn_completed("th-1", "t1", "completed"));
    assert_eq!(
        persisted(&actions),
        [
            NativeEvent::TurnStarted {
                turn_id: "t1".into()
            },
            NativeEvent::TurnCompleted {
                turn_id: "t1".into(),
                status: "completed".into()
            }
        ]
    );
    assert_eq!(
        teardown(&actions),
        Some((Outcome::Succeeded, "turn completed".into()))
    );
    assert!(machine
        .step(ok(3, json!({"turn": {"id": "t1"}})))
        .is_empty());
}

#[test]
fn turn_response_without_id_tears_down_failed() {
    assert_eq!(
        teardown(&thread_ready().step(ok(3, json!({})))),
        Some((
            Outcome::Failed,
            "turn/start response missing turn.id".into()
        ))
    );
}

#[test]
fn malformed_notification_and_completion_for_other_turn_are_ignored() {
    let mut machine = active();
    assert!(machine.step(notify("turn/completed", json!({}))).is_empty());
    assert!(machine
        .step(turn_completed("th-1", "other", "completed"))
        .is_empty());
    assert!(!machine.is_finished());
}
