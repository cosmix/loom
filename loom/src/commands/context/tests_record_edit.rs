//! Compatibility tests for `loom context record-edit`.

use super::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn malformed_stage_is_rejected() {
    assert!(record_edit("", &[]).is_err());
    assert!(record_edit("../escape", &[]).is_err());
    assert!(record_edit("stage-a", &[]).is_ok());
}

#[test]
fn notifications_leave_existing_data_unchanged() {
    let temp = TempDir::new().unwrap();
    let legacy = temp.path().join("dirty-paths.json");
    fs::write(&legacy, "legacy data").unwrap();
    let missing = temp.path().join("missing");
    record_edit("stage-a", &[legacy.clone(), missing.clone()]).unwrap();
    assert_eq!(fs::read_to_string(legacy).unwrap(), "legacy data");
    assert!(!missing.exists());
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
}
