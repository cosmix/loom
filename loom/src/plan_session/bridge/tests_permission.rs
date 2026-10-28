//! Bridge tests: permission cards, their answer files, expiry, the binding to the tool call a
//! request names, and the denials of draining and ending.

use std::fs;
use std::thread;
use std::time::Duration;

use serde_json::json;

use super::fake_host::*;
use super::{BridgeError, BridgeEvent};
use crate::plan_session::kind::SessionKind::Authoring;
use crate::plan_session::model::{DenyReason, Item, PermissionStatus, SessionState};
use crate::plan_session::permission::Behavior;

fn card_status(h: &Harness, request: &str) -> Option<PermissionStatus> {
    let turns = h.bridge.chat_snapshot(true).turns;
    let items = turns.into_iter().flat_map(|turn| turn.items);
    let mut cards = items.filter_map(|item| match item {
        Item::Permission {
            request_id, status, ..
        } if request_id == request => Some(status),
        _ => None,
    });
    cards.next()
}

#[test]
fn allow_deny_and_expiry_write_their_answer_files() {
    let h = Harness::ready(|c| c.permission_deny_after = Duration::from_millis(1500));
    let s = h.idle(Authoring);
    h.working(&s);
    let allowed = h.permission(&s, "Bash", json!({"command": "a"}));
    let denied = h.permission(&s, "Bash", json!({"command": "b"}));
    let expiring = h.permission(&s, "Bash", json!({"command": "c"}));
    h.wait_state(&s, SessionState::Permission);
    h.bridge
        .answer_permission(&s.id, &allowed, true, None)
        .unwrap();
    let note = Some("not that one".to_owned());
    h.bridge
        .answer_permission(&s.id, &denied, false, note)
        .unwrap();
    let allow = h.answer(&s, &allowed).unwrap();
    assert_eq!((allow.behavior, allow.reason), (Behavior::Allow, None));
    let deny = h.answer(&s, &denied).unwrap();
    assert_eq!(deny.reason, Some(DenyReason::Holder));
    assert_eq!(deny.message.as_deref(), Some("not that one"));
    assert_eq!(
        h.state(&s),
        Some(SessionState::Permission),
        "one request still waits"
    );
    h.wait_deny(&s, &expiring, DenyReason::Timeout);
    h.wait_state(&s, SessionState::Working);
    assert_eq!(card_status(&h, &allowed), Some(PermissionStatus::Allowed));
    assert_eq!(card_status(&h, &expiring), Some(PermissionStatus::Denied));
}

#[test]
fn two_requests_keep_permission_until_both_are_answered() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    let first = h.permission(&s, "Bash", json!({"command": "one"}));
    let second = h.permission(&s, "Bash", json!({"command": "two"}));
    h.wait_state(&s, SessionState::Permission);
    h.bridge
        .answer_permission(&s.id, &first, true, None)
        .unwrap();
    settle();
    assert_eq!(h.state(&s), Some(SessionState::Permission));
    h.bridge
        .answer_permission(&s.id, &second, true, None)
        .unwrap();
    h.wait_state(&s, SessionState::Working);
}

#[test]
fn an_answer_after_the_stop_is_refused_and_writes_nothing() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    let request = h.permission(&s, "Bash", json!({"command": "ls"}));
    h.wait_state(&s, SessionState::Permission);
    h.spool(&s, "Stop", json!({}));
    h.wait_deny(&s, &request, DenyReason::SessionEnded);
    let late = h.bridge.answer_permission(&s.id, &request, true, None);
    assert!(matches!(late, Err(BridgeError::Refused(_))), "{late:?}");
    let answer = h.answer(&s, &request).unwrap();
    assert_eq!(
        answer.behavior,
        Behavior::Deny,
        "the late answer replaced the deny"
    );
}

#[test]
fn the_named_tools_result_closes_its_request() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    let input = json!({"command": "make"});
    h.record(
        &s,
        assistant_tools("msg_make", &[("toolu_m", "Bash", input.clone())]),
    );
    settle();
    let request = h.permission(&s, "Bash", input);
    h.wait_state(&s, SessionState::Permission);
    h.record(&s, tool_result("toolu_m", json!({})));
    h.wait_deny(&s, &request, DenyReason::SessionEnded);
    h.wait_state(&s, SessionState::Working);
    assert_eq!(card_status(&h, &request), Some(PermissionStatus::Denied));
}

#[test]
fn an_auto_denied_request_shows_no_card_and_keeps_the_state() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    let request = uuid::Uuid::new_v4().to_string();
    let extra = json!({"request_id": request, "tool_name": "Bash", "tool_input": {},
        "auto_denied": "summary_session"});
    h.spool(&s, "PermissionRequest", extra);
    settle();
    assert_eq!(h.state(&s), Some(SessionState::Working));
    assert_eq!(card_status(&h, &request), None);
}

#[test]
fn draining_denies_pending_and_arriving_requests_and_still_closes() {
    let h = Harness::ready(|c| c.final_write_bound = Duration::from_secs(3));
    let s = h.idle(Authoring);
    h.working(&s);
    let pending = h.permission(&s, "Bash", json!({"command": "one"}));
    h.wait_state(&s, SessionState::Permission);
    let bridge = h.bridge.clone();
    let drained = thread::spawn(move || bridge.drain());
    h.wait_deny(&s, &pending, DenyReason::ServerStopping);
    h.spool(&s, "Stop", json!({}));
    wait_until("the closing message", || h.sent(&s) == [CLOSING]);
    h.spool(&s, "UserPromptSubmit", json!({"prompt": CLOSING}));
    let arriving = h.permission(&s, "Bash", json!({"command": "two"}));
    h.wait_deny(&s, &arriving, DenyReason::ServerStopping);
    h.spool(&s, "Stop", json!({}));
    drained.join().unwrap();
    assert!(h.ended(&s.id).is_some(), "drain returned before the end");
    assert!(!s.dir.path().exists());
}

#[test]
fn a_replaced_spool_applies_only_its_new_event() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    let request = h.permission(&s, "Bash", json!({"command": "ls"}));
    h.wait_state(&s, SessionState::Permission);
    h.bridge
        .answer_permission(&s.id, &request, true, None)
        .unwrap();
    h.wait_state(&s, SessionState::Working);
    let started = |h: &Harness| h.count_events(|e| matches!(e, BridgeEvent::TurnStarted { .. }));
    let ended = |h: &Harness| h.count_events(|e| matches!(e, BridgeEvent::TurnEnded { .. }));
    let cards = |h: &Harness| {
        let turns = h.bridge.chat_snapshot(true).turns;
        let items = turns.into_iter().flat_map(|turn| turn.items);
        items
            .filter(|item| matches!(item, Item::Permission { .. }))
            .count()
    };
    let before = (started(&h), cards(&h), ended(&h));
    let copy = s.dir.path().join("events.copy");
    fs::copy(s.dir.events(), &copy).unwrap();
    fs::rename(&copy, s.dir.events()).unwrap();
    h.spool(&s, "Stop", json!({}));
    h.wait_state(&s, SessionState::Idle);
    settle();
    assert_eq!(started(&h), before.0, "a turn replayed");
    assert_eq!(cards(&h), before.1, "a permission card replayed");
    assert_eq!(ended(&h), before.2 + 1);
}
