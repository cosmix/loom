use super::*;
use crate::models::stage::Stage;
use crate::verify::contracts::store::{
    write_freeze, FreezeRecord, FrozenFile, FREEZE_RECORD_VERSION,
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
