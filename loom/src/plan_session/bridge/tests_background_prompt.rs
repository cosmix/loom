//! Bridge tests: the prompt Claude Code submits for a background task's notification, an
//! agent's hand-back or a teammate's message opens a turn but never confirms the message the
//! bridge pasted.

use serde_json::{json, Value};

use super::fake_host::*;
use super::tests_queue::delivery;
use super::SendOutcome;
use crate::plan_session::kind::SessionKind::Authoring;
use crate::plan_session::model::{Delivery, SessionState};

/// The first background prompt (a leading `<`) of a recorded probe spool.
fn recorded_prompt(spool: &str) -> String {
    spool
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|event| event["event"] == "UserPromptSubmit")
        .filter_map(|event| event["payload"]["prompt"].as_str().map(str::to_owned))
        .find(|prompt| prompt.starts_with('<'))
        .expect("the probe spool holds a background prompt")
}

/// Idle, "A" pasted; then Claude Code starts the background turn before submitting it.
fn pasted_then_background_turn(prompt: &str) -> (Harness, Sess) {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    let sent = h.bridge.send(&s.id, "A".to_owned(), Vec::new()).unwrap();
    assert!(matches!(sent, SendOutcome::Delivering { .. }), "{sent:?}");
    wait_until("A pasted", || h.sent(&s) == ["A"]);
    h.spool(&s, "UserPromptSubmit", json!({ "prompt": prompt }));
    h.wait_state(&s, SessionState::Working);
    settle();
    (h, s)
}

#[test]
fn a_background_prompt_does_not_confirm_the_pasted_message() {
    let prompts = [
        include_str!("../../../tests/fixtures/plan_session/spool/p1-background-bash.jsonl"),
        include_str!("../../../tests/fixtures/plan_session/spool/p1-background-agent.jsonl"),
    ]
    .map(recorded_prompt);
    assert!(
        prompts[0].starts_with("<task-notification>"),
        "{}",
        prompts[0]
    );
    assert!(prompts[1].starts_with("<agent-message"), "{}", prompts[1]);
    for prompt in prompts {
        let (h, s) = pasted_then_background_turn(&prompt);
        assert_eq!(
            delivery(&h, "A"),
            Some(Delivery::Sent),
            "the background turn's prompt confirmed A: {prompt:.40}"
        );
        h.spool(&s, "UserPromptSubmit", json!({"prompt": "A"}));
        wait_until("A delivered by its own prompt", || {
            delivery(&h, "A") == Some(Delivery::Delivered)
        });
    }
}

/// Idle, the user's own message starting with a background tag pasted.
fn pasted_tagged_message(timeout_ms: u64) -> (Harness, Sess, String) {
    let timeout = std::time::Duration::from_millis(timeout_ms);
    let h = Harness::ready(|c| c.delivery_timeout = timeout);
    let s = h.idle(Authoring);
    h.bridge.send(&s.id, TAGGED.to_owned(), Vec::new()).unwrap();
    wait_until("the message pasted", || h.sent(&s).len() == 1);
    let pasted = h.sent(&s)[0].clone();
    (h, s, pasted)
}

const TAGGED: &str = "<task-notification> is what the hand-back starts with";

#[test]
fn a_message_that_starts_with_a_background_tag_is_confirmed_by_its_prompt() {
    let (h, s, pasted) = pasted_tagged_message(10_000);
    h.spool(&s, "UserPromptSubmit", json!({ "prompt": pasted }));
    wait_until("the message delivered", || {
        delivery(&h, TAGGED) == Some(Delivery::Delivered)
    });
}

#[test]
fn a_message_that_starts_with_a_background_tag_is_confirmed_late() {
    let (h, s, pasted) = pasted_tagged_message(400);
    wait_until("the message undelivered", || {
        delivery(&h, TAGGED) == Some(Delivery::Undelivered)
    });
    h.spool(&s, "UserPromptSubmit", json!({ "prompt": pasted }));
    wait_until("the message delivered late", || {
        delivery(&h, TAGGED) == Some(Delivery::Delivered)
    });
}

#[test]
fn a_message_held_behind_a_background_turn_is_confirmed_late() {
    let prompt = "<task-notification>\n<task-id>b1</task-id>\n</task-notification>";
    let h = Harness::ready(|c| c.delivery_timeout = std::time::Duration::from_millis(400));
    let s = h.idle(Authoring);
    h.bridge.send(&s.id, "A".to_owned(), Vec::new()).unwrap();
    wait_until("A pasted", || h.sent(&s) == ["A"]);
    h.spool(&s, "UserPromptSubmit", json!({ "prompt": prompt }));
    wait_until("A undelivered", || {
        delivery(&h, "A") == Some(Delivery::Undelivered)
    });
    h.spool(&s, "Stop", json!({}));
    h.wait_state(&s, SessionState::Idle);
    // Claude Code submits the held message after the background turn's Stop.
    h.spool(&s, "UserPromptSubmit", json!({"prompt": "A"}));
    wait_until("A delivered late", || {
        delivery(&h, "A") == Some(Delivery::Delivered)
    });
    settle();
    assert_eq!(h.sent(&s), ["A"], "A was pasted twice");
}

#[test]
fn a_teammate_message_prompt_does_not_confirm_the_pasted_message() {
    let prompt = "Another Claude session sent a message:\n<teammate-message teammate_id=\"slash\">\nDONE\n</teammate-message>";
    let (h, s) = pasted_then_background_turn(prompt);
    assert_eq!(
        delivery(&h, "A"),
        Some(Delivery::Sent),
        "the teammate turn's prompt confirmed A"
    );
    h.spool(&s, "UserPromptSubmit", json!({"prompt": "A"}));
    wait_until("A delivered by its own prompt", || {
        delivery(&h, "A") == Some(Delivery::Delivered)
    });
}
