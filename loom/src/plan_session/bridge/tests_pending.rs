//! Bridge tests: the atomic session record and the pending input kept next to it.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex};
use std::thread;

use super::fake_host::{settle, wait_until, Harness};
use super::pending::read_pending;
use super::registry::{read_record, write_record, SessionRecord};
use super::{Bridge, BridgeError, BridgeEvent, SendOutcome};
use crate::plan_session::kind::SessionKind::Authoring;
use crate::process::IdentityStatus;

const TEXT: &str = "text";

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
