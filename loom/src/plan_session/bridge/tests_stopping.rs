//! Bridge tests: draining a summary before its opening and waiting for end publication.

use std::sync::{mpsc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::fake_host::*;
use super::{BridgeEvent, EndCause, EndMode, EndOutcome};
use crate::plan_session::kind::SessionKind::{self, Authoring};
use crate::plan_session::model::SessionState;
use crate::process::IdentityStatus;

#[test]
fn drain_tears_down_a_summary_session_whose_task_never_went_out() {
    let h = Harness::ready(|c| c.final_write_bound = Duration::from_secs(4));
    let s = h.start(SessionKind::Summary);
    wait_until("the start", || {
        h.count_events(|event| matches!(event, BridgeEvent::Started { id, .. } if id == &s.id)) == 1
    });
    let asked = Instant::now();
    h.bridge.drain();
    assert!(
        asked.elapsed() < Duration::from_secs(3),
        "drain waited for the bound"
    );
    assert_eq!(h.ended(&s.id), Some(EndOutcome::TornDown));
    assert_eq!(h.host.delivered(&s.socket), Vec::<String>::new());
}

#[test]
fn drain_waits_for_the_ended_event_to_reach_listeners() {
    let h = Harness::ready(|c| c.final_write_bound = Duration::from_secs(4));
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let release_rx = Mutex::new(release_rx);
    h.bridge.subscribe(Box::new(move |event| {
        if matches!(event, BridgeEvent::Started { .. }) {
            entered_tx.send(()).unwrap();
            release_rx
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(10))
                .unwrap();
        }
    }));
    let s = h.start(SessionKind::Summary);
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();

    let (done_tx, done_rx) = mpsc::channel();
    let bridge = h.bridge.clone();
    let drained = thread::spawn(move || {
        bridge.drain();
        done_tx.send(()).unwrap();
    });
    h.wait_state(&s, SessionState::Ending);
    // The driver is held in a listener; finish this session on the test thread.
    h.bridge.end_step(&s.id);
    let early = done_rx.recv_timeout(Duration::from_millis(500)).is_ok();
    release_tx.send(()).unwrap();
    drained.join().unwrap();

    assert!(!early, "drain returned before listeners saw Ended");
    assert_eq!(h.ended(&s.id), Some(EndOutcome::TornDown));
}

/// A `FinalWrite` end whose closing message nobody answers is hardened by a `Teardown`
/// `end_within`: the next `end_step` tears it down, well before either bound, with one `Ended`.
#[test]
fn a_teardown_end_within_hardens_a_final_write_end() {
    let h = Harness::ready(|c| c.final_write_bound = Duration::from_secs(30));
    let s = h.idle(Authoring);
    h.working(&s);
    h.bridge
        .end(&s.id, EndMode::FinalWrite, EndCause::Holder)
        .unwrap();
    assert_eq!(h.ended(&s.id), None, "ended before it was hardened");

    let asked = Instant::now();
    h.bridge
        .end_within(
            &s.id,
            EndMode::Teardown,
            EndCause::Caller,
            Duration::from_secs(20),
        )
        .unwrap();
    wait_until("the end", || h.ended(&s.id).is_some());
    assert!(
        asked.elapsed() < Duration::from_secs(4),
        "waited for a bound"
    );
    settle();
    assert_eq!(h.ended(&s.id), Some(EndOutcome::TornDown));
    let ended = |e: &BridgeEvent| matches!(e, BridgeEvent::Ended { id, .. } if *id == s.id);
    assert_eq!(h.count_events(|e| ended(e)), 1);
    // The first request set the plan; the hardening keeps its cause.
    assert_eq!(h.ended_cause(&s.id), Some(Some(EndCause::Holder)));
}

/// A later `end_within` lowers the bound but never raises it.
#[test]
fn a_later_end_within_never_raises_the_bound() {
    let h = Harness::ready(|c| c.final_write_bound = Duration::from_secs(30));
    let s = h.idle(Authoring);
    h.working(&s);
    for secs in [1, 20] {
        h.bridge
            .end_within(
                &s.id,
                EndMode::FinalWrite,
                EndCause::Holder,
                Duration::from_secs(secs),
            )
            .unwrap();
    }
    let bound = h.bridge.lock().sessions[&s.id]
        .flow
        .end
        .as_ref()
        .unwrap()
        .bound;
    assert_eq!(bound, Some(Duration::from_secs(1)));
}

#[test]
fn ended_carries_the_holder_that_asked_for_the_end() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    h.bridge
        .end(&s.id, EndMode::Teardown, EndCause::Holder)
        .unwrap();
    wait_until("the end", || h.ended(&s.id).is_some());
    assert_eq!(h.ended_cause(&s.id), Some(Some(EndCause::Holder)));
}

#[test]
fn ended_carries_the_agent_when_the_done_fence_asked_for_the_end() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    let text = "Written.\n\n```loom-done\n{\"decisions\":[],\"questions\":[]}\n```";
    h.record(
        &s,
        serde_json::json!({"type": "assistant", "message": {
            "id": uuid::Uuid::new_v4().to_string(), "role": "assistant",
            "content": [{"type": "text", "text": text}], "stop_reason": "end_turn"}}),
    );
    wait_until("the assistant text", || {
        h.bridge.lock().sessions.get(&s.id).is_some_and(|session| {
            let turns = session.mapper.turns();
            let items = turns.iter().flat_map(|turn| &turn.items);
            items.into_iter().any(|item| {
                matches!(item, crate::plan_session::model::Item::Text { markdown, .. }
                    if markdown == text)
            })
        })
    });
    h.spool(&s, "Stop", serde_json::json!({}));
    wait_until("the agent end", || h.ended(&s.id).is_some());
    assert_eq!(h.ended_cause(&s.id), Some(Some(EndCause::Agent)));
}

#[test]
fn ended_carries_no_cause_when_the_process_vanished() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.host.set_identity(s.pid, IdentityStatus::Dead);
    wait_until("the end", || h.ended(&s.id).is_some());
    assert_eq!(h.ended_cause(&s.id), Some(None));
}
