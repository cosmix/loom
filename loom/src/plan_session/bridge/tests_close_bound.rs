//! `Bridge::close_within` bounds the closing write by its own bound: a set aside waits that long,
//! never the configured `final_write_bound`.

use std::time::{Duration, Instant};

use crate::plan_session::bridge::fake_host::{wait_until, Call, Harness};
use crate::plan_session::bridge::{EndCause, EndOutcome};
use crate::plan_session::kind::SessionKind::Authoring;

/// A running turn the user wrote in, interrupted by the close and never answering the closing
/// message: with `final_write_bound` at 30 s, a 1 s close tears it down shortly after that
/// second. A close that planned a plain `end` would wait the configured 30 s.
#[test]
fn close_within_bounds_the_closing_write_by_its_own_bound() {
    let h = Harness::ready(|c| c.final_write_bound = Duration::from_secs(30));
    let s = h.idle(Authoring);
    h.working(&s);

    let asked = Instant::now();
    h.bridge
        .close_within(&s.id, EndCause::Holder, Duration::from_secs(1))
        .unwrap();
    assert_eq!(
        h.bridge.planned_end_bound(&s.id),
        Some(Duration::from_secs(1)),
        "the close planned an end without its own bound"
    );
    wait_until("the end", || h.ended(&s.id).is_some());

    let waited = asked.elapsed();
    assert!(waited >= Duration::from_secs(1), "ended early: {waited:?}");
    assert!(
        waited < Duration::from_secs(4),
        "ignored its bound: {waited:?}"
    );
    assert_eq!(h.ended(&s.id), Some(EndOutcome::TornDown));
    assert_eq!(h.host.count(&Call::Teardown(s.socket.clone())), 1);
}
