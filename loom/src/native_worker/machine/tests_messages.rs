use super::test_support::*;
use super::*;
use crate::native_worker::model::{MessageState, MAX_QUEUED_MESSAGES};
use serde_json::json;

#[test]
fn steer_while_active_persists_dispatching_and_sends_steer_with_active_turn_id() {
    let actions = active().step(steer("s1", "adjust"));
    assert_eq!(
        actions[0],
        Action::Persist(message_event("s1", MessageState::Dispatching, None, None))
    );
    assert_eq!(methods(&actions), ["turn/steer"]);
    let frame = sent(&actions, "turn/steer");
    assert_eq!(frame["params"]["threadId"], "th-1");
    assert_eq!(frame["params"]["expectedTurnId"], "t1");
    assert_eq!(
        frame["params"]["input"],
        json!([{"type": "text", "text": "adjust"}])
    );
}

#[test]
fn steer_response_ok_persists_delivered_with_turn_id() {
    let mut machine = active();
    let actions = machine.step(steer("s1", "adjust"));
    let id = sent(&actions, "turn/steer")["id"].as_u64().unwrap();
    assert_eq!(
        persisted(&machine.step(ok(id, json!({})))),
        [message_event(
            "s1",
            MessageState::Delivered,
            Some("t1"),
            None
        )]
    );
    assert!(machine.step(ok(id, json!({}))).is_empty());
}

#[test]
fn steer_response_error_persists_rejected_with_message() {
    let mut machine = active();
    let actions = machine.step(steer("s1", "adjust"));
    let id = sent(&actions, "turn/steer")["id"].as_u64().unwrap();
    assert_eq!(
        persisted(&machine.step(err(id, -1, "already done"))),
        [message_event(
            "s1",
            MessageState::Rejected,
            None,
            Some("already done")
        )]
    );
}

#[test]
fn steer_before_any_turn_is_rejected_and_sends_nothing() {
    for mut machine in [Machine::new(config()), started(), thread_ready()] {
        assert_eq!(
            machine.step(steer("s1", "adjust")),
            vec![Action::Persist(message_event(
                "s1",
                MessageState::Rejected,
                None,
                Some("no active turn")
            ))]
        );
    }
}

#[test]
fn steer_after_turn_end_is_rejected() {
    let mut machine = active();
    machine.step(next_turn("n1", "next"));
    let actions = machine.step(turn_completed("th-1", "t1", "completed"));
    assert_eq!(methods(&actions), ["turn/start"]);
    assert_eq!(
        machine.step(steer("s1", "adjust")),
        vec![Action::Persist(message_event(
            "s1",
            MessageState::Rejected,
            None,
            Some("no active turn")
        ))]
    );
}

#[test]
fn steer_response_arriving_after_next_turn_still_reports_the_original_turn() {
    let mut machine = active();
    let actions = machine.step(steer("s1", "adjust"));
    let steer_id = sent(&actions, "turn/steer")["id"].as_u64().unwrap();
    machine.step(next_turn("n1", "next"));
    let actions = machine.step(turn_completed("th-1", "t1", "completed"));
    let turn_id = sent(&actions, "turn/start")["id"].as_u64().unwrap();
    machine.step(ok(turn_id, json!({"turn": {"id": "t2"}})));
    assert_eq!(
        persisted(&machine.step(ok(steer_id, json!({})))),
        [message_event(
            "s1",
            MessageState::Delivered,
            Some("t1"),
            None
        )]
    );
    let actions = machine.step(steer("s2", "adjust again"));
    assert_eq!(
        sent(&actions, "turn/steer")["params"]["expectedTurnId"],
        "t2"
    );
}

#[test]
fn steer_while_closed_is_rejected_no_active_turn() {
    let mut machine = active();
    machine.step(Input::Cancel {
        reason: "stop".into(),
    });
    assert_eq!(
        machine.step(steer("s1", "adjust")),
        vec![Action::Persist(message_event(
            "s1",
            MessageState::Rejected,
            None,
            Some("no active turn")
        ))]
    );
}

#[test]
fn next_turn_message_is_queued() {
    assert_eq!(
        active().step(next_turn("n1", "next")),
        vec![Action::Persist(message_event(
            "n1",
            MessageState::Queued,
            None,
            None
        ))]
    );
}

#[test]
fn next_turn_queue_full_is_rejected_at_the_seventeenth() {
    assert_eq!(MAX_QUEUED_MESSAGES, 16);
    let mut machine = active();
    for id in 0..MAX_QUEUED_MESSAGES {
        let id = format!("n{id}");
        assert_eq!(
            persisted(&machine.step(next_turn(&id, "next"))),
            [message_event(&id, MessageState::Queued, None, None)]
        );
    }
    assert_eq!(
        persisted(&machine.step(next_turn("overflow", "next"))),
        [message_event(
            "overflow",
            MessageState::Rejected,
            None,
            Some("queue full")
        )]
    );
}

#[test]
fn next_turn_when_closed_is_rejected_worker_is_closing() {
    let mut machine = active();
    machine.step(Input::Cancel {
        reason: "stop".into(),
    });
    assert_eq!(
        persisted(&machine.step(next_turn("n1", "next"))),
        [message_event(
            "n1",
            MessageState::Rejected,
            None,
            Some("worker is closing")
        )]
    );
}

#[test]
fn duplicate_message_id_is_ignored() {
    let mut machine = thread_ready();
    machine.step(steer("same", "no active turn"));
    assert!(machine.step(next_turn("same", "next")).is_empty());
    machine.step(next_turn("queued", "first"));
    assert!(machine.step(next_turn("queued", "second")).is_empty());
}

#[test]
fn completed_turn_with_queue_sends_one_turn_start_with_one_text_per_message_in_order() {
    let mut machine = active();
    machine.step(next_turn("n1", "first"));
    machine.step(next_turn("n2", "second"));
    let actions = machine.step(turn_completed("th-1", "t1", "completed"));
    assert_eq!(methods(&actions), ["turn/start"]);
    assert_eq!(
        persisted(&actions),
        [
            NativeEvent::TurnCompleted {
                turn_id: "t1".into(),
                status: "completed".into()
            },
            message_event("n1", MessageState::Dispatching, None, None),
            message_event("n2", MessageState::Dispatching, None, None),
        ]
    );
    assert_eq!(
        sent(&actions, "turn/start")["params"]["input"],
        json!([
        {"type": "text", "text": "first"}, {"type": "text", "text": "second"}])
    );
    assert!(matches!(actions.last(), Some(Action::Send(_))));
    assert!(!actions.contains(&Action::ArmDeadline));
    assert!(teardown(&actions).is_none());
}

#[test]
fn queued_messages_are_delivered_when_the_new_turn_starts() {
    let mut machine = active();
    machine.step(next_turn("n1", "first"));
    machine.step(next_turn("n2", "second"));
    let actions = machine.step(turn_completed("th-1", "t1", "completed"));
    let id = sent(&actions, "turn/start")["id"].as_u64().unwrap();
    assert_eq!(
        persisted(&machine.step(turn_started("th-1", "t2"))),
        [
            NativeEvent::TurnStarted {
                turn_id: "t2".into()
            },
            message_event("n1", MessageState::Delivered, Some("t2"), None),
            message_event("n2", MessageState::Delivered, Some("t2"), None),
        ]
    );
    assert!(machine
        .step(ok(id, json!({"turn": {"id": "t2"}})))
        .is_empty());
}

#[test]
fn failed_turn_cancels_queued_messages() {
    let mut machine = active();
    machine.step(next_turn("n1", "next"));
    let actions = machine.step(turn_completed("th-1", "t1", "failed"));
    assert_eq!(
        persisted(&actions),
        [
            NativeEvent::TurnCompleted {
                turn_id: "t1".into(),
                status: "failed".into()
            },
            message_event("n1", MessageState::CancelledBeforeDelivery, None, None),
        ]
    );
    assert_eq!(
        teardown(&actions),
        Some((Outcome::Failed, "turn failed".into()))
    );
    assert!(matches!(actions.last(), Some(Action::Teardown { .. })));
}

#[test]
fn transport_closed_marks_dispatching_unknown_and_queued_cancelled() {
    let mut machine = active();
    machine.step(steer("s1", "adjust"));
    machine.step(next_turn("n1", "next"));
    let actions = machine.step(Input::TransportClosed);
    assert_eq!(
        persisted(&actions),
        [
            message_event("s1", MessageState::DeliveryUnknown, None, None),
            message_event("n1", MessageState::CancelledBeforeDelivery, None, None),
        ]
    );
    assert!(matches!(actions.last(), Some(Action::Teardown { .. })));
}

#[test]
fn pending_turn_delivery_is_unknown_when_transport_closes() {
    let mut machine = active();
    machine.step(next_turn("n1", "next"));
    machine.step(turn_completed("th-1", "t1", "completed"));
    assert_eq!(
        persisted(&machine.step(Input::TransportClosed)),
        [message_event(
            "n1",
            MessageState::DeliveryUnknown,
            None,
            None
        )]
    );
}

#[test]
fn report_uses_only_the_last_completed_turn() {
    let mut machine = active();
    machine.step(agent_item("th-1", "t1", "a", "first", Some("final_answer")));
    machine.step(next_turn("n1", "next"));
    let actions = machine.step(turn_completed("th-1", "t1", "completed"));
    let id = sent(&actions, "turn/start")["id"].as_u64().unwrap();
    machine.step(ok(id, json!({"turn": {"id": "t2"}})));
    machine.step(agent_item("th-1", "t2", "b", "second", None));
    let actions = machine.step(turn_completed("th-1", "t2", "completed"));
    assert_eq!(
        actions[1],
        Action::StoreReport {
            text: "second".into(),
            complete: false
        }
    );
}
