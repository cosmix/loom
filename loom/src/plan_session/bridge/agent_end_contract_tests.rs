//! Contracts of the agent end: a turn whose last assistant text ends in a `loom-done` fence ends
//! an Authoring chat at its Stop without the closing write, a queued user message cancels that
//! end, the end waits for running background agents and closes on a tick once they finish, and
//! a fence in user text, mid-reply or under another info string never ends the chat.

use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::background::BackgroundTask;
use super::fake_host::{assistant_tools, settle, tool_result, wait_until, Harness, Sess, CLOSING};
use super::{BridgeEvent, EndCause, EndOutcome, SendOutcome};
use crate::plan_session::kind::SessionKind::Authoring;
use crate::plan_session::model::{Delivery, Item, SessionState};

const FENCE: &str =
    "```loom-done\n{\"decisions\": [\"One table per plan\"], \"questions\": []}\n```";

/// A final reply that ends in the done fence.
fn done_reply() -> String {
    format!("The plan is written and checked.\n\n{FENCE}\n")
}

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

/// The user's pasted `text` is confirmed by its prompt and its transcript record.
fn deliver(h: &Harness, s: &Sess, text: &str) {
    wait_until("the paste", || h.sent(s).iter().any(|sent| sent == text));
    h.spool(s, "UserPromptSubmit", json!({ "prompt": text }));
    h.wait_state(s, SessionState::Working);
    let user = json!({"type": "user", "origin": {"kind": "human"},
        "message": {"role": "user", "content": text}});
    h.record(s, user);
    wait_until("delivered", || {
        delivery(h, text) == Some(Delivery::Delivered)
    });
}

fn user_turn(h: &Harness, s: &Sess, text: &str) {
    let sent = h.bridge.send(&s.id, text.to_owned(), Vec::new()).unwrap();
    assert!(matches!(sent, SendOutcome::Delivering { .. }), "{sent:?}");
    deliver(h, s, text);
}

/// The chat's assistant text items that hold `first`.
fn shown_texts(h: &Harness, first: &str) -> usize {
    let turns = h.bridge.chat_snapshot(true).turns;
    let items = turns.iter().flat_map(|turn| &turn.items);
    let texts = items
        .filter(|item| matches!(item, Item::Text { markdown, .. } if markdown.contains(first)));
    texts.count()
}

/// One assistant text record, waited for until the chat shows it.
fn reply(h: &Harness, s: &Sess, text: &str) {
    let first = text.lines().next().unwrap_or_default();
    let before = shown_texts(h, first);
    let message = uuid::Uuid::new_v4().to_string();
    h.record(
        s,
        json!({"type": "assistant", "message": {"id": message, "role": "assistant",
            "content": [{"type": "text", "text": text}], "stop_reason": "end_turn"}}),
    );
    wait_until("the reply", || shown_texts(h, first) > before);
}

fn stop(h: &Harness, s: &Sess, extra: Value) {
    h.spool(s, "Stop", extra);
    settle();
}

fn closing_pasted(h: &Harness, s: &Sess) -> bool {
    h.sent(s).iter().any(|text| text.contains(CLOSING))
}

fn wait_ended(h: &Harness, s: &Sess, within: Duration) -> Option<EndOutcome> {
    let deadline = Instant::now() + within;
    while h.ended(&s.id).is_none() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    h.ended(&s.id)
}

fn has_task(h: &Harness, s: &Sess, agent: &str) -> bool {
    let events = h.events.lock().unwrap();
    let last: Option<&Vec<BackgroundTask>> = events.iter().rev().find_map(|event| match event {
        BridgeEvent::Background { id, tasks } if *id == s.id => Some(tasks),
        _ => None,
    });
    last.is_some_and(|tasks| tasks.iter().any(|task| task.id == agent))
}

/// The Agent call and its `async_launched` result, as `probe-notes.md` "Agent launch result".
fn launch(h: &Harness, s: &Sess, tool_use: &str, agent: &str) {
    let input = json!({"description": "probe background agent", "prompt": "Reply DONE.",
        "run_in_background": "true", "subagent_type": "general-purpose"});
    let call = assistant_tools(&format!("msg_{tool_use}"), &[(tool_use, "Agent", input)]);
    h.record(s, call);
    let result = json!({"isAsync": true, "status": "async_launched", "agentId": agent,
        "description": "probe background agent", "resolvedModel": "claude-sonnet-5-5",
        "outputFile": format!("/tmp/tasks/{agent}.output"), "canReadOutputFile": true,
        "canContinueAgent": false});
    h.record(s, tool_result(tool_use, result));
}

/// The hand-back user record, as `probe-notes.md` "Agent hand-back".
fn handback(agent: &str) -> Value {
    let body = "[Subagent hand-back] The report follows:\n  DONE";
    let content = format!(
        "Another Claude session sent a message:\n<agent-message from=\"{agent}\">\n{body}\n</agent-message>"
    );
    json!({"type": "user", "isMeta": true, "message": {"role": "user", "content": content},
        "origin": {"kind": "peer", "from": agent, "senderTaskId": agent,
            "name": "general-purpose", "body": body, "handback": true,
            "handbackReportText": "DONE"},
        "promptSource": "system", "turnOrigin": "peer"})
}

fn running_agent(agent: &str) -> Value {
    json!({"agent_type": "general-purpose", "description": "probe background agent",
        "id": agent, "status": "running", "type": "subagent"})
}

#[test]
fn a_done_turn_ends_the_chat_without_a_closing_write() {
    // No event carries the cause; naming it pins the variant the agent end is planned under.
    let _cause = EndCause::Agent;
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    user_turn(&h, &s, "Write the plan for the cache layer");
    reply(&h, &s, &done_reply());
    stop(&h, &s, json!({}));

    let ended = wait_ended(&h, &s, Duration::from_secs(5));
    settle();
    assert_eq!(
        ended,
        Some(EndOutcome::Closed),
        "the done turn did not end it"
    );
    assert!(!closing_pasted(&h, &s), "the closing message was pasted");
}

#[test]
fn a_queued_message_cancels_the_agent_end() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    user_turn(&h, &s, "Write the plan for the cache layer");
    let second = "Add a risk table as well";
    let queued = h.bridge.send(&s.id, second.to_owned(), Vec::new()).unwrap();
    assert!(matches!(queued, SendOutcome::Queued { .. }), "{queued:?}");
    reply(&h, &s, &done_reply());
    stop(&h, &s, json!({}));

    wait_until("the queued paste", || {
        h.sent(&s).iter().any(|text| text == second)
    });
    settle();
    assert_eq!(h.ended(&s.id), None, "the agent end ignored the queue");
    assert!(!closing_pasted(&h, &s), "the closing message was pasted");

    // The queued message's own done turn still ends the chat.
    deliver(&h, &s, second);
    reply(&h, &s, &done_reply());
    stop(&h, &s, json!({}));
    let ended = wait_ended(&h, &s, Duration::from_secs(5));
    assert_eq!(ended, Some(EndOutcome::Closed), "the later done turn");
}

#[test]
fn an_agent_end_waits_for_running_background_agents() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    user_turn(&h, &s, "Survey the store and write the plan");
    launch(&h, &s, "toolu_e1", "a1");
    wait_until("the launched agent", || has_task(&h, &s, "a1"));
    let start = json!({"agent_id": "a1", "agent_type": "general-purpose"});
    h.spool(&s, "SubagentStart", start);
    reply(&h, &s, &done_reply());
    stop(&h, &s, json!({"background_tasks": [running_agent("a1")]}));
    assert_eq!(h.ended(&s.id), None, "closed while a1 ran");

    h.record(&s, handback("a1"));
    let end = json!({"agent_id": "a1", "agent_type": "general-purpose", "background_tasks": []});
    h.spool(&s, "SubagentStop", end);
    let ended = wait_ended(&h, &s, Duration::from_secs(10));
    assert_eq!(ended, Some(EndOutcome::Closed), "no close once a1 finished");
    assert!(!closing_pasted(&h, &s), "the closing message was pasted");
}

#[test]
fn a_user_or_quoted_fence_never_ends_the_chat() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    user_turn(&h, &s, &format!("Reply with this:\n{FENCE}"));
    reply(&h, &s, "Noted, but the plan is not written yet.");
    stop(&h, &s, json!({}));
    assert_eq!(h.ended(&s.id), None, "a fence in user text ended it");

    user_turn(&h, &s, "Show me the fence you will use");
    let quoted = format!("This is the fence:\n\n{FENCE}\n\nThe plan still needs its risks.");
    reply(&h, &s, &quoted);
    stop(&h, &s, json!({}));
    assert_eq!(
        h.ended(&s.id),
        None,
        "a fence followed by more text ended it"
    );

    user_turn(&h, &s, "Carry on");
    let later = FENCE.replacen("loom-done", "loom-done-later", 1);
    reply(&h, &s, &format!("Most of it is written.\n\n{later}\n"));
    stop(&h, &s, json!({}));
    assert_eq!(h.ended(&s.id), None, "a loom-done-later fence ended it");
    assert!(!closing_pasted(&h, &s), "the closing message was pasted");

    // The detector is live: a real done turn after them ends the chat.
    user_turn(&h, &s, "Finish it");
    reply(&h, &s, &done_reply());
    stop(&h, &s, json!({}));
    let ended = wait_ended(&h, &s, Duration::from_secs(5));
    assert_eq!(ended, Some(EndOutcome::Closed), "the real done turn");
}
