//! Bridge tests: sending, the queue, delivery timeouts, withdrawal, interrupts and `/compact`.

use std::thread;
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
fn a_turn_ended_by_stop_failure_delivers_the_next_queued_message() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    assert!(matches!(send(&h, &s, "B"), SendOutcome::Queued { .. }));
    h.spool(&s, "StopFailure", json!({"error": "server_error"}));
    wait_until("B", || h.sent(&s) == ["B"]);
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
fn a_late_prompt_in_a_pasted_content_wrapper_delivers_its_message() {
    let h = Harness::ready(|c| c.delivery_timeout = Duration::from_millis(400));
    let s = h.idle(Authoring);
    assert!(matches!(
        send(&h, &s, "A b"),
        SendOutcome::Delivering { .. }
    ));
    wait_until("A b undelivered", || {
        delivery(&h, "A b") == Some(Delivery::Undelivered)
    });
    let wrapped = "\n\n<pasted_content id=\"f3a0\">\nA b\n</pasted_content id=\"f3a0\">\n";
    h.spool(&s, "UserPromptSubmit", json!({"prompt": wrapped}));
    wait_until("A b delivered", || {
        delivery(&h, "A b") == Some(Delivery::Delivered)
    });
}

#[test]
fn a_typed_pasted_content_wrapper_is_queued_and_shown_as_typed() {
    let typed = "<pasted_content id=\"f3a0\">\nA b\n</pasted_content id=\"f3a0\">";
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    assert!(matches!(send(&h, &s, typed), SendOutcome::Queued { .. }));
    let queued = h.bridge.chat_snapshot(true).queue;
    assert_eq!(queued[0].text.as_deref(), Some(typed), "the queue entry");
    h.spool(&s, "Stop", json!({}));
    wait_until("pasted", || h.sent(&s) == [typed]);
    assert_eq!(delivery(&h, typed), Some(Delivery::Sent), "the sent item");
}

#[test]
fn an_unsubmitted_authoring_opening_waits_undelivered_and_a_late_prompt_delivers_it() {
    let h = Harness::ready(|c| c.delivery_timeout = Duration::from_millis(2500));
    let s = h.start(Authoring);
    h.spool(&s, "SessionStart", json!({"source": "startup"}));
    wait_until("the opening", || h.host.delivered(&s.socket) == [OPENING]);
    wait_until("the opening undelivered", || {
        delivery(&h, OPENING) == Some(Delivery::Undelivered)
    });
    let enter = Call::PressEnter(s.socket.clone());
    assert_eq!(
        h.host.count(&enter),
        1,
        "Enter again at 2 s, none past the timeout"
    );
    assert_eq!(
        h.ended(&s.id),
        None,
        "an authoring session failed on its opening"
    );
    assert_eq!(h.host.delivered(&s.socket), [OPENING]);
    h.spool(&s, "UserPromptSubmit", json!({"prompt": OPENING}));
    wait_until("the opening delivered", || {
        delivery(&h, OPENING) == Some(Delivery::Delivered)
    });
}

#[test]
fn a_chat_message_never_gets_enter_again() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    send(&h, &s, "A");
    wait_until("A pasted", || h.sent(&s) == ["A"]);
    thread::sleep(Duration::from_millis(2300));
    assert_eq!(h.host.count(&Call::PressEnter(s.socket.clone())), 0);
    assert_eq!(h.sent(&s), ["A"]);
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
fn a_second_interrupt_after_the_press_presses_nothing() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    let press = Call::Interrupt(s.socket.clone());
    h.working(&s);
    assert_eq!(h.bridge.interrupt(&s.id).unwrap(), Vec::<String>::new());
    wait_until("the press", || h.host.count(&press) == 1);
    assert_eq!(h.bridge.interrupt(&s.id).unwrap(), Vec::<String>::new());
    settle();
    assert_eq!(
        h.host.count(&press),
        1,
        "a second Escape opens the rewind picker"
    );
}

#[test]
fn an_interrupt_returns_a_message_accepted_at_idle_and_not_yet_pasted() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    let content = "<task-notification>\n<task-id>b1</task-id>";
    let notification = json!({"type": "user", "message": {"role": "user", "content": content},
        "origin": {"kind": "task-notification"}});
    let line = record_line(notification, &s.id);
    // One locked step, so the driver cannot paste in between: A is accepted at Idle, then the
    // notification's turn starts and holds it.
    let sent = h.bridge.with_session(&s.id, |session, step| {
        let sent = session.send("A".to_owned(), Vec::new(), step);
        session.ingest_main(&line, step);
        sent
    });
    h.bridge.flush();
    assert!(
        matches!(sent, Some(Ok(SendOutcome::Delivering { .. }))),
        "{sent:?}"
    );
    h.wait_state(&s, SessionState::Working);
    assert_eq!(h.bridge.interrupt(&s.id).unwrap(), ["A"]);
    h.record(&s, interrupted());
    h.wait_state(&s, SessionState::Idle);
    settle();
    assert_eq!(
        h.sent(&s),
        Vec::<String>::new(),
        "A was pasted at the interrupt's idle"
    );
    assert_eq!(delivery(&h, "A"), Some(Delivery::Undelivered));
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

#[test]
fn a_sent_message_is_one_item_after_delivery() {
    let text = "Make this a feature instead.";
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    assert!(matches!(send(&h, &s, text), SendOutcome::Delivering { .. }));
    wait_until("pasted", || h.sent(&s) == [text]);
    // The hook confirms the paste before Claude Code writes the user record.
    h.spool(&s, "UserPromptSubmit", json!({"prompt": text}));
    wait_until("delivered", || {
        delivery(&h, text) == Some(Delivery::Delivered)
    });
    let user = json!({"type": "user", "origin": {"kind": "human"},
        "message": {"role": "user", "content": text}});
    let reply = json!({"type": "assistant", "message": {"id": "m-1", "role": "assistant",
        "content": [{"type": "text", "text": "Done."}]}});
    // Records apply in order: the reply showing means the user record was mapped.
    h.record(&s, user);
    h.record(&s, reply);
    wait_until("the reply", || {
        let turns = h.bridge.chat_snapshot(true).turns;
        let mut items = turns.iter().flat_map(|turn| &turn.items);
        items.any(|item| matches!(item, Item::Text { markdown, .. } if markdown == "Done."))
    });
    let turns = h.bridge.chat_snapshot(true).turns;
    let shown = turns.iter().flat_map(|turn| &turn.items);
    let users = shown.filter(|item| matches!(item, Item::User { text: t, .. } if t == text));
    assert_eq!(users.count(), 1, "the message shows once");
}
