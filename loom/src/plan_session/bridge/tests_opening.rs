//! Bridge tests: an opening pasted but never submitted is re-entered, then fails a Summary
//! session; a kickoff opening with typed text is two sent items.
//!
//! `Call::Deliver` is one paste together with its own Enter; each further Enter the bridge
//! presses is one `Call::PressEnter(socket)`.

use std::thread;
use std::time::Duration;

use serde_json::json;

use super::fake_host::*;
use super::tests::wait_for;
use super::{EndOutcome, LaunchGuidance};
use crate::plan_session::kind::SessionDir;
use crate::plan_session::kind::SessionKind::{Authoring, Summary};
use crate::plan_session::model::{Delivery, Item, Origin, Turn};

const GUIDANCE: &str = "loom plan: Kickoff.";
const TYPED: &str = "add support for a microvm test environment";
const BLOCK: &str =
    "<loom-attachments>\n[1] source doc/product/demo/index.md\n> # Vision\n</loom-attachments>";

/// Start a Summary session and let its first `SessionStart` paste the opening.
fn pasted_opening(h: &Harness) -> Sess {
    let s = h.start(Summary);
    h.spool(&s, "SessionStart", json!({"source": "startup"}));
    wait_until("the opening", || h.host.delivered(&s.socket) == [OPENING]);
    s
}

#[test]
fn opening_enter_is_repressed() {
    let h = Harness::ready(|_| {});
    let s = pasted_opening(&h);
    thread::sleep(Duration::from_millis(4500));
    let presses = h.host.count(&Call::PressEnter(s.socket.clone()));
    assert_eq!(presses, 2, "Enter pressed again at 2 s and 4 s");
    assert_eq!(
        h.host.delivered(&s.socket),
        [OPENING],
        "the opening was pasted a second time"
    );
}

#[test]
fn unsubmitted_opening_fails_summary_session() {
    let h = Harness::ready(|c| c.delivery_timeout = Duration::from_millis(600));
    let s = pasted_opening(&h);
    let reason = wait_for("the failure", || match h.ended(&s.id) {
        Some(EndOutcome::Failed(reason)) => Some(reason),
        _ => None,
    });
    assert!(reason.contains("opening not submitted"), "{reason}");
    assert_eq!(h.host.delivered(&s.socket), [OPENING]);
}

fn typed_with_block() -> String {
    format!("{TYPED}\n\n{BLOCK}")
}

fn kickoff_opening() -> String {
    format!("{GUIDANCE}\n\n{}", typed_with_block())
}

/// A kickoff start whose guidance leaves the typed part where `SessionDir::opening_typed` names
/// it, the way the real provider does.
fn kickoff_start(h: &Harness) -> Sess {
    let opening = kickoff_opening();
    let mut request = h.request(Authoring, &opening, false);
    request.opening_origin = Some(Origin::Offer);
    request.guidance = Box::new(move |dir: &SessionDir| {
        std::fs::write(dir.opening_typed(), typed_with_block())?;
        Ok(LaunchGuidance {
            brief_path: dir.brief(),
            plugin_dir: dir.plugin(),
            opening_message: opening,
            model: "opus".to_owned(),
            effort: "medium".to_owned(),
            display_name: "loom plan: tests".to_owned(),
            opening_on_argv: false,
        })
    });
    let id = h.bridge.start_session(request).unwrap();
    h.sess(&id)
}

/// Text, chip count, origin and delivery of every user item.
fn user_items(turns: &[Turn]) -> Vec<(String, usize, Option<Origin>, Delivery)> {
    let items = turns.iter().flat_map(|turn| &turn.items);
    items
        .filter_map(|item| match item {
            Item::User {
                text,
                attachments,
                origin,
                delivery,
                ..
            } => Some((text.clone(), attachments.len(), *origin, *delivery)),
            _ => None,
        })
        .collect()
}

fn deliveries(turns: &[Turn]) -> Vec<Delivery> {
    user_items(turns).into_iter().map(|item| item.3).collect()
}

#[test]
fn a_kickoff_opening_with_typed_text_creates_two_sent_items() {
    let h = Harness::ready(|_| {});
    let s = kickoff_start(&h);
    h.spool(&s, "SessionStart", json!({"source": "startup"}));
    wait_until("the opening", || {
        h.host.delivered(&s.socket) == [kickoff_opening()]
    });

    let sent = user_items(&h.bridge.chat_snapshot(true).turns);
    let guidance = (GUIDANCE.to_owned(), 0, Some(Origin::Offer), Delivery::Sent);
    let typed = (TYPED.to_owned(), 1, None, Delivery::Sent);
    assert_eq!(sent, [guidance, typed]);

    h.spool(&s, "UserPromptSubmit", json!({"prompt": kickoff_opening()}));
    wait_until("both delivered", || {
        deliveries(&h.bridge.chat_snapshot(true).turns) == [Delivery::Delivered; 2]
    });
}

#[test]
fn a_failed_opening_delivery_marks_both_items() {
    let failed = Harness::ready(|_| {});
    failed.host.fail_deliver();
    let s = kickoff_start(&failed);
    failed.spool(&s, "SessionStart", json!({"source": "startup"}));
    wait_until("the failure", || {
        matches!(failed.ended(&s.id), Some(EndOutcome::Failed(_)))
    });
    let previous = failed.bridge.chat_snapshot(false).previous.unwrap();
    assert_eq!(deliveries(&previous.turns), [Delivery::Undelivered; 2]);

    let timed_out = Harness::ready(|c| c.delivery_timeout = Duration::from_millis(400));
    let s = kickoff_start(&timed_out);
    timed_out.spool(&s, "SessionStart", json!({"source": "startup"}));
    wait_until("both undelivered", || {
        deliveries(&timed_out.bridge.chat_snapshot(true).turns) == [Delivery::Undelivered; 2]
    });
}

#[test]
fn a_start_without_an_offer_keeps_its_chips_on_the_sent_item() {
    let h = Harness::ready(|_| {});
    let opening = typed_with_block();
    let id = h
        .bridge
        .start_session(h.request(Authoring, &opening, false))
        .unwrap();
    let s = h.sess(&id);
    h.spool(&s, "SessionStart", json!({"source": "startup"}));
    wait_until("the opening", || {
        h.host.delivered(&s.socket) == [opening.clone()]
    });

    let sent = user_items(&h.bridge.chat_snapshot(true).turns);
    let origin = Some(Origin::Start);
    assert_eq!(sent, [(TYPED.to_owned(), 1, origin, Delivery::Sent)]);
}
