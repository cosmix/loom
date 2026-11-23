//! The operator's `--resolved` while a merge quarantine holds operator files.

use std::path::{Path, PathBuf};

use serial_test::serial;
use tempfile::TempDir;

use super::merge_resolved;
use crate::git::MergeBlock;
use crate::models::stage::{Stage, StageStatus};
use crate::verify::transitions::{load_stage, save_stage};

const STAGE: &str = "s1";

/// A state directory under `project` holding the stage `MergeBlocked` on an
/// incomplete restore into `quarantine`; returns the state directory.
fn blocked_on(project: &Path, quarantine: PathBuf) -> PathBuf {
    let work_dir = project.join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    std::fs::write(work_dir.join("config.toml"), "").unwrap();
    let mut stage = Stage {
        id: STAGE.to_string(),
        name: STAGE.to_string(),
        status: StageStatus::MergeBlocked,
        ..Stage::default()
    };
    stage.merge.block = Some(MergeBlock::CheckoutRestoreIncomplete {
        quarantine,
        failures: vec![],
        detail: None,
    });
    save_stage(&stage, &work_dir).unwrap();
    work_dir
}

/// `merge_resolved` from `project`, as the error's text.
fn resolved_error(project: &Path) -> String {
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(project).unwrap();
    let result = merge_resolved(Some(STAGE.to_string()));
    std::env::set_current_dir(original).unwrap();
    format!("{:#}", result.unwrap_err())
}

#[test]
#[serial]
fn resolved_refuses_while_the_quarantine_exists_and_keeps_the_block() {
    let temp = TempDir::new().unwrap();
    let quarantine = temp.path().join("loom-displaced-s1-123");
    std::fs::create_dir(&quarantine).unwrap();
    let work_dir = blocked_on(temp.path(), quarantine.clone());

    let error = resolved_error(temp.path());

    assert!(error.contains("cannot merge while"), "{error}");
    assert!(error.contains(&quarantine.display().to_string()), "{error}");
    let saved = load_stage(STAGE, &work_dir).unwrap();
    assert_eq!(saved.status, StageStatus::MergeBlocked);
    assert!(saved.merge.block.is_some_and(|b| b.holds_displaced_files()));
}

#[test]
#[serial]
fn resolved_goes_past_the_check_once_the_quarantine_is_deleted() {
    let temp = TempDir::new().unwrap();
    blocked_on(temp.path(), temp.path().join("deleted-quarantine"));

    let error = resolved_error(temp.path());

    assert!(!error.contains("cannot merge while"), "{error}");
}
