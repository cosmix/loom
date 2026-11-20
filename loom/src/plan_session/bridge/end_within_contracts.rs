//! Contract: `Bridge::end_within` bounds the closing write by its own bound, not by the
//! configured `final_write_bound`.

use std::time::{Duration, Instant};

use super::fake_host::{wait_until, Call, Harness};
use super::{EndCause, EndMode, EndOutcome};
use crate::plan_session::kind::SessionKind::Authoring;

/// Contract `end-within-bounds-the-closing-write`: an authoring session the user wrote in never
/// answers the closing message. With `final_write_bound` at 30 s, `end_within` with a 1 s bound
/// tears it down shortly after that second; an `end_within` that ignores its bound would wait
/// the configured 30 s. The bridge keeps real time (no fake clock), so the test waits for the
/// driver's own `end_step` and asserts the end came well inside the configured bound.
#[test]
fn end_within_bounds_the_closing_write() {
    let h = Harness::ready(|c| c.final_write_bound = Duration::from_secs(30));
    let s = h.idle(Authoring);
    // The user wrote in the session, so the closing message is pasted and never answered.
    h.working(&s);

    let asked = Instant::now();
    h.bridge
        .end_within(
            &s.id,
            EndMode::FinalWrite,
            EndCause::Holder,
            Duration::from_secs(1),
        )
        .unwrap();
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
