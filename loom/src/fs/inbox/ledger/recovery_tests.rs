use super::*;
use crate::relay::new_request_id;
use std::io::Write;

fn record(id: &str, state: Option<LedgerState>, outcome: Option<LedgerOutcome>) -> LedgerRecord {
    LedgerRecord {
        id: id.into(),
        kind: RequestKind::Memory,
        state,
        outcome,
        reason: None,
        at: Utc::now(),
    }
}

fn path(root: &Path) -> std::path::PathBuf {
    root.join("inbox/session-1/ledger.jsonl")
}

#[test]
fn append_repairs_torn_suffix_and_preserves_applying() {
    for suffix in ["{\"id\":\"partial", "{\"id\":\"partial\n"] {
        let dir = tempfile::tempdir().unwrap();
        let id = new_request_id();
        let applying = record(&id, Some(LedgerState::Applying), None);
        append_ledger(dir.path(), "session-1", &applying).unwrap();
        std::fs::OpenOptions::new()
            .append(true)
            .open(path(dir.path()))
            .unwrap()
            .write_all(suffix.as_bytes())
            .unwrap();
        let unknown = record(&id, None, Some(LedgerOutcome::UnknownAfterRestart));
        append_ledger(dir.path(), "session-1", &unknown).unwrap();
        let next = record(&new_request_id(), None, Some(LedgerOutcome::Applied));
        append_ledger(dir.path(), "session-1", &next).unwrap();
        assert_eq!(
            read_ledger(dir.path(), "session-1").unwrap(),
            vec![applying, unknown, next]
        );
        assert!(is_recorded(dir.path(), "session-1", &id).unwrap());
    }
}

#[test]
fn append_rejects_interior_corruption_without_modifying_ledger() {
    let dir = tempfile::tempdir().unwrap();
    let entry = record(&new_request_id(), Some(LedgerState::Applying), None);
    append_ledger(dir.path(), "session-1", &entry).unwrap();
    let ledger = path(dir.path());
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&ledger)
        .unwrap();
    write!(
        file,
        "{{broken\n{}\n",
        serde_json::to_string(&entry).unwrap()
    )
    .unwrap();
    let before = std::fs::read(&ledger).unwrap();
    assert!(append_ledger(dir.path(), "session-1", &entry).is_err());
    assert!(read_ledger(dir.path(), "session-1").is_err());
    assert_eq!(std::fs::read(&ledger).unwrap(), before);
}

#[test]
fn retains_history_past_old_total_file_limit() {
    let dir = tempfile::tempdir().unwrap();
    let id = new_request_id();
    let entry = record(&id, Some(LedgerState::Applying), None);
    append_ledger(dir.path(), "session-1", &entry).unwrap();
    let line = format!("{}\n", serde_json::to_string(&entry).unwrap());
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(path(dir.path()))
        .unwrap();
    for _ in 0..(crate::fs::safe_fs::MAX_LOG_BYTES / line.len() + 1) {
        file.write_all(line.as_bytes()).unwrap();
    }
    drop(file);
    let next = record(&new_request_id(), None, Some(LedgerOutcome::Applied));
    append_ledger(dir.path(), "session-1", &next).unwrap();
    let records = read_ledger(dir.path(), "session-1").unwrap();
    assert_eq!(records.first(), Some(&entry));
    assert_eq!(records.last(), Some(&next));
    assert!(is_recorded(dir.path(), "session-1", &id).unwrap());
}

#[test]
fn valid_unterminated_record_is_preserved_before_append() {
    let dir = tempfile::tempdir().unwrap();
    let first = record(&new_request_id(), Some(LedgerState::Applying), None);
    append_ledger(dir.path(), "session-1", &first).unwrap();
    let ledger = path(dir.path());
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&ledger)
        .unwrap();
    file.set_len(file.metadata().unwrap().len() - 1).unwrap();
    let next = record(&new_request_id(), None, Some(LedgerOutcome::Applied));
    append_ledger(dir.path(), "session-1", &next).unwrap();
    assert_eq!(
        read_ledger(dir.path(), "session-1").unwrap(),
        vec![first, next]
    );
}

#[test]
fn torn_applying_before_handler_can_be_retried() {
    let dir = tempfile::tempdir().unwrap();
    let first = record(&new_request_id(), Some(LedgerState::Applying), None);
    append_ledger(dir.path(), "session-1", &first).unwrap();
    let retry = record(&new_request_id(), Some(LedgerState::Applying), None);
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(path(dir.path()))
        .unwrap();
    file.write_all(b"{\"id\":\"incomplete").unwrap();
    drop(file);
    append_ledger(dir.path(), "session-1", &retry).unwrap();
    assert_eq!(
        read_ledger(dir.path(), "session-1").unwrap(),
        vec![first, retry]
    );
}
