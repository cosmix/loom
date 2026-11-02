//! Bridge tests: draining a summary before its opening and waiting for end publication.

use std::sync::{mpsc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::fake_host::*;
use super::{BridgeEvent, EndOutcome};
use crate::plan_session::kind::SessionKind;
use crate::plan_session::model::SessionState;

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
