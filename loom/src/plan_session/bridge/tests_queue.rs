//! Bridge tests: sending, the queue, delivery timeouts, withdrawal, interrupts and `/compact`.

use std::time::Duration;

use serde_json::json;

use super::fake_host::*;
use super::{BridgeError, EndCause, EndMode, EndOutcome, SendOutcome};
use crate::plan_session::kind::SessionKind::Authoring;
use crate::plan_session::model::{Delivery, DenyReason, Item, SessionState};

fn send(h: &Harness, s: &Sess, text: &str) -> SendOutcome {
    h.bridge.send(&s.id, text.to_owned(), Vec::new()).unwrap()
}

/// The delivery state of the latest user item with this text.
fn delivery(h: &Harness, text: &str) -> Option<Delivery> {
    let turns = h.bridge.chat_snapshot(true).turns;
    let items = turns.into_iter().flat_map(|turn| turn.items);
    let mut found = items.filter_map(|item| match item {
        Item::User {
            text: shown,
            delivery,
            ..
        } if shown == text => Some(delivery),
        _ => None,
    });
    found.next_back()
}

#[test]
fn a_send_while_idle_is_pasted_once_and_confirmed() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    assert!(matches!(send(&h, &s, "A"), SendOutcome::Delivering { .. }));
    wait_until("A pasted", || h.sent(&s) == ["A"]);
    assert_eq!(delivery(&h, "A"), Some(Delivery::Sent));
    h.working(&s);
    wait_until("A delivered", || {
        delivery(&h, "A") == Some(Delivery::Delivered)
    });
    h.spool(&s, "Stop", json!({}));
    settle();
    assert_eq!(h.sent(&s), ["A"]);
}

#[test]
fn queued_messages_go_out_one_per_idle() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    for text in ["B", "C"] {
        assert!(matches!(send(&h, &s, text), SendOutcome::Queued { .. }));
    }
    assert_eq!(h.bridge.chat_snapshot(true).queue.len(), 2);
    h.spool(&s, "Stop", json!({}));
    wait_until("B", || h.sent(&s) == ["B"]);
    settle();
    assert_eq!(h.sent(&s), ["B"], "C went out before B's turn");
    h.working(&s);
    h.spool(&s, "Stop", json!({}));
    wait_until("C", || h.sent(&s) == ["B", "C"]);
    assert!(h.bridge.chat_snapshot(true).queue.is_empty());
}

#[test]
fn an_unconfirmed_paste_turns_undelivered_and_a_late_prompt_delivers_it() {
    let h = Harness::ready(|c| c.delivery_timeout = Duration::from_millis(400));
    let s = h.idle(Authoring);
    assert!(matches!(send(&h, &s, "A"), SendOutcome::Delivering { .. }));
    assert!(matches!(send(&h, &s, "B"), SendOutcome::Queued { .. }));
    wait_until("A undelivered", || {
        delivery(&h, "A") == Some(Delivery::Undelivered)
    });
    settle();
    assert_eq!(h.sent(&s), ["A"], "the queue did not pause");
    h.spool(&s, "UserPromptSubmit", json!({"prompt": "A"}));
    wait_until("A delivered", || {
        delivery(&h, "A") == Some(Delivery::Delivered)
    });
    settle();
    assert_eq!(h.sent(&s), ["A"], "the queue resumed before a Stop");
    h.spool(&s, "Stop", json!({}));
    wait_until("B", || h.sent(&s) == ["A", "B"]);
}

#[test]
fn a_closing_message_confirmed_after_its_timeout_still_closes_at_the_stop() {
    let h = Harness::ready(|c| c.delivery_timeout = Duration::from_millis(400));
    let s = h.idle(Authoring);
    h.bridge
        .end(&s.id, EndMode::FinalWrite, EndCause::Holder)
        .unwrap();
    wait_until("the closing message", || h.sent(&s) == [CLOSING]);
    wait_until("the closing undelivered", || {
        delivery(&h, CLOSING) == Some(Delivery::Undelivered)
    });
    h.spool(&s, "UserPromptSubmit", json!({"prompt": CLOSING}));
    wait_until("the closing delivered", || {
        delivery(&h, CLOSING) == Some(Delivery::Delivered)
    });
    h.spool(&s, "Stop", json!({}));
    wait_until("the end", || h.ended(&s.id) == Some(EndOutcome::Closed));
}

#[test]
fn withdraw_takes_back_a_queued_message_but_not_one_being_delivered() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    let SendOutcome::Queued { queue_id: first } = send(&h, &s, "A") else {
        panic!("A was not queued");
    };
    let SendOutcome::Queued { queue_id: second } = send(&h, &s, "B") else {
        panic!("B was not queued");
    };
    h.spool(&s, "Stop", json!({}));
    wait_until("A", || h.sent(&s) == ["A"]);
    let refused = h.bridge.withdraw(&s.id, &first);
    assert!(
        matches!(refused, Err(BridgeError::Refused(_))),
        "{refused:?}"
    );
    h.bridge.withdraw(&s.id, &second).unwrap();
    assert!(h.bridge.chat_snapshot(true).queue.is_empty());
    h.working(&s);
    h.spool(&s, "Stop", json!({}));
    settle();
    assert_eq!(h.sent(&s), ["A"], "the withdrawn message went out");
}

#[test]
fn interrupt_needs_a_running_turn_and_presses_once() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    let press = Call::Interrupt(s.socket.clone());
    assert!(matches!(
        h.bridge.interrupt(&s.id),
        Err(BridgeError::NotRunning)
    ));
    settle();
    assert_eq!(
        h.host.count(&press),
        0,
        "an idle interrupt reached the host"
    );
    h.working(&s);
    let request = h.permission(&s, "Bash", json!({"command": "ls"}));
    h.wait_state(&s, SessionState::Permission);
    assert!(matches!(send(&h, &s, "Q"), SendOutcome::Queued { .. }));
    assert_eq!(h.bridge.interrupt(&s.id).unwrap(), ["Q"]);
    h.wait_deny(&s, &request, DenyReason::Interrupt);
    assert_eq!(h.bridge.interrupt(&s.id).unwrap(), Vec::<String>::new());
    wait_until("the press", || h.host.count(&press) == 1);
    settle();
    assert_eq!(h.host.count(&press), 1, "two Escapes in one turn");
    h.record(&s, interrupted());
    h.wait_state(&s, SessionState::Idle);
    h.working(&s);
    h.bridge
        .end(&s.id, EndMode::FinalWrite, EndCause::Holder)
        .unwrap();
    let ending = h.bridge.interrupt(&s.id);
    assert!(matches!(ending, Err(BridgeError::NotRunning)), "{ending:?}");
}

#[test]
fn sends_and_withdrawals_are_refused_while_ending() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    let SendOutcome::Queued { queue_id } = send(&h, &s, "A") else {
        panic!("A was not queued");
    };
    assert_eq!(
        h.bridge
            .end(&s.id, EndMode::FinalWrite, EndCause::Holder)
            .unwrap(),
        ["A"]
    );
    let late = h.bridge.send(&s.id, "B".to_owned(), Vec::new());
    assert!(matches!(late, Err(BridgeError::NotRunning)), "{late:?}");
    let withdrawn = h.bridge.withdraw(&s.id, &queue_id);
    assert!(
        matches!(withdrawn, Err(BridgeError::NotRunning)),
        "{withdrawn:?}"
    );
}

#[test]
fn a_compact_confirmed_by_its_session_start_lets_the_queue_go_on() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    assert!(matches!(
        send(&h, &s, "/compact keep it"),
        SendOutcome::Delivering { .. }
    ));
    wait_until("the compact", || h.sent(&s) == ["/compact keep it"]);
    assert!(matches!(send(&h, &s, "next"), SendOutcome::Queued { .. }));
    h.spool(&s, "SessionStart", json!({"source": "compact"}));
    wait_until("compact delivered", || {
        delivery(&h, "/compact keep it") == Some(Delivery::Delivered)
    });
    wait_until("next", || h.sent(&s) == ["/compact keep it", "next"]);
}

#[test]
fn a_compact_boundary_with_no_stop_after_it_goes_idle() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    send(&h, &s, "/compact");
    wait_until("the compact", || h.sent(&s) == ["/compact"]);
    h.spool(&s, "UserPromptSubmit", json!({"prompt": "/compact"}));
    h.wait_state(&s, SessionState::Working);
    h.record(
        &s,
        json!({"type": "system", "subtype": "compact_boundary",
        "compactMetadata": {"trigger": "manual", "preTokens": 9, "postTokens": 3}}),
    );
    h.wait_state(&s, SessionState::Idle);
}
