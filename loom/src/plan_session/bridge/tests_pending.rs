//! Bridge tests: the atomic session record and the pending input kept next to it.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use super::fake_host::{settle, wait_until, Harness, Sess};
use super::pending::read_pending;
use super::registry::{read_record, write_record, SessionRecord};
use super::{input_unsaved, Bridge, BridgeError, BridgeEvent, SendOutcome};
use crate::plan_session::kind::SessionKind::Authoring;
use crate::process::IdentityStatus;

const TEXT: &str = "text";

/// Whether a session directory existed, and the pending texts it held.
type Kept = (bool, Vec<String>);

#[test]
fn a_symlinked_record_is_replaced_with_the_newest_generation() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    settle();
    let (record, copy) = (s.dir.record(), s.dir.path().join("record.copy"));
    fs::copy(&record, &copy).unwrap();
    fs::remove_file(&record).unwrap();
    std::os::unix::fs::symlink(&copy, &record).unwrap();

    h.bridge
        .set_owner(&s.id, Some("alpha".to_owned()), None)
        .unwrap();

    wait_until("the record replaced", || {
        let plain = fs::symlink_metadata(&record).is_ok_and(|meta| meta.is_file());
        plain && read_record(&s.dir).is_ok_and(|read| read.feature.as_deref() == Some("alpha"))
    });
}

#[test]
fn a_failed_record_write_keeps_the_previous_record() {
    // SAFETY: geteuid has no arguments and only reads process credentials.
    if unsafe { libc::geteuid() } == 0 {
        return; // root writes into a read-only directory, so there is no failure to keep
    }
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    settle();
    let first = read_record(&s.dir).unwrap();
    let next = SessionRecord {
        feature: Some("beta".to_owned()),
        ..first.clone()
    };

    fs::set_permissions(s.dir.path(), fs::Permissions::from_mode(0o500)).unwrap();
    let written = write_record(&s.dir, &next);
    let read = read_record(&s.dir);
    fs::set_permissions(s.dir.path(), fs::Permissions::from_mode(0o700)).unwrap();

    assert!(
        written.is_err(),
        "the write into a read-only directory worked"
    );
    assert_eq!(read.unwrap(), first);
}

#[test]
fn a_busy_send_is_saved_before_it_is_acknowledged() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);

    let sent = h.bridge.send(&s.id, TEXT.to_owned(), Vec::new()).unwrap();

    assert!(matches!(sent, SendOutcome::Queued { .. }), "{sent:?}");
    assert_eq!(read_pending(&s.dir), [TEXT]);
}

#[test]
fn a_send_that_cannot_be_saved_is_refused() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    let blocker = s.dir.path().join("pending.json");
    fs::create_dir(&blocker).unwrap();
    fs::write(blocker.join("kept"), b"kept").unwrap();

    let sent = h.bridge.send(&s.id, TEXT.to_owned(), Vec::new());

    assert!(matches!(sent, Err(BridgeError::Io(_))), "{sent:?}");
    assert!(h.bridge.chat_snapshot(true).queue.is_empty());
}

#[test]
fn a_withdrawn_message_leaves_the_file() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    let sent = h.bridge.send(&s.id, TEXT.to_owned(), Vec::new()).unwrap();
    let SendOutcome::Queued { queue_id } = sent else {
        panic!("not queued: {sent:?}");
    };
    assert_eq!(read_pending(&s.dir), [TEXT]);

    h.bridge.withdraw(&s.id, &queue_id).unwrap();

    let file = s.dir.path().join("pending.json");
    wait_until("the emptied file", || {
        fs::read_to_string(&file).is_ok_and(|text| text == r#"{"texts":[]}"#)
    });
}

#[test]
fn a_pasted_but_unconfirmed_message_is_returned_too() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    let sent = h.bridge.send(&s.id, TEXT.to_owned(), Vec::new()).unwrap();
    assert!(matches!(sent, SendOutcome::Delivering { .. }), "{sent:?}");
    wait_until("the paste", || {
        h.host.delivered(&s.socket).iter().any(|text| text == TEXT)
    });

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
        texts: vec![TEXT.to_owned()],
    };
    wait_until("the returned input", || {
        heard.lock().unwrap().contains(&returned)
    });
    recovering.join().unwrap();
}

/// A dead session whose accepted text was pasted and never confirmed, and a second bridge
/// that has not recovered yet.
fn dead_session() -> (Harness, Sess, Arc<Bridge>) {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.bridge.send(&s.id, TEXT.to_owned(), Vec::new()).unwrap();
    wait_until("the paste", || {
        h.host.delivered(&s.socket).iter().any(|text| text == TEXT)
    });
    h.host.set_identity(s.pid, IdentityStatus::Dead);
    let second = Bridge::new(h.bridge.config.clone(), h.host.clone(), None);
    (h, s, second)
}

/// The accepted text must be saved elsewhere before recovery removes the session's copy: a
/// kill between the two would lose it.
#[test]
fn the_returned_input_is_handed_over_before_its_directory_is_removed() {
    let (_h, s, second) = dead_session();
    let dir = s.dir.clone();
    let seen: Arc<Mutex<Vec<Kept>>> = Arc::default();
    let record = seen.clone();
    second.subscribe(Box::new(move |event| {
        if matches!(event, BridgeEvent::InputReturned { .. }) {
            let kept = (dir.path().is_dir(), read_pending(&dir));
            record.lock().unwrap().push(kept);
        }
    }));

    let report = second.recover();

    assert_eq!(*seen.lock().unwrap(), [(true, vec![TEXT.to_owned()])]);
    assert_eq!(report.torn_down.len(), 1, "{report:?}");
    assert!(!s.dir.path().exists(), "the directory stayed");
}

#[test]
fn an_unsaved_return_keeps_the_directory_for_the_next_restart() {
    let (_h, s, second) = dead_session();
    second.subscribe(Box::new(|event| {
        if matches!(event, BridgeEvent::InputReturned { .. }) {
            input_unsaved();
        }
    }));

    let report = second.recover();

    assert!(report.torn_down.is_empty(), "{report:?}");
    assert!(
        report
            .left_alone
            .iter()
            .any(|(id, _, why)| *id == s.id && why.contains("could not be saved")),
        "{report:?}"
    );
    assert_eq!(read_pending(&s.dir), [TEXT], "the text is gone");
}

/// A listener that dies on the returned input may not have saved it, so the copy stays.
#[test]
fn a_listener_that_panics_on_the_returned_input_keeps_the_directory() {
    let (_h, s, second) = dead_session();
    second.subscribe(Box::new(|event| {
        if matches!(event, BridgeEvent::InputReturned { .. }) {
            panic!("the listener failed");
        }
    }));

    let report = second.recover();

    assert!(report.torn_down.is_empty(), "{report:?}");
    assert!(
        report
            .left_alone
            .iter()
            .any(|(id, _, why)| *id == s.id && why.contains("could not be saved")),
        "{report:?}"
    );
    assert_eq!(read_pending(&s.dir), [TEXT], "the text is gone");
}

/// The listeners hear events from one thread at a time: recovery's returned input waits
/// while another thread is flushing instead of publishing beside it.
#[test]
fn the_returned_input_waits_for_the_thread_that_is_flushing() {
    let (_h, _s, second) = dead_session();
    let heard: Arc<Mutex<Vec<BridgeEvent>>> = Arc::default();
    let seen = heard.clone();
    second.subscribe(Box::new(move |event| {
        seen.lock().unwrap().push(event.clone())
    }));
    second.flushing.store(true, Ordering::SeqCst);
    let recovering = {
        let second = second.clone();
        thread::spawn(move || second.recover())
    };

    thread::sleep(Duration::from_millis(300));
    let early = heard.lock().unwrap().len();
    second.flushing.store(false, Ordering::SeqCst);
    let report = recovering.join().unwrap();

    assert_eq!(early, 0, "the input was published during another flush");
    assert_eq!(report.torn_down.len(), 1, "{report:?}");
    let returned = |event: &&BridgeEvent| matches!(event, BridgeEvent::InputReturned { .. });
    assert_eq!(heard.lock().unwrap().iter().filter(returned).count(), 1);
}
