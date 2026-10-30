//! Bridge tests: an opening pasted but never submitted is re-entered, then fails a Summary
//! session.
//!
//! `Call::Deliver` is one paste together with its own Enter; each further Enter the bridge
//! presses is one `Call::PressEnter(socket)`.

use std::thread;
use std::time::Duration;

use serde_json::json;

use super::fake_host::*;
use super::tests::wait_for;
use super::EndOutcome;
use crate::plan_session::kind::SessionKind::Summary;

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
