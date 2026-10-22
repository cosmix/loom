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
