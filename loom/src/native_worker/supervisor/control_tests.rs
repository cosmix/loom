use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use tempfile::TempDir;

use super::{ControlEvent, ControlPoller};
use crate::native_worker::model::{CancelRequest, ControlMessage, Delivery, NativeManifest};
use crate::native_worker::store;

fn record() -> (TempDir, PathBuf) {
    let work = tempfile::tempdir().unwrap();
    let brief = b"brief";
    let manifest = NativeManifest {
        v: 1,
        stage_id: "stage".into(),
        unit_id: "unit".into(),
        invocation_id: format!("inv-{}", "0".repeat(32)),
        owner_session_id: "session".into(),
        workspace_root: work.path().to_path_buf(),
        brief_path: work.path().join("brief.md"),
        brief_sha256: crate::utils::sha256_hex(brief),
        model: "model".into(),
        effort: "high".into(),
        deadline_secs: 60,
        created_at: Utc::now(),
    };
    let dir = store::create_invocation(work.path(), &manifest, brief).unwrap();
    (work, dir)
}

fn cancel(reason: &str) -> CancelRequest {
    CancelRequest {
        reason: reason.into(),
        requested_at: Utc::now(),
        by: "operator".into(),
    }
}

fn message(id: &str, accepted_at: DateTime<Utc>) -> ControlMessage {
    let text = "message";
    ControlMessage {
        message_id: id.into(),
        delivery: Delivery::Steer,
        text: text.into(),
        sha256: crate::utils::sha256_hex(text.as_bytes()),
        accepted_at,
    }
}

fn write_message(dir: &Path, id: &str, accepted_at: DateTime<Utc>) {
    store::write_message(dir, &message(id, accepted_at)).unwrap();
}

#[test]
#[serial_test::serial]
fn new_record_reports_nothing() {
    let (_work, dir) = record();
    let (events, error) = ControlPoller::new(&dir).poll();
    assert!(events.is_empty());
    assert!(error.is_none());
}

#[test]
#[serial_test::serial]
fn cancel_is_reported_once() {
    let (_work, dir) = record();
    store::write_cancel(&dir, &cancel("stop")).unwrap();
    let mut poller = ControlPoller::new(&dir);
    let (events, error) = poller.poll();
    assert!(matches!(events.as_slice(), [ControlEvent::Cancel(_)]));
    assert!(error.is_none());
    let (events, error) = poller.poll();
    assert!(events.is_empty());
    assert!(error.is_none());
}

#[test]
#[serial_test::serial]
fn existing_cancel_is_not_replaced_or_repeated() {
    let (_work, dir) = record();
    store::write_cancel(&dir, &cancel("first")).unwrap();
    store::write_cancel(&dir, &cancel("second")).unwrap();
    let mut poller = ControlPoller::new(&dir);
    let (events, error) = poller.poll();
    assert!(matches!(events.as_slice(), [ControlEvent::Cancel(c)] if c.reason == "first"));
    assert!(error.is_none());
    let (events, error) = poller.poll();
    assert!(events.is_empty());
    assert!(error.is_none());
}

#[test]
#[serial_test::serial]
fn messages_come_in_accepted_at_order() {
    let (_work, dir) = record();
    let now = Utc::now();
    write_message(&dir, "z-first", now);
    write_message(&dir, "a-second", now + Duration::seconds(1));
    let (events, error) = ControlPoller::new(&dir).poll();
    assert!(
        matches!(events.as_slice(), [ControlEvent::Message(a), ControlEvent::Message(b)]
        if a.message_id == "z-first" && b.message_id == "a-second")
    );
    assert!(error.is_none());
}

#[test]
#[serial_test::serial]
fn each_message_is_reported_once() {
    let (_work, dir) = record();
    write_message(&dir, "one", Utc::now());
    let mut poller = ControlPoller::new(&dir);
    let (events, error) = poller.poll();
    assert!(matches!(events.as_slice(), [ControlEvent::Message(_)]));
    assert!(error.is_none());
    let (events, error) = poller.poll();
    assert!(events.is_empty());
    assert!(error.is_none());
}

#[test]
#[serial_test::serial]
fn late_message_is_reported_on_the_next_poll() {
    let (_work, dir) = record();
    let mut poller = ControlPoller::new(&dir);
    let (events, error) = poller.poll();
    assert!(events.is_empty());
    assert!(error.is_none());
    write_message(&dir, "late", Utc::now());
    let (events, error) = poller.poll();
    assert!(matches!(events.as_slice(), [ControlEvent::Message(m)] if m.message_id == "late"));
    assert!(error.is_none());
}

#[test]
#[serial_test::serial]
fn cancel_precedes_messages_in_one_poll() {
    let (_work, dir) = record();
    store::write_cancel(&dir, &cancel("stop")).unwrap();
    write_message(&dir, "one", Utc::now());
    let (events, error) = ControlPoller::new(&dir).poll();
    assert!(matches!(
        events.as_slice(),
        [ControlEvent::Cancel(_), ControlEvent::Message(_)]
    ));
    assert!(error.is_none());
}

#[test]
#[serial_test::serial]
fn repeated_identical_error_is_reported_once() {
    let (_work, dir) = record();
    std::fs::write(dir.join("control/cancel.json"), b"not json").unwrap();
    let mut poller = ControlPoller::new(&dir);
    let (events, error) = poller.poll();
    assert!(events.is_empty());
    assert!(error.is_some());
    let (events, error) = poller.poll();
    assert!(events.is_empty());
    assert!(error.is_none());
}

#[test]
#[serial_test::serial]
fn cancel_error_does_not_hide_messages_and_is_reported_again_after_recovery() {
    let (_work, dir) = record();
    let cancel_path = dir.join("control/cancel.json");
    std::fs::write(&cancel_path, b"not json").unwrap();
    write_message(&dir, "one", Utc::now());
    let mut poller = ControlPoller::new(&dir);
    let (events, error) = poller.poll();
    assert!(matches!(events.as_slice(), [ControlEvent::Message(m)] if m.message_id == "one"));
    assert!(error.is_some());
    std::fs::remove_file(&cancel_path).unwrap();
    let (events, error) = poller.poll();
    assert!(events.is_empty());
    assert!(error.is_none());
    std::fs::write(&cancel_path, b"not json").unwrap();
    assert!(poller.poll().1.is_some());
}

#[test]
#[serial_test::serial]
fn missing_control_directory_is_empty() {
    let (_work, dir) = record();
    std::fs::remove_dir_all(dir.join("control")).unwrap();
    let (events, error) = ControlPoller::new(&dir).poll();
    assert!(events.is_empty());
    assert!(error.is_none());
}
