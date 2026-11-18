use super::*;
use crate::models::stage::Stage;
use crate::verify::contracts::store::{
    faults, refreeze_set, write_freeze, FreezeRecord, FrozenFile, FREEZE_RECORD_VERSION,
};
use std::os::unix::fs::PermissionsExt;

#[test]
fn restores_frozen_file_without_modifying_a_hard_link() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path().join("work");
    let tree = tmp.path().join("tree");
    std::fs::create_dir_all(&work).unwrap();
    std::fs::create_dir_all(&tree).unwrap();
    let content = b"frozen contract";
    let record = FreezeRecord {
        version: FREEZE_RECORD_VERSION,
        stage_id: "stage-a".into(),
        session_id: "session-a".into(),
        frozen_at: chrono::Utc::now(),
        base: "a".repeat(40),
        files: vec![FrozenFile {
            path: "contract.txt".into(),
            sha256: sha256_hex(content),
        }],
        contracts: Vec::new(),
    };
    write_freeze(&work, &record, &[("contract.txt".into(), content.to_vec())]).unwrap();
    let outside = tmp.path().join("outside");
    std::fs::write(&outside, b"other file").unwrap();
    std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o640)).unwrap();
    std::fs::hard_link(&outside, tree.join("contract.txt")).unwrap();
    let site = ContractSite::resolved(work, Stage::default(), tree.clone(), tree.clone());

    restore_at(&site, "stage-a", None).unwrap();

    assert_eq!(std::fs::read(&outside).unwrap(), b"other file");
    assert_eq!(
        std::fs::metadata(&outside).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert_eq!(std::fs::read(tree.join("contract.txt")).unwrap(), content);
}

#[test]
fn restore_uses_committed_generation_and_keeps_loaded_snapshot_readable() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path().join("work");
    let tree = tmp.path().join("tree");
    std::fs::create_dir_all(&work).unwrap();
    std::fs::create_dir_all(&tree).unwrap();
    let old = b"old contract";
    let record = FreezeRecord {
        version: FREEZE_RECORD_VERSION,
        stage_id: "stage-a".into(),
        session_id: "session-a".into(),
        frozen_at: chrono::Utc::now(),
        base: "base".into(),
        files: vec![FrozenFile {
            path: "contract.txt".into(),
            sha256: sha256_hex(old),
        }],
        contracts: Vec::new(),
    };
    write_freeze(&work, &record, &[("contract.txt".into(), old.to_vec())]).unwrap();
    let snapshot = load_freeze(&work, "stage-a").unwrap().unwrap();
    crate::verify::contracts::store::refreeze_set(
        &work,
        "stage-a",
        &[("contract.txt".into(), b"new contract".to_vec())],
        &[],
    )
    .unwrap();
    assert_eq!(
        read_frozen(&work, &snapshot, "contract.txt", &snapshot.files[0].sha256).unwrap(),
        old
    );
    let site = ContractSite::resolved(work, Stage::default(), tree.clone(), tree.clone());
    restore_at(&site, "stage-a", None).unwrap();
    assert_eq!(
        std::fs::read(tree.join("contract.txt")).unwrap(),
        b"new contract"
    );
}

const CRASH_WORK: &str = "LOOM_REFREEZE_CRASH_WORK";
const CRASH_POINT: &str = "LOOM_REFREEZE_CRASH_POINT";

fn contract_set(tag: &str) -> Vec<(String, Vec<u8>)> {
    ["one.txt", "nested/two.txt"]
        .into_iter()
        .map(|path| (path.to_string(), format!("{tag} {path}").into_bytes()))
        .collect()
}

fn record_of(files: &[(String, Vec<u8>)]) -> FreezeRecord {
    FreezeRecord {
        version: FREEZE_RECORD_VERSION,
        stage_id: "stage-a".into(),
        session_id: "session-a".into(),
        frozen_at: chrono::Utc::now(),
        base: "base".into(),
        files: files
            .iter()
            .map(|(path, bytes)| FrozenFile {
                path: path.clone(),
                sha256: sha256_hex(bytes),
            })
            .collect(),
        contracts: Vec::new(),
    }
}

/// Run by [`a_crashed_refreeze_restarts_with_every_committed_file_restorable`]:
/// the process ends inside the re-freeze.
#[test]
fn refreeze_crash_child() {
    let Some(work) = std::env::var_os(CRASH_WORK) else {
        return;
    };
    let point = match std::env::var(CRASH_POINT).unwrap().as_str() {
        "copy" => faults::Point::AfterCopy,
        "rename" => faults::Point::BeforeGenerationRename,
        _ => faults::Point::BeforeRecord,
    };
    faults::arm(point, faults::Action::Exit);
    let new = contract_set("new");
    refreeze_set(std::path::Path::new(&work), "stage-a", &new, &[]).unwrap();
    panic!("the injected crash did not fire");
}

#[test]
fn a_crashed_refreeze_restarts_with_every_committed_file_restorable() {
    let child = concat!(module_path!(), "::refreeze_crash_child");
    let child = child.split_once("::").unwrap().1;
    for point in ["copy", "rename", "record"] {
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path().join("work");
        let tree = tmp.path().join("tree");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::create_dir_all(&tree).unwrap();
        let old = contract_set("old");
        let record = record_of(&old);
        write_freeze(&work, &record, &old).unwrap();

        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", child, "--nocapture"])
            .env(CRASH_WORK, &work)
            .env(CRASH_POINT, point)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(81), "{point}");

        // A new process: the committed record and all its copies still stand.
        assert_eq!(load_freeze(&work, "stage-a").unwrap().unwrap(), record);
        let site =
            ContractSite::resolved(work.clone(), Stage::default(), tree.clone(), tree.clone());
        restore_at(&site, "stage-a", None).unwrap();
        for (path, bytes) in &old {
            assert_eq!(&std::fs::read(tree.join(path)).unwrap(), bytes, "{point}");
        }

        let new = contract_set("new");
        refreeze_set(&work, "stage-a", &new, &[]).unwrap();
        restore_at(&site, "stage-a", None).unwrap();
        for (path, bytes) in &new {
            assert_eq!(&std::fs::read(tree.join(path)).unwrap(), bytes, "{point}");
        }
    }
}
