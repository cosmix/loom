use super::archive_run_state;
use std::fs;
use tempfile::TempDir;

#[test]
fn archive_copies_journals_and_telemetry_and_survives_state_removal() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join(".loom/work");
    fs::create_dir_all(work_dir.join("memory")).unwrap();
    fs::create_dir_all(work_dir.join("telemetry")).unwrap();
    fs::write(work_dir.join("memory/stage.md"), "journal").unwrap();
    fs::write(work_dir.join("telemetry/events.jsonl"), "event").unwrap();

    let archive = archive_run_state(&work_dir, temp.path(), Some("plan-a"))
        .unwrap()
        .unwrap();
    fs::remove_dir_all(&work_dir).unwrap();

    assert_eq!(
        (
            fs::read_to_string(archive.join("memory/stage.md")).unwrap(),
            fs::read_to_string(archive.join("telemetry/events.jsonl")).unwrap(),
            work_dir.exists(),
        ),
        ("journal".to_string(), "event".to_string(), false)
    );
}

#[test]
fn archive_is_a_no_op_without_journals() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join(".loom/work");
    fs::create_dir_all(&work_dir).unwrap();

    let archive = archive_run_state(&work_dir, temp.path(), Some("empty")).unwrap();

    assert!(archive.is_none());
    assert!(!temp.path().join(".loom/memory/archive").exists());
}

#[test]
fn archive_failure_without_journals_only_warns() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join(".loom/work");
    fs::create_dir_all(work_dir.join("telemetry")).unwrap();
    fs::write(work_dir.join("telemetry/events.jsonl"), "event").unwrap();
    fs::create_dir_all(temp.path().join(".loom/memory")).unwrap();
    fs::write(temp.path().join(".loom/memory/archive"), "blocker").unwrap();

    let archive = archive_run_state(&work_dir, temp.path(), Some("plan-a")).unwrap();

    assert!(archive.is_none());
}

#[test]
fn archive_failure_with_unlistable_memory_returns_error() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join(".loom/work");
    fs::create_dir_all(&work_dir).unwrap();
    fs::write(work_dir.join("memory"), "not a directory").unwrap();
    fs::create_dir_all(temp.path().join(".loom/memory")).unwrap();
    fs::write(temp.path().join(".loom/memory/archive"), "blocker").unwrap();

    let result = archive_run_state(&work_dir, temp.path(), Some("plan-a"));

    assert!(result.is_err());
}
