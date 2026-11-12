//! Agent-end eligibility and the close gate after background work drains.

use serde_json::{json, Value};

use super::fake_host::{settle, wait_until, Harness, Sess, CLOSING, OPENING};
use super::{BridgeError, EndCause, EndMode, EndOutcome, SendOutcome};
use crate::plan_session::kind::SessionKind;
use crate::plan_session::model::{Item, SessionState};

const DONE: &str =
    "Written.\n\n```loom-done\n{\"decisions\":[\"Use a cache\"],\"questions\":[]}\n```";

fn reply(h: &Harness, s: &Sess, text: &str) {
    h.record(
        s,
        json!({"type": "assistant", "message": {
            "id": uuid::Uuid::new_v4().to_string(), "role": "assistant",
            "content": [{"type": "text", "text": text}], "stop_reason": "end_turn"}}),
    );
    wait_until("the assistant text", || {
        h.bridge.lock().sessions.get(&s.id).is_some_and(|session| {
            session
                .mapper
                .turns()
                .iter()
                .flat_map(|turn| &turn.items)
                .any(|item| matches!(item, Item::Text { markdown, .. } if markdown == text))
        })
    });
}

fn running() -> Value {
    json!({"background_tasks": [{"id": "a1", "type": "subagent", "status": "running",
        "agent_type": "general-purpose", "description": "Review the plan"}]})
}

fn start_agent(h: &Harness, s: &Sess) {
    h.spool(
        s,
        "SubagentStart",
        json!({"agent_id": "a1", "agent_type": "general-purpose"}),
    );
    wait_until("the running background agent", || {
        h.bridge
            .lock()
            .sessions
            .get(&s.id)
            .is_some_and(|session| session.background.blocking() == 1)
    });
}

fn finish_agent(h: &Harness, s: &Sess) {
    let body = "[Subagent hand-back] The report follows:\n  DONE";
    let content = format!(
        "Another Claude session sent a message:\n<agent-message from=\"a1\">\n{body}\n</agent-message>"
    );
    h.record(
        s,
        json!({"type": "user", "isMeta": true,
            "message": {"role": "user", "content": content},
            "origin": {"kind": "peer", "from": "a1", "senderTaskId": "a1",
                "name": "general-purpose", "body": body, "handback": true,
                "handbackReportText": "DONE"},
            "promptSource": "system", "turnOrigin": "peer"}),
    );
    // A SubagentStop can still list itself as running; the explicit stop wins.
    let mut stopped = running();
    stopped["agent_id"] = json!("a1");
    stopped["agent_type"] = json!("general-purpose");
    h.spool(s, "SubagentStop", stopped);
}

#[test]
fn a_user_end_closes_when_background_work_finishes_without_another_main_stop() {
    let h = Harness::ready(|_| {});
    let s = h.idle(SessionKind::Authoring);
    h.working(&s);
    start_agent(&h, &s);
    h.bridge
        .end(&s.id, EndMode::FinalWrite, EndCause::Holder)
        .unwrap();
    h.spool(&s, "Stop", running());
    wait_until("the closing paste", || h.sent(&s) == [CLOSING]);
    h.spool(&s, "UserPromptSubmit", json!({"prompt": CLOSING}));
    reply(&h, &s, "Closing write done.");
    h.spool(&s, "Stop", running());
    settle();
    assert_eq!(h.ended(&s.id), None);
    assert_eq!(h.state(&s), Some(SessionState::Ending));

    finish_agent(&h, &s);
    wait_until("the end after the hand-back", || {
        h.ended(&s.id) == Some(EndOutcome::Closed)
    });
    assert_eq!(
        h.sent(&s),
        [CLOSING],
        "the closing message was pasted again"
    );
}

#[test]
fn an_agent_end_refuses_a_new_user_message_while_waiting_for_teardown() {
    let h = Harness::ready(|_| {});
    let s = h.idle(SessionKind::Authoring);
    h.working(&s);
    start_agent(&h, &s);
    reply(&h, &s, DONE);
    h.spool(&s, "Stop", running());
    h.wait_state(&s, SessionState::Ending);
    assert_eq!(h.ended(&s.id), None);
    let sent = h.bridge.send(&s.id, "One more change".into(), Vec::new());
    assert!(matches!(sent, Err(BridgeError::NotRunning)), "{sent:?}");

    finish_agent(&h, &s);
    wait_until("the agent end", || {
        h.ended(&s.id) == Some(EndOutcome::Closed)
    });
    assert!(h.sent(&s).is_empty(), "pasted after the agent end");
}

#[test]
fn a_summary_done_fence_uses_the_normal_summary_end() {
    let h = Harness::ready(|_| {});
    let s = h.start(SessionKind::Summary);
    h.spool(&s, "SessionStart", json!({"source": "startup"}));
    wait_until("the opening", || h.host.delivered(&s.socket) == [OPENING]);
    h.spool(&s, "UserPromptSubmit", json!({"prompt": OPENING}));
    h.wait_state(&s, SessionState::Working);
    start_agent(&h, &s);
    reply(&h, &s, DONE);
    assert_eq!(h.ended(&s.id), None, "ended before the main Stop");
    h.spool(&s, "Stop", running());
    h.wait_state(&s, SessionState::Idle);
    assert!(h.bridge.lock().sessions[&s.id].flow.end.is_none());

    finish_agent(&h, &s);
    settle();
    assert_eq!(h.ended(&s.id), None, "closed before the hand-back turn");
    let prompt = "<agent-message from=\"a1\">\n[Subagent hand-back] DONE\n</agent-message>";
    h.spool(&s, "UserPromptSubmit", json!({ "prompt": prompt }));
    h.spool(&s, "Stop", json!({"background_tasks": []}));
    wait_until("the summary end", || {
        h.ended(&s.id) == Some(EndOutcome::Closed)
    });
    assert_eq!(h.host.delivered(&s.socket), [OPENING]);
    assert!(h.bridge.chat_snapshot(false).previous.is_none());
}

#[test]
fn a_delivering_user_message_cancels_the_agent_end() {
    let h = Harness::ready(|_| {});
    let s = h.idle(SessionKind::Authoring);
    let sent = h
        .bridge
        .send(&s.id, "Keep working".into(), Vec::new())
        .unwrap();
    assert!(matches!(sent, SendOutcome::Delivering { .. }));
    wait_until("the pending paste", || h.sent(&s) == ["Keep working"]);
    // No prompt or user record confirms the delivery before this unprompted turn's Stop.
    reply(&h, &s, DONE);
    h.spool(&s, "Stop", json!({}));
    h.wait_state(&s, SessionState::Idle);
    settle();
    assert_eq!(h.ended(&s.id), None);
    let inner = h.bridge.lock();
    let session = inner.sessions.get(&s.id).unwrap();
    assert!(session.flow.end.is_none());
    assert!(session.flow.delivering.is_some());
    assert_eq!(h.sent(&s), ["Keep working"]);
}

#[test]
fn a_stop_failure_does_not_plan_an_agent_end() {
    let h = Harness::ready(|_| {});
    let s = h.idle(SessionKind::Authoring);
    h.working(&s);
    reply(&h, &s, DONE);
    h.spool(&s, "StopFailure", json!({}));
    h.wait_state(&s, SessionState::Idle);
    settle();
    assert_eq!(h.ended(&s.id), None);
    assert!(h.bridge.lock().sessions[&s.id].flow.end.is_none());
    assert!(h.sent(&s).is_empty());
}

#[test]
fn a_real_main_turn_after_the_final_stop_still_holds_the_close_gate() {
    let h = Harness::ready(|_| {});
    let s = h.idle(SessionKind::Authoring);
    h.working(&s);
    start_agent(&h, &s);
    reply(&h, &s, DONE);
    h.spool(&s, "Stop", running());
    h.wait_state(&s, SessionState::Ending);
    h.spool(
        &s,
        "UserPromptSubmit",
        json!({"prompt": "<task-notification>update</task-notification>"}),
    );
    reply(&h, &s, "Checking the last report.");
    finish_agent(&h, &s);
    settle();
    assert_eq!(h.ended(&s.id), None, "closed during main work");

    h.spool(&s, "Stop", json!({"background_tasks": []}));
    wait_until("the end after main work", || {
        h.ended(&s.id) == Some(EndOutcome::Closed)
    });
    assert!(h.sent(&s).is_empty());
}

#[test]
fn a_closing_delivery_confirmed_by_its_record_still_needs_its_stop() {
    let h = Harness::ready(|_| {});
    let s = h.idle(SessionKind::Authoring);
    h.bridge
        .end(&s.id, EndMode::FinalWrite, EndCause::Holder)
        .unwrap();
    wait_until("the closing paste", || h.sent(&s) == [CLOSING]);
    h.record(
        &s,
        json!({"type": "user", "origin": {"kind": "human"},
            "message": {"role": "user", "content": CLOSING}}),
    );
    wait_until("the closing confirmation", || {
        h.bridge
            .lock()
            .sessions
            .get(&s.id)
            .is_some_and(|session| session.closing() == Some(super::session::Closing::Confirmed))
    });
    settle();
    assert_eq!(h.ended(&s.id), None, "closed before the final Stop");

    h.spool(&s, "Stop", json!({}));
    wait_until("the final Stop", || {
        h.ended(&s.id) == Some(EndOutcome::Closed)
    });
}
