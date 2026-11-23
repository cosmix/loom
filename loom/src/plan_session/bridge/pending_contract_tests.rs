//! Contract of the queued send: a message the bridge accepted while a turn ran is on disk
//! before the send is acknowledged, so a server that dies right after it loses nothing. The
//! next server's recovery reports it as `InputReturned` and never pastes it. Persisting the
//! queue only from a later driver tick leaves a window in which the acknowledged send is lost.

use std::sync::{Arc, Mutex};
use std::thread;

use super::fake_host::{wait_until, Harness};
use super::{Bridge, BridgeEvent, SendOutcome};
use crate::plan_session::kind::SessionKind::Authoring;
use crate::process::IdentityStatus;

const KEPT: &str = "keep this";

#[test]
fn a_queued_send_is_returned_by_a_rebuilt_bridge() {
    let h = Harness::ready(|config| config.final_write_bound = std::time::Duration::from_secs(1));
    let s = h.idle(Authoring);
    h.working(&s);
    let sent = h.bridge.send(&s.id, KEPT.to_owned(), Vec::new()).unwrap();
    assert!(matches!(sent, SendOutcome::Queued { .. }), "{sent:?}");

    // The first server dies here: holding its lock keeps its driver from ever ticking again,
    // so nothing but the send itself can have put the text on disk.
    let frozen = h.bridge.lock();
    h.host.set_identity(s.pid, IdentityStatus::Dead);
    let second = Bridge::new(h.bridge.config.clone(), h.host.clone(), None);
    let heard: Arc<Mutex<Vec<BridgeEvent>>> = Arc::default();
    let seen = heard.clone();
    second.subscribe(Box::new(move |event| {
        seen.lock().unwrap().push(event.clone())
    }));
    let recovering = {
        let second = second.clone();
        thread::spawn(move || second.recover())
    };

    let returned = BridgeEvent::InputReturned {
        id: s.id.clone(),
        texts: vec![KEPT.to_owned()],
    };
    wait_until("the returned input", || {
        heard.lock().unwrap().contains(&returned)
    });
    let pasted = h.host.delivered(&s.socket);
    assert!(
        !pasted.iter().any(|text| text.contains(KEPT)),
        "pasted: {pasted:?}"
    );

    recovering.join().unwrap();
    let pasted = h.host.delivered(&s.socket);
    assert!(
        !pasted.iter().any(|text| text.contains(KEPT)),
        "pasted: {pasted:?}"
    );
    drop(frozen);
}
