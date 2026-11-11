//! Bridge tests: a delivery is confirmed by a hook prompt or a transcript record whose text
//! differs from the pasted text only in where Claude Code put its paste wrappers or in
//! whitespace.

use std::time::Duration;

use serde_json::json;

use super::fake_host::*;
use super::tests_queue::delivery;
use super::{EndCause, EndMode, EndOutcome};
use crate::plan_session::kind::SessionKind::Authoring;
use crate::plan_session::model::Delivery;

/// `CLOSE` with a paste wrapper amid it, as Claude Code expands a chunk in place.
const SPLIT_CLOSING: &str = "\n\n<pasted_content id=\"c1\">\nCL\n</pasted_content id=\"c1\">\nOSE";

fn timed_out() -> Harness {
    Harness::ready(|c| c.delivery_timeout = Duration::from_millis(400))
}

#[test]
fn a_late_hook_prompt_with_a_wrapper_amid_the_text_delivers_the_message() {
    let h = timed_out();
    let s = h.idle(Authoring);
    let text = "first line\nsecond line";
    h.bridge.send(&s.id, text.to_owned(), Vec::new()).unwrap();
    wait_until("undelivered", || {
        delivery(&h, text) == Some(Delivery::Undelivered)
    });
    let recorded = "\n\n<pasted_content id=\"5193\">\nfirst line\n</pasted_content id=\"5193\">\n\nsecond line";
    h.spool(&s, "UserPromptSubmit", json!({"prompt": recorded}));
    wait_until("delivered", || {
        delivery(&h, text) == Some(Delivery::Delivered)
    });
}

#[test]
fn a_late_hook_prompt_with_other_whitespace_delivers_the_message() {
    let h = timed_out();
    let s = h.idle(Authoring);
    let text = "  A  b \n";
    h.bridge.send(&s.id, text.to_owned(), Vec::new()).unwrap();
    wait_until("undelivered", || {
        delivery(&h, text) == Some(Delivery::Undelivered)
    });
    h.spool(&s, "UserPromptSubmit", json!({"prompt": "A b"}));
    wait_until("delivered", || {
        delivery(&h, text) == Some(Delivery::Delivered)
    });
}

/// The closing message times out, then its transcript record confirms it: the Stop then ends
/// the session as closed, which it only does for a confirmed closing message.
fn closing_confirmed_late_by(content: &str) {
    let h = timed_out();
    let s = h.idle(Authoring);
    h.bridge
        .end(&s.id, EndMode::FinalWrite, EndCause::Holder)
        .unwrap();
    wait_until("the closing message", || h.sent(&s) == [CLOSING]);
    wait_until("the closing undelivered", || {
        delivery(&h, CLOSING) == Some(Delivery::Undelivered)
    });
    let user = json!({"type": "user", "origin": {"kind": "human"},
        "message": {"role": "user", "content": content}});
    h.record(&s, user);
    settle();
    h.spool(&s, "Stop", json!({}));
    wait_until("the end", || h.ended(&s.id) == Some(EndOutcome::Closed));
}

#[test]
fn a_transcript_record_with_a_wrapper_amid_the_closing_text_confirms_it_late() {
    closing_confirmed_late_by(SPLIT_CLOSING);
}

#[test]
fn a_transcript_record_with_padded_closing_text_confirms_it_late() {
    closing_confirmed_late_by("\n CLOSE \n");
}
