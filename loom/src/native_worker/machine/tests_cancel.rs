use super::test_support::*;
use super::*;
use crate::native_worker::model::MessageState;
use serde_json::json;

#[test]
fn cancel_before_start_input_tears_down_cancelled() {
    let mut machine = Machine::new(config());
    assert_eq!(
        machine.step(Input::Cancel {
            reason: "stop".into()
        }),
        vec![
            Action::Persist(NativeEvent::CancelRequested {
                reason: "stop".into()
            }),
            Action::Teardown {
                outcome: Outcome::Cancelled,
                reason: "stop".into()
            },
        ]
    );
    assert!(machine.step(Input::Start).is_empty());
}

#[test]
fn cancel_awaiting_initialize_waits_for_the_response_and_sends_no_thread_start() {
    let mut machine = started();
    assert_eq!(
        machine.step(Input::Cancel {
            reason: "stop".into()
        }),
        vec![Action::Persist(NativeEvent::CancelRequested {
            reason: "stop".into()
        })]
    );
    assert!(!machine.is_finished());
    assert_eq!(
        machine.step(ok(1, json!({}))),
        vec![Action::Teardown {
            outcome: Outcome::Cancelled,
            reason: "stop".into()
        }]
    );
}

#[test]
fn cancel_before_turn_never_starts_a_turn() {
    let mut machine = started();
    machine.step(ok(1, json!({})));
    assert_eq!(
        machine
            .step(Input::Cancel {
                reason: "stop".into()
            })
            .len(),
        1
    );
    assert!(!machine.is_finished());
    assert_eq!(
        machine.step(ok(2, json!({"thread": {"id": "th-1"}}))),
        vec![
            Action::Persist(NativeEvent::ThreadStarted {
                thread_id: "th-1".into()
            }),
            Action::Teardown {
                outcome: Outcome::Cancelled,
                reason: "stop".into()
            },
        ]
    );
}

#[test]
fn cancel_with_requested_turn_defers_interrupt_until_turn_id_is_known() {
    let mut machine = thread_ready();
    assert_eq!(
        machine.step(Input::Cancel {
            reason: "stop".into()
        }),
        vec![
            Action::Persist(NativeEvent::CancelRequested {
                reason: "stop".into()
            }),
            Action::ArmGrace,
        ]
    );
    let actions = machine.step(ok(3, json!({"turn": {"id": "t1"}})));
    assert_eq!(
        persisted(&actions),
        [NativeEvent::TurnStarted {
            turn_id: "t1".into()
        }]
    );
    assert_eq!(methods(&actions), ["turn/interrupt"]);
    assert_eq!(
        sent(&actions, "turn/interrupt")["params"],
        json!({"threadId": "th-1", "turnId": "t1"})
    );
    assert!(!actions.contains(&Action::ArmGrace));
    assert!(machine.step(turn_started("th-1", "t1")).is_empty());
}

#[test]
fn cancel_with_active_turn_persists_interrupts_and_arms_grace() {
    let mut machine = active();
    let actions = machine.step(Input::Cancel {
        reason: "stop".into(),
    });
    assert_eq!(
        actions[0],
        Action::Persist(NativeEvent::CancelRequested {
            reason: "stop".into()
        })
    );
    assert_eq!(actions.len(), 3);
    let frame = sent(&actions, "turn/interrupt");
    assert_eq!(frame["params"], json!({"threadId": "th-1", "turnId": "t1"}));
    assert_eq!(actions[2], Action::ArmGrace);
    assert!(machine
        .step(err(frame["id"].as_u64().unwrap(), -1, "gone"))
        .is_empty());
}

#[test]
fn accepted_cancel_beats_late_success() {
    let mut machine = active();
    machine.step(agent_item(
        "th-1",
        "t1",
        "a",
        "answer",
        Some("final_answer"),
    ));
    machine.step(Input::Cancel {
        reason: "stop".into(),
    });
    let actions = machine.step(turn_completed("th-1", "t1", "completed"));
    assert_eq!(
        actions,
        vec![
            Action::Persist(NativeEvent::TurnCompleted {
                turn_id: "t1".into(),
                status: "completed".into()
            }),
            Action::Teardown {
                outcome: Outcome::Cancelled,
                reason: "stop".into()
            },
        ]
    );
}

#[test]
fn second_cancel_is_ignored() {
    let mut machine = active();
    machine.step(Input::Cancel {
        reason: "first".into(),
    });
    assert!(machine
        .step(Input::Cancel {
            reason: "second".into()
        })
        .is_empty());
    assert_eq!(
        teardown(&machine.step(Input::GraceElapsed)),
        Some((Outcome::Cancelled, "first".into()))
    );
}

#[test]
fn deadline_with_active_turn_interrupts_arms_grace_and_times_out() {
    let mut machine = active();
    let actions = machine.step(Input::Deadline);
    assert_eq!(actions[0], Action::Persist(NativeEvent::DeadlineExpired));
    assert_eq!(methods(&actions), ["turn/interrupt"]);
    assert_eq!(actions[2], Action::ArmGrace);
    assert_eq!(
        teardown(&machine.step(turn_completed("th-1", "t1", "completed"))),
        Some((Outcome::TimedOut, "deadline expired".into()))
    );
}

#[test]
fn deadline_after_cancel_is_ignored() {
    let mut machine = active();
    machine.step(Input::Cancel {
        reason: "stop".into(),
    });
    assert!(machine.step(Input::Deadline).is_empty());
    assert_eq!(
        teardown(&machine.step(Input::GraceElapsed)),
        Some((Outcome::Cancelled, "stop".into()))
    );
}

#[test]
fn cancel_after_deadline_stays_timed_out() {
    let mut machine = active();
    machine.step(Input::Deadline);
    assert!(machine
        .step(Input::Cancel {
            reason: "stop".into()
        })
        .is_empty());
    assert_eq!(
        teardown(&machine.step(Input::TransportClosed)),
        Some((Outcome::TimedOut, "deadline expired".into()))
    );
}

#[test]
fn deadline_before_first_turn_is_ignored() {
    let mut awaiting_thread = started();
    awaiting_thread.step(ok(1, json!({})));
    for mut machine in [Machine::new(config()), started(), awaiting_thread] {
        assert!(machine.step(Input::Deadline).is_empty());
        assert!(!machine.is_finished());
    }
}

#[test]
fn startup_deadline_before_thread_start_times_out_with_the_reason() {
    let mut awaiting_thread = started();
    awaiting_thread.step(ok(1, json!({})));
    for mut machine in [started(), awaiting_thread] {
        assert_eq!(
            teardown(&machine.step(Input::StartupDeadline)),
            Some((
                Outcome::TimedOut,
                "startup deadline (120 s) passed before the Codex thread started".into()
            ))
        );
        assert!(machine.is_finished());
    }
}

#[test]
fn startup_deadline_after_thread_start_or_before_start_is_ignored() {
    for mut machine in [Machine::new(config()), thread_ready(), active()] {
        assert!(machine.step(Input::StartupDeadline).is_empty());
        assert!(!machine.is_finished());
    }
}

#[test]
fn startup_deadline_after_cancel_keeps_cancelled() {
    let mut machine = started();
    machine.step(Input::Cancel {
        reason: "stop".into(),
    });
    assert_eq!(
        teardown(&machine.step(Input::StartupDeadline)),
        Some((Outcome::Cancelled, "stop".into()))
    );
}

#[test]
fn grace_elapsed_after_cancel_tears_down_cancelled_and_settles_messages() {
    let mut machine = active();
    machine.step(steer("s1", "adjust"));
    machine.step(next_turn("n1", "next"));
    machine.step(Input::Cancel {
        reason: "stop".into(),
    });
    assert_eq!(
        machine.step(Input::GraceElapsed),
        vec![
            Action::Persist(message_event(
                "s1",
                MessageState::DeliveryUnknown,
                None,
                None
            )),
            Action::Persist(message_event(
                "n1",
                MessageState::CancelledBeforeDelivery,
                None,
                None
            )),
            Action::Teardown {
                outcome: Outcome::Cancelled,
                reason: "stop".into()
            },
        ]
    );
}

#[test]
fn grace_elapsed_after_deadline_tears_down_timed_out() {
    let mut machine = active();
    machine.step(Input::Deadline);
    assert_eq!(
        teardown(&machine.step(Input::GraceElapsed)),
        Some((Outcome::TimedOut, "deadline expired".into()))
    );
}

#[test]
fn grace_elapsed_without_close_is_ignored() {
    assert!(active().step(Input::GraceElapsed).is_empty());
    assert!(started().step(Input::GraceElapsed).is_empty());
}

#[test]
fn transport_closed_after_cancel_is_cancelled() {
    let mut machine = active();
    machine.step(Input::Cancel {
        reason: "stop".into(),
    });
    assert_eq!(
        teardown(&machine.step(Input::TransportClosed)),
        Some((Outcome::Cancelled, "stop".into()))
    );
}

#[test]
fn transport_closed_when_open_is_failed_with_exact_reason() {
    assert_eq!(
        teardown(&active().step(Input::TransportClosed)),
        Some((Outcome::Failed, "codex app-server closed its output".into()))
    );
}

#[test]
fn cancel_wins_when_pending_handshake_response_fails() {
    for (mut machine, id) in [
        (started(), 1),
        (
            {
                let mut machine = started();
                machine.step(ok(1, json!({})));
                machine
            },
            2,
        ),
        (thread_ready(), 3),
    ] {
        machine.step(Input::Cancel {
            reason: "stop".into(),
        });
        assert_eq!(
            teardown(&machine.step(err(id, -1, "failed"))),
            Some((Outcome::Cancelled, "stop".into()))
        );
    }
}

#[test]
fn deadline_with_requested_turn_interrupts_on_notification_only_once() {
    let mut machine = thread_ready();
    assert_eq!(
        machine.step(Input::Deadline),
        vec![
            Action::Persist(NativeEvent::DeadlineExpired),
            Action::ArmGrace
        ]
    );
    assert_eq!(
        methods(&machine.step(turn_started("th-1", "t1"))),
        ["turn/interrupt"]
    );
    assert!(machine
        .step(ok(3, json!({"turn": {"id": "t1"}})))
        .is_empty());
    assert!(machine.step(Input::Deadline).is_empty());
}

#[test]
fn cancel_wins_for_every_terminal_turn_status() {
    for status in ["completed", "failed", "interrupted"] {
        let mut machine = active();
        machine.step(Input::Cancel {
            reason: "stop".into(),
        });
        assert_eq!(
            teardown(&machine.step(turn_completed("th-1", "t1", status))),
            Some((Outcome::Cancelled, "stop".into()))
        );
    }
}

#[test]
fn unknown_turn_status_and_error_notifications_are_ignored() {
    let mut machine = active();
    assert!(machine
        .step(turn_completed("th-1", "t1", "inProgress"))
        .is_empty());
    assert!(machine
        .step(notify(
            "error",
            json!({"threadId": "th-1", "turnId": "t1",
        "error": {"message": "retrying"}, "willRetry": true})
        ))
        .is_empty());
    assert!(!machine.is_finished());
}
