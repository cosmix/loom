use super::*;
use std::io::Write;
use tempfile::TempDir;

fn event(id: &str, to: StageStatus) -> StageEvent {
    StageEvent::new(id, StageStatus::Queued, to, None, None)
}

#[test]
fn missing_log_reads_empty() {
    let dir = TempDir::new().unwrap();
    assert!(read_recent(dir.path(), 10).is_empty());
}

#[test]
fn older_events_without_session_type_still_deserialize() {
    let original = event("legacy", StageStatus::Executing);
    let json = serde_json::to_value(&original).unwrap();
    assert!(json.get("session_type").is_none());
    let restored: StageEvent = serde_json::from_value(json).unwrap();
    assert_eq!(restored, original);
}

#[test]
fn returns_last_n_and_skips_garbage_line() {
    let dir = TempDir::new().unwrap();
    for i in 0..5 {
        append_event(dir.path(), &event(&format!("s{i}"), StageStatus::Executing)).unwrap();
        if i == 2 {
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(dir.path().join(EVENTS_FILE))
                .unwrap();
            writeln!(file, "{{not json").unwrap();
        }
    }
    let ids: Vec<_> = read_recent(dir.path(), 3)
        .into_iter()
        .map(|e| e.stage_id)
        .collect();
    assert_eq!(ids, ["s2", "s3", "s4"]);
}

#[test]
fn wire_form_is_kebab_case_and_reason_only_for_blocked() {
    let blocked = StageEvent::new(
        "a",
        StageStatus::Executing,
        StageStatus::Blocked,
        Some("sess".into()),
        Some("boom"),
    );
    assert_eq!(blocked.reason.as_deref(), Some("boom"));
    let json = serde_json::to_value(&blocked).unwrap();
    assert_eq!(json["from"], "executing");
    let other = StageEvent::new(
        "a",
        StageStatus::Executing,
        StageStatus::NeedsHandoff,
        None,
        Some("stale"),
    );
    assert_eq!(other.reason, None);
    assert_eq!(serde_json::to_value(&other).unwrap()["to"], "needs-handoff");
}

#[test]
fn symlinked_work_dir_resolves_to_its_target() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("shared");
    std::fs::create_dir(&target).unwrap();
    let link = dir.path().join("work");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    append_event(&link, &event("s1", StageStatus::Executing)).unwrap();

    assert!(target.join(EVENTS_FILE).is_file());
    let ids: Vec<_> = read_recent(&link, 10)
        .into_iter()
        .map(|e| e.stage_id)
        .collect();
    assert_eq!(ids, ["s1"]);
}

#[test]
fn symlink_planted_at_the_events_file_is_refused() {
    let dir = TempDir::new().unwrap();
    let work = dir.path().join("work");
    std::fs::create_dir(&work).unwrap();
    let outside = dir.path().join("outside.jsonl");
    std::fs::write(&outside, "").unwrap();
    std::os::unix::fs::symlink(&outside, work.join(EVENTS_FILE)).unwrap();

    assert!(append_event(&work, &event("s1", StageStatus::Executing)).is_err());
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "");
    assert!(read_recent(&work, 10).is_empty());
}
