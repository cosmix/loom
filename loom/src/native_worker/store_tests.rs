use super::super::model::{Delivery, NATIVE_DEADLINE_SECS};
use super::*;
use std::os::unix::fs::{symlink, PermissionsExt};
use tempfile::TempDir;

const INV: &str = "inv-0123456789abcdef0123456789abcdef";
const BRIEF: &[u8] = b"worker brief\n";

fn manifest(unit: &str, inv: &str, brief: &[u8]) -> NativeManifest {
    NativeManifest {
        v: 1,
        stage_id: "native-core".into(),
        unit_id: unit.into(),
        invocation_id: inv.into(),
        owner_session_id: "session-1".into(),
        workspace_root: PathBuf::from("/workspace"),
        brief_path: PathBuf::from("/workspace/brief.md"),
        brief_sha256: sha256_hex(brief),
        model: "gpt-5.4".into(),
        effort: "high".into(),
        deadline_secs: NATIVE_DEADLINE_SECS,
        created_at: Utc::now(),
    }
}

fn invocation() -> (TempDir, PathBuf) {
    let work = TempDir::new().unwrap();
    let dir = create_invocation(work.path(), &manifest("unit", INV, BRIEF), BRIEF).unwrap();
    (work, dir)
}

fn message(id: &str) -> ControlMessage {
    ControlMessage {
        message_id: id.into(),
        delivery: Delivery::NextTurn,
        text: "follow up".into(),
        sha256: sha256_hex(b"follow up"),
        accepted_at: Utc::now(),
    }
}

#[test]
fn create_invocation_writes_manifest_brief_and_admitted_event() {
    let work = TempDir::new().unwrap();
    let expected = manifest("unit", INV, BRIEF);
    let dir = create_invocation(work.path(), &expected, BRIEF).unwrap();
    assert_eq!(read_manifest(&dir).unwrap(), expected);
    assert_eq!(fs::read(dir.join("brief.md")).unwrap(), BRIEF);
    let events = read_events(&dir).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].seq, 1);
    assert_eq!(events[0].event, NativeEvent::Admitted);
}

#[test]
fn directories_are_0700_and_files_0600() {
    let (work, dir) = invocation();
    for path in [
        work.path().join("native-workers"),
        stage_dir(work.path(), "native-core"),
        dir.clone(),
        dir.join("control"),
    ] {
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
    write_message(&dir, &message("msg")).unwrap();
    write_report(&dir, "report").unwrap();
    for name in [
        "manifest.json",
        "brief.md",
        "events.jsonl",
        "control/msg-msg.json",
        "report.md",
    ] {
        let mode = fs::metadata(dir.join(name)).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[test]
fn create_invocation_twice_is_already_exists() {
    let (work, _) = invocation();
    let result = create_invocation(work.path(), &manifest("unit", INV, BRIEF), BRIEF);
    assert!(matches!(result, Err(StoreError::AlreadyExists)));
}

#[test]
fn create_invocation_rejects_traversal_unit_id() {
    let work = TempDir::new().unwrap();
    let bad = manifest("../escape", INV, BRIEF);
    let result = create_invocation(work.path(), &bad, BRIEF);
    assert!(matches!(result, Err(StoreError::UnsafePath(_))));
    assert!(!work.path().join("native-workers").exists());
}

#[test]
fn create_invocation_rejects_sha_mismatch() {
    let work = TempDir::new().unwrap();
    let bad = manifest("unit", INV, b"other");
    assert!(matches!(
        create_invocation(work.path(), &bad, BRIEF),
        Err(StoreError::Corrupt(_))
    ));
}

#[test]
fn create_leaves_no_temporary_directory() {
    let (work, _) = invocation();
    let names: Vec<_> = fs::read_dir(stage_dir(work.path(), "native-core"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, [std::ffi::OsString::from(INV)]);
}

#[test]
fn event_log_append_assigns_consecutive_seq() {
    let (_work, dir) = invocation();
    let mut log = EventLog::open(&dir).unwrap();
    assert_eq!(log.append(NativeEvent::Progress).unwrap().seq, 2);
    assert_eq!(log.append(NativeEvent::Progress).unwrap().seq, 3);
    assert_eq!(read_events(&dir).unwrap().len(), 3);
}

#[test]
fn event_log_reopen_continues_the_sequence() {
    let (_work, dir) = invocation();
    let mut log = EventLog::open(&dir).unwrap();
    log.append(NativeEvent::Progress).unwrap();
    // Opening again must not wait for a lock held by the previous handle.
    let mut reopened = EventLog::open(&dir).unwrap();
    assert_eq!(reopened.append(NativeEvent::Progress).unwrap().seq, 3);
}

#[test]
fn event_log_open_refuses_symlink() {
    let (work, dir) = invocation();
    let target = work.path().join("target");
    fs::write(&target, b"do not touch").unwrap();
    fs::remove_file(dir.join("events.jsonl")).unwrap();
    symlink(&target, dir.join("events.jsonl")).unwrap();
    assert!(matches!(
        EventLog::open(&dir),
        Err(StoreError::UnsafePath(_))
    ));
    assert_eq!(fs::read(target).unwrap(), b"do not touch");
}

#[test]
fn read_events_rejects_gap_in_seq() {
    let (_work, dir) = invocation();
    let record = EventRecord {
        seq: 2,
        at: Utc::now(),
        event: NativeEvent::Admitted,
    };
    let line = format!("{}\n", serde_json::to_string(&record).unwrap());
    fs::write(dir.join("events.jsonl"), line).unwrap();
    assert!(matches!(read_events(&dir), Err(StoreError::Corrupt(s)) if s.starts_with("line 1:")));
}

#[test]
fn read_events_rejects_unparsable_line() {
    let (_work, dir) = invocation();
    let original = fs::read_to_string(dir.join("events.jsonl")).unwrap();
    fs::write(dir.join("events.jsonl"), format!("{original}broken\n")).unwrap();
    assert!(matches!(read_events(&dir), Err(StoreError::Corrupt(s)) if s.starts_with("line 2:")));
}

#[test]
fn read_events_rejects_unterminated_last_line() {
    let (_work, dir) = invocation();
    let original = fs::read_to_string(dir.join("events.jsonl")).unwrap();
    fs::write(dir.join("events.jsonl"), original.trim_end()).unwrap();
    assert!(matches!(read_events(&dir), Err(StoreError::Corrupt(_))));
}

#[test]
fn read_regular_refuses_symlink_hardlink_and_oversized_file() {
    let work = TempDir::new().unwrap();
    fs::write(work.path().join("target"), b"data").unwrap();
    symlink("target", work.path().join("symlink")).unwrap();
    let linked = read_regular(work.path(), "symlink", 4);
    assert!(matches!(linked, Err(StoreError::UnsafePath(_))));
    let oversized = read_regular(work.path(), "target", 3);
    assert!(matches!(oversized, Err(StoreError::TooLarge(_))));
    fs::hard_link(work.path().join("target"), work.path().join("hardlink")).unwrap();
    let linked = read_regular(work.path(), "hardlink", 4);
    assert!(matches!(linked, Err(StoreError::UnsafePath(_))));
}

#[test]
fn write_cancel_keeps_first() {
    let (_work, dir) = invocation();
    let first = CancelRequest {
        reason: "first".into(),
        requested_at: Utc::now(),
        by: "daemon".into(),
    };
    write_cancel(&dir, &first).unwrap();
    let second = CancelRequest {
        reason: "second".into(),
        ..first.clone()
    };
    write_cancel(&dir, &second).unwrap();
    assert_eq!(read_cancel(&dir).unwrap(), Some(first));
}

#[test]
fn write_message_stored_already_stored_conflict() {
    let (_work, dir) = invocation();
    let original = message("msg");
    assert_eq!(
        write_message(&dir, &original).unwrap(),
        MessageWrite::Stored
    );
    assert_eq!(
        write_message(&dir, &original).unwrap(),
        MessageWrite::AlreadyStored
    );
    let mut conflicting = original.clone();
    conflicting.text = "different".into();
    conflicting.sha256 = sha256_hex(conflicting.text.as_bytes());
    assert_eq!(
        write_message(&dir, &conflicting).unwrap(),
        MessageWrite::Conflict
    );
    assert_eq!(read_messages(&dir).unwrap(), [original]);
}

#[test]
fn read_messages_sorts_by_accepted_at_then_id() {
    let (_work, dir) = invocation();
    let first = message("a");
    let second = ControlMessage {
        message_id: "b".into(),
        ..first.clone()
    };
    let third = ControlMessage {
        message_id: "c".into(),
        accepted_at: first.accepted_at + chrono::Duration::seconds(1),
        ..first.clone()
    };
    for msg in [&third, &second, &first] {
        write_message(&dir, msg).unwrap();
    }
    for name in [".msg-temp.json.tmp", "msg-!.json", "unrelated"] {
        fs::write(dir.join("control").join(name), b"invalid").unwrap();
    }
    assert_eq!(read_messages(&dir).unwrap(), [first, second, third]);
}

#[test]
fn write_report_then_read_report_truncates_on_char_boundary() {
    let (_work, dir) = invocation();
    assert_eq!(read_report(&dir, 10).unwrap(), None);
    let info = write_report(&dir, "a€b").unwrap();
    assert_eq!(info.bytes, 5);
    assert_eq!(info.sha256, sha256_hex("a€b".as_bytes()));
    assert!(!info.complete);
    assert_eq!(read_report(&dir, 3).unwrap(), Some(("a".into(), true)));
    assert_eq!(read_report(&dir, 4).unwrap(), Some(("a€".into(), true)));
    assert_eq!(read_report(&dir, 5).unwrap(), Some(("a€b".into(), false)));
}

#[test]
fn write_boundary_roundtrip() {
    let (_work, dir) = invocation();
    let identity = BoundaryIdentity {
        supervisor_pid: 1,
        supervisor_start: "start".into(),
        boundary_pid: 2,
        pid_namespace: 3,
        boot_id: "boot".into(),
    };
    assert_eq!(read_boundary(&dir).unwrap(), None);
    write_boundary(&dir, &identity).unwrap();
    assert_eq!(read_boundary(&dir).unwrap(), Some(identity));
}

#[test]
fn event_log_failed_append_poisons_the_handle() {
    let (_work, dir) = invocation();
    let mut log = EventLog::open(&dir).unwrap();
    log.file = File::open(dir.join("events.jsonl")).unwrap();
    assert!(matches!(
        log.append(NativeEvent::Progress),
        Err(StoreError::Io(_))
    ));
    let poisoned = log.append(NativeEvent::Progress);
    assert!(matches!(poisoned,
        Err(StoreError::Corrupt(s)) if s == "event log poisoned by a failed append"));
    assert_eq!(read_events(&dir).unwrap().len(), 1);
}

#[test]
fn event_log_size_limit_does_not_poison_or_consume_sequence() {
    let (_work, dir) = invocation();
    let mut log = EventLog::open(&dir).unwrap();
    let bytes = log.bytes;
    log.bytes = MAX_RECORD_BYTES;
    let oversized = log.append(NativeEvent::Progress);
    assert!(matches!(
        oversized,
        Err(StoreError::TooLarge("events.jsonl"))
    ));
    log.bytes = bytes;
    assert_eq!(log.append(NativeEvent::Progress).unwrap().seq, 2);
}

#[test]
fn write_message_rejects_unsafe_id_and_oversized_text() {
    let (_work, dir) = invocation();
    let unsafe_id = write_message(&dir, &message("../escape"));
    assert!(matches!(unsafe_id, Err(StoreError::UnsafePath(_))));
    let mut oversized = message("large");
    oversized.text = "x".repeat(MAX_MESSAGE_BYTES + 1);
    let result = write_message(&dir, &oversized);
    assert!(matches!(result, Err(StoreError::TooLarge("message"))));
    assert!(read_messages(&dir).unwrap().is_empty());
}

#[test]
fn read_messages_rejects_malformed_json_and_mismatched_id() {
    let (_work, dir) = invocation();
    let path = dir.join("control/msg-expected.json");
    fs::write(&path, b"broken").unwrap();
    assert!(matches!(read_messages(&dir), Err(StoreError::Corrupt(_))));
    fs::write(path, serde_json::to_vec(&message("different")).unwrap()).unwrap();
    assert!(matches!(read_messages(&dir), Err(StoreError::Corrupt(_))));
}
