use super::{append_event, read_payload, SpoolEvent, SpoolReader, SPOOL_EVENTS};
use serde_json::{json, Value};
use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::sync::Barrier;
use tempfile::TempDir;

fn event_line(event: &str) -> String {
    let value = json!({
        "received_at": "2026-10-28T10:00:00.123Z",
        "event": event,
        "payload": {},
    });
    let mut line = serde_json::to_string(&value).expect("serialize event");
    line.push('\n');
    line
}

fn append(path: &Path, bytes: &[u8]) {
    crate::fs::safe_fs::safe_append(
        path.parent().expect("parent"),
        Path::new(path.file_name().expect("file name")),
        bytes,
    )
    .expect("append bytes");
}

#[test]
fn four_concurrent_appends_produce_four_whole_lines() {
    let tmp = TempDir::new().expect("tempdir");
    let barrier = Barrier::new(4);
    std::thread::scope(|scope| {
        for id in 0..4 {
            let barrier = &barrier;
            let dir = tmp.path();
            scope.spawn(move || {
                let payload = json!({"id": id, "message": "line\n".repeat(12_000)});
                barrier.wait();
                append_event(dir, "Notification", &payload).expect("append event");
            });
        }
    });

    let contents = fs::read_to_string(tmp.path().join("events.jsonl")).expect("read spool");
    assert!(contents.ends_with('\n'));
    let events: Vec<SpoolEvent> = contents
        .lines()
        .map(|line| serde_json::from_str(line).expect("whole JSON event"))
        .collect();
    assert_eq!(events.len(), 4);
    let mut ids: Vec<_> = events
        .iter()
        .map(|event| event.payload["id"].clone())
        .collect();
    ids.sort_by_key(|id| id.as_u64());
    assert_eq!(ids, [json!(0), json!(1), json!(2), json!(3)]);
    for event in events {
        assert_eq!(event.event, "Notification");
        assert_eq!(event.payload["message"], "line\n".repeat(12_000));
    }
}

#[test]
fn payload_over_one_mib_is_refused() {
    let mut raw = vec![b' '; 1024 * 1024 + 1];
    raw[..2].copy_from_slice(b"{}");

    assert_eq!(read_payload(raw.as_slice()), None);
}

#[test]
fn payload_at_one_mib_is_accepted() {
    let mut raw = vec![b' '; 1024 * 1024];
    raw[..2].copy_from_slice(b"{}");

    assert_eq!(read_payload(raw.as_slice()), Some(json!({})));
}

#[test]
fn payload_read_stops_after_the_sentinel_byte() {
    let mut input = io::repeat(b' ').take(2 * 1024 * 1024);

    assert_eq!(read_payload(&mut input), None);
    assert_eq!(input.limit(), 1024 * 1024 - 1);
}

#[test]
fn invalid_json_and_read_errors_are_refused() {
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("broken stdin"))
        }
    }

    assert_eq!(read_payload(&b"{"[..]), None);
    assert_eq!(read_payload(Broken), None);
}

#[test]
fn half_written_line_waits_for_its_newline() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("events.jsonl");
    let first = event_line("SessionStart");
    let second = event_line("Stop");
    fs::write(&path, first).expect("write first event");
    append(&path, &second.as_bytes()[..second.len() / 2]);
    let mut reader = SpoolReader::new(&path);

    assert_eq!(reader.read_new()[0].event, "SessionStart");
    assert!(reader.read_new().is_empty());
    append(
        &path,
        &second.as_bytes()[second.len() / 2..second.len() - 1],
    );
    assert!(
        reader.read_new().is_empty(),
        "valid JSON still needs its newline"
    );
    append(&path, b"\n");
    let events = reader.read_new();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event, "Stop");
    assert!(!reader.was_reset());
    assert!(reader.read_new().is_empty());
}

#[test]
fn append_copies_metadata_and_uses_utc_milliseconds() {
    let tmp = TempDir::new().expect("tempdir");
    let payload = json!({"session_id": "session", "transcript_path": "/tmp/transcript.jsonl"});
    append_event(tmp.path(), "SessionStart", &payload).expect("append");

    let raw = fs::read_to_string(tmp.path().join("events.jsonl")).expect("read");
    let entry: Value = serde_json::from_str(&raw).expect("JSON");
    let at = entry["received_at"].as_str().expect("timestamp");
    assert!(at.ends_with('Z'));
    assert_eq!(at.rsplit_once('.').expect("fraction").1.len(), 4);
    assert!(entry.get("request_id").is_none());
    let mut reader = SpoolReader::new(tmp.path().join("events.jsonl"));
    let events = reader.read_new();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].session_id.as_deref(), Some("session"));
    assert_eq!(
        events[0].transcript_path.as_deref(),
        Some(Path::new("/tmp/transcript.jsonl"))
    );
    assert_eq!(events[0].payload, payload);
}

#[test]
fn permission_request_id_is_copied_from_the_payload() {
    let tmp = TempDir::new().expect("tempdir");
    let payload = json!({"request_id": "67b7a747-ad8d-40f8-8a5e-0251d26896fd"});
    append_event(tmp.path(), "PermissionRequest", &payload).expect("append");

    let mut reader = SpoolReader::new(tmp.path().join("events.jsonl"));
    let events = reader.read_new();
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].request_id.as_deref(),
        payload["request_id"].as_str()
    );
}

#[test]
fn nested_strings_are_cut_at_utf8_boundaries_without_mutating_the_payload() {
    let tmp = TempDir::new().expect("tempdir");
    let mut long = "x".repeat(65_535);
    long.push('é');
    let payload = json!({"nested": [{"long": long, "exact": "y".repeat(65_536)}]});
    append_event(tmp.path(), "Notification", &payload).expect("append");

    let mut reader = SpoolReader::new(tmp.path().join("events.jsonl"));
    let events = reader.read_new();
    let mut expected = "x".repeat(65_535);
    expected.push_str("…[truncated]");
    assert_eq!(events[0].payload["nested"][0]["long"], expected);
    assert_eq!(events[0].payload["nested"][0]["exact"], "y".repeat(65_536));
    assert_eq!(
        payload["nested"][0]["long"]
            .as_str()
            .expect("original")
            .len(),
        65_537
    );
}

#[test]
fn shrinking_file_replays_complete_lines_and_resets_only_that_call() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("events.jsonl");
    let first = event_line("SessionStart");
    let mut contents = first.clone();
    contents.push_str(&event_line("Stop"));
    fs::write(&path, contents).expect("write");
    let mut reader = SpoolReader::new(&path);
    assert_eq!(reader.read_new().len(), 2);
    assert!(!reader.was_reset());

    fs::write(&path, &first).expect("truncate");
    assert_eq!(
        reader.read_new().len(),
        1,
        "the reader does not de-duplicate replay"
    );
    assert!(reader.was_reset());
    assert!(reader.read_new().is_empty());
    assert!(!reader.was_reset());
}

#[test]
fn shrink_of_an_unfinished_tail_also_reports_a_reset() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("events.jsonl");
    let first = event_line("SessionStart");
    fs::write(&path, &first).expect("write");
    append(&path, b"unfinished");
    let mut reader = SpoolReader::new(&path);
    assert_eq!(reader.read_new().len(), 1);

    fs::write(&path, &first).expect("truncate tail");
    assert_eq!(reader.read_new().len(), 1);
    assert!(reader.was_reset());
}

#[test]
fn replaced_inode_restarts_even_when_the_length_is_unchanged() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("events.jsonl");
    let line = event_line("Stop");
    fs::write(&path, &line).expect("write");
    let mut reader = SpoolReader::new(&path);
    let original = reader.read_new();
    assert_eq!(original.len(), 1);
    fs::rename(&path, tmp.path().join("old.jsonl")).expect("keep old inode alive");
    fs::write(&path, &line).expect("replace");

    assert_eq!(reader.read_new(), original);
    assert!(reader.was_reset());
    assert!(reader.read_new().is_empty());
    assert!(!reader.was_reset());
}

#[test]
fn malformed_lines_are_skipped_and_duplicate_events_are_preserved() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("events.jsonl");
    let line = event_line("Stop");
    fs::write(&path, ["not JSON\n{}\n", &line, &line].concat()).expect("write");
    let mut reader = SpoolReader::new(path);

    let events = reader.read_new();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0], events[1]);
    assert!(reader.read_new().is_empty());
}

#[test]
fn missing_and_symlinked_spools_are_not_read_or_written() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("events.jsonl");
    let mut reader = SpoolReader::new(&path);
    assert!(reader.read_new().is_empty());
    assert!(!reader.was_reset());
    let target = tmp.path().join("target.jsonl");
    let original = event_line("Stop");
    fs::write(&target, &original).expect("write target");
    std::os::unix::fs::symlink(&target, &path).expect("symlink");

    assert!(reader.read_new().is_empty());
    assert!(append_event(tmp.path(), "Stop", &json!({})).is_err());
    assert_eq!(fs::read_to_string(target).expect("read target"), original);
}

#[test]
fn hook_registrations_cover_all_nine_events() {
    assert_eq!(
        SPOOL_EVENTS.map(|hook| hook.event),
        [
            "SessionStart",
            "UserPromptSubmit",
            "PermissionRequest",
            "Notification",
            "Stop",
            "SubagentStart",
            "SubagentStop",
            "PreCompact",
            "SessionEnd"
        ]
    );
    let permission = SPOOL_EVENTS[2];
    assert_eq!(permission.matcher, Some("*"));
    assert_eq!(permission.timeout_secs, Some(3600));
    for hook in SPOOL_EVENTS
        .into_iter()
        .filter(|hook| hook.event != "PermissionRequest")
    {
        assert_eq!(hook.matcher, None);
        assert_eq!(hook.timeout_secs, None);
    }
}
