//! Contract `a-changed-frozen-file-is-drift`: `frozen_drift` compares every
//! frozen file's worktree bytes with the sha256 the freeze recorded.
use crate::utils::sha256_hex;
use crate::verify::contracts::drift::{frozen_drift, DriftKind};
use crate::verify::contracts::store::{FreezeRecord, FrozenFile, FREEZE_RECORD_VERSION};
use chrono::Utc;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

const CHANGED: &str = "src/changed_contract_tests.rs";
const DELETED: &str = "src/deleted_contract_tests.rs";
const ORIGINAL: &str = "fn frozen() { assert_eq!(1, 1); }\n";
// Same length as ORIGINAL: a size comparison cannot see the change.
const EDITED: &str = "fn frozen() { assert_eq!(1, 2); }\n";

fn write(dir: &Path, path: &str, content: &str) {
    let full = dir.join(path);
    fs::create_dir_all(full.parent().expect("parent")).expect("create dirs");
    fs::write(full, content).expect("write file");
}

fn record(paths: &[&str]) -> FreezeRecord {
    FreezeRecord {
        version: FREEZE_RECORD_VERSION,
        stage_id: "s1".to_string(),
        session_id: "session-1".to_string(),
        frozen_at: Utc::now(),
        base: "0".repeat(40),
        files: paths
            .iter()
            .map(|path| FrozenFile {
                path: (*path).to_string(),
                sha256: sha256_hex(ORIGINAL.as_bytes()),
            })
            .collect(),
        contracts: Vec::new(),
    }
}

#[test]
fn a_changed_frozen_file_is_drift() {
    let worktree = TempDir::new().expect("tempdir");
    // The paths are relative to the acceptance directory, a subdirectory here.
    let acceptance = worktree.path().join("loom");
    write(&acceptance, CHANGED, ORIGINAL);
    write(&acceptance, DELETED, ORIGINAL);
    let record = record(&[CHANGED, DELETED]);

    let clean = frozen_drift(&record, &acceptance, worktree.path()).expect("clean tree");
    assert!(clean.is_empty(), "unchanged frozen files are not drift");

    write(&acceptance, CHANGED, EDITED);
    fs::remove_file(acceptance.join(DELETED)).expect("delete frozen file");

    let drift = frozen_drift(&record, &acceptance, worktree.path()).expect("drift");
    assert_eq!(drift.len(), 2, "one drift per changed or missing file");
    let kind_of = |path: &str| {
        drift
            .iter()
            .find(|entry| entry.path == path)
            .map(|entry| &entry.kind)
    };
    assert!(
        matches!(kind_of(CHANGED), Some(DriftKind::Changed)),
        "a file whose bytes differ from the recorded sha256 is Changed"
    );
    assert!(
        matches!(kind_of(DELETED), Some(DriftKind::Missing)),
        "a deleted frozen file is Missing"
    );
}
