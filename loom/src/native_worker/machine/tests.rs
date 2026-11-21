use super::test_support::*;
use super::*;
use serde_json::json;

#[test]
fn start_sends_initialize_with_id_1() {
    let mut machine = Machine::new(config());
    assert_eq!(
        machine.step(Input::Start),
        vec![
            Action::ArmStartupDeadline,
            Action::Send(wire::initialize(1, "test-version"))
        ]
    );
    assert!(machine.step(Input::Start).is_empty());
}

#[test]
fn initialize_ok_sends_initialized_then_thread_start() {
    let actions = started().step(ok(1, json!({})));
    assert_eq!(methods(&actions), ["initialized", "thread/start"]);
    assert_eq!(actions[0], Action::Send(wire::initialized()));
    assert_eq!(
        actions[1],
        Action::Send(wire::thread_start(
            2,
            &config().model,
            &config().cwd,
            wire::DEVELOPER_INSTRUCTIONS,
            config().sandbox,
        ))
    );
}

#[test]
fn initialize_error_tears_down_failed() {
    assert_eq!(
        teardown(&started().step(err(1, -1, "unavailable"))),
        Some((Outcome::Failed, "initialize failed: unavailable".into()))
    );
}

#[test]
fn transport_closed_awaiting_initialize_fails() {
    assert_eq!(
        teardown(&started().step(Input::TransportClosed)),
        Some((Outcome::Failed, "codex app-server closed its output".into()))
    );
}

#[test]
fn thread_ok_persists_thread_started_and_sends_first_turn_with_task_text_and_arms_deadline() {
    let mut machine = started();
    machine.step(ok(1, json!({})));
    let actions = machine.step(ok(2, json!({"thread": {"id": "th-1"}})));
    assert_eq!(
        actions,
        vec![
            Action::Persist(NativeEvent::ThreadStarted {
                thread_id: "th-1".into()
            }),
            Action::Send(wire::turn_start(
                3,
                "th-1",
                &[config().task_text],
                "high",
                &config().cwd,
                config().sandbox,
            )),
            Action::ArmDeadline,
        ]
    );
}

#[test]
fn thread_error_tears_down_failed() {
    let mut machine = started();
    machine.step(ok(1, json!({})));
    assert_eq!(
        teardown(&machine.step(err(2, -1, "bad thread"))),
        Some((Outcome::Failed, "thread/start failed: bad thread".into()))
    );
}

#[test]
fn thread_response_without_id_tears_down_failed() {
    let mut machine = started();
    machine.step(ok(1, json!({})));
    assert_eq!(
        teardown(&machine.step(ok(2, json!({})))),
        Some((
            Outcome::Failed,
            "thread/start response missing thread.id".into()
        ))
    );
}

#[test]
fn turn_response_persists_turn_started_once_even_with_later_notification() {
    let mut machine = thread_ready();
    assert_eq!(
        persisted(&machine.step(ok(3, json!({"turn": {"id": "t1"}})))),
        [NativeEvent::TurnStarted {
            turn_id: "t1".into()
        }]
    );
    assert!(machine.step(turn_started("th-1", "t1")).is_empty());
}

#[test]
fn turn_started_notification_first_then_response_persists_once() {
    let mut machine = thread_ready();
    assert_eq!(
        persisted(&machine.step(turn_started("th-1", "t1"))),
        [NativeEvent::TurnStarted {
            turn_id: "t1".into()
        }]
    );
    assert!(machine
        .step(ok(3, json!({"turn": {"id": "t1"}})))
        .is_empty());
}

#[test]
fn turn_start_error_tears_down_failed() {
    assert_eq!(
        teardown(&thread_ready().step(err(3, -1, "bad turn"))),
        Some((Outcome::Failed, "turn/start failed: bad turn".into()))
    );
}

#[test]
fn agent_message_for_other_turn_is_not_reported() {
    let mut machine = active();
    assert!(machine
        .step(agent_item("th-1", "other", "a", "ignored", None))
        .is_empty());
    let actions = machine.step(turn_completed("th-1", "t1", "completed"));
    assert!(!actions
        .iter()
        .any(|action| matches!(action, Action::StoreReport { .. })));
}

#[test]
fn completed_turn_stores_report_then_succeeds_in_that_order() {
    let mut machine = active();
    assert!(machine
        .step(agent_item("th-1", "t1", "a", "answer", None))
        .is_empty());
    assert_eq!(
        machine.step(turn_completed("th-1", "t1", "completed")),
        vec![
            Action::Persist(NativeEvent::TurnCompleted {
                turn_id: "t1".into(),
                status: "completed".into()
            }),
            Action::StoreReport {
                text: "answer".into(),
                complete: false
            },
            Action::Teardown {
                outcome: Outcome::Succeeded,
                reason: "turn completed".into()
            },
        ]
    );
    assert!(machine.is_finished());
}

#[test]
fn completed_turn_without_agent_message_stores_no_report() {
    let actions = active().step(turn_completed("th-1", "t1", "completed"));
    assert_eq!(actions.len(), 2);
    assert_eq!(
        teardown(&actions),
        Some((Outcome::Succeeded, "turn completed".into()))
    );
}

#[test]
fn report_keeps_only_final_answer() {
    let mut machine = active();
    machine.step(agent_item("th-1", "t1", "a", "A", Some("commentary")));
    machine.step(agent_item("th-1", "t1", "b", "B", Some("final_answer")));
    machine.step(agent_item("th-1", "t1", "b", "B2", Some("final_answer")));
    let actions = machine.step(turn_completed("th-1", "t1", "completed"));
    assert_eq!(
        actions[1],
        Action::StoreReport {
            text: "B2".into(),
            complete: true
        }
    );
}

#[test]
fn failed_turn_tears_down_failed_with_provider_message() {
    let actions = active().step(notify(
        "turn/completed",
        json!({"threadId": "th-1",
        "turn": {"id": "t1", "status": "failed", "error": {"message": "provider failed"}}}),
    ));
    assert_eq!(
        teardown(&actions),
        Some((Outcome::Failed, "provider failed".into()))
    );
}

#[test]
fn interrupted_turn_without_cancel_is_cancelled() {
    assert_eq!(
        teardown(&active().step(turn_completed("th-1", "t1", "interrupted"))),
        Some((Outcome::Cancelled, "turn interrupted".into()))
    );
}

#[test]
fn server_request_is_answered_echoing_the_id_and_persisted() {
    for id in [json!("provider-1"), json!(77)] {
        let actions = Machine::new(config()).step(Input::Frame(Incoming::ServerRequest {
            id: id.clone(),
            method: "item/commandExecution/requestApproval".into(),
            params: json!({}),
        }));
        assert_eq!(
            actions,
            vec![
                Action::Send(json!({"id": id, "result": {"decision": "decline"}})),
                Action::Persist(NativeEvent::ServerRequestRefused {
                    method: "item/commandExecution/requestApproval".into(),
                })
            ]
        );
    }
}

#[test]
fn unknown_response_id_is_ignored() {
    let mut machine = started();
    assert!(machine.step(ok(999, json!({}))).is_empty());
    machine.step(ok(1, json!({})));
    assert!(machine.step(err(1, -1, "duplicate")).is_empty());
}

#[test]
fn notification_for_another_thread_is_ignored() {
    let mut machine = thread_ready();
    assert!(machine.step(turn_started("other", "t1")).is_empty());
    machine.step(turn_started("th-1", "t1"));
    assert!(machine
        .step(agent_item("other", "t1", "a", "ignored", None))
        .is_empty());
    assert!(machine
        .step(turn_completed("other", "t1", "completed"))
        .is_empty());
    assert!(!machine.is_finished());
}

#[test]
fn unknown_notification_method_is_ignored() {
    assert!(active().step(notify("future/method", json!({}))).is_empty());
}

#[test]
fn stale_turn_started_for_finished_turn_is_ignored() {
    let mut machine = active();
    machine.step(next_turn("m1", "next"));
    machine.step(turn_completed("th-1", "t1", "completed"));
    assert!(machine.step(turn_started("th-1", "t1")).is_empty());
    assert!(machine
        .step(turn_completed("th-1", "t1", "completed"))
        .is_empty());
}

#[test]
fn any_input_after_teardown_yields_nothing() {
    let mut machine = active();
    machine.step(Input::TransportClosed);
    for input in [
        Input::Start,
        ok(3, json!({})),
        steer("s", "text"),
        next_turn("n", "text"),
        Input::Cancel {
            reason: "stop".into(),
        },
        Input::Deadline,
        Input::StartupDeadline,
        Input::GraceElapsed,
        Input::TransportClosed,
        turn_started("th-1", "t2"),
    ] {
        assert!(machine.step(input).is_empty());
    }
}

#[test]
fn teardown_is_emitted_exactly_once() {
    let mut machine = active();
    let actions = machine.step(turn_completed("th-1", "t1", "completed"));
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::Teardown { .. }))
            .count(),
        1
    );
    assert!(matches!(actions.last(), Some(Action::Teardown { .. })));
    assert!(machine.step(Input::TransportClosed).is_empty());
}
