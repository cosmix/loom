use super::tests::{admit, manifest, unverified};
use super::*;
use tempfile::TempDir;

#[test]
fn disposition_unverified_for_corrupt_events() {
    let work = TempDir::new().unwrap();
    let dir = admit(work.path(), "corrupt", 1, 0, &[]);
    admit(work.path(), "unknown", 2, 1, &[unverified()]);
    fs::write(dir.join("events.jsonl"), b"broken\n").unwrap();
    let NativeDisposition::Unverified(cause) = stage_disposition(work.path(), "stage") else {
        panic!("corruption must block the stage");
    };
    assert!(cause.starts_with(&format!("inv-{:032x}: ", 1)));
    assert!(cause.contains("line 1:"));
}

#[test]
fn disposition_unverified_for_manifest_that_disagrees_with_directory_name() {
    let work = TempDir::new().unwrap();
    let dir = admit(work.path(), "unit", 1, 0, &[]);
    let wrong = manifest("unit", 2, 0);
    fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec(&wrong).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        stage_disposition(work.path(), "stage"),
        NativeDisposition::Unverified(_)
    ));
}

#[test]
fn disposition_attributes_symlinked_stage_to_stage() {
    for target_exists in [false, true] {
        let work = TempDir::new().unwrap();
        let dir = stage_dir(work.path(), "stage");
        fs::create_dir_all(dir.parent().unwrap()).unwrap();
        let target = work.path().join("target");
        if target_exists {
            fs::create_dir(&target).unwrap();
        }
        std::os::unix::fs::symlink(target, dir).unwrap();
        let NativeDisposition::Unverified(cause) = stage_disposition(work.path(), "stage") else {
            panic!("symlinked stage must block the stage");
        };
        assert!(cause.starts_with("stage stage: "), "{cause}");
        assert!(cause.contains("stage is not a real directory"), "{cause}");
    }
}

#[test]
fn disposition_attributes_invalid_stage_id_to_stage() {
    for stage_id in ["../escape", "bad.name"] {
        for directory_exists in [false, true] {
            let work = TempDir::new().unwrap();
            if directory_exists {
                fs::create_dir_all(stage_dir(work.path(), stage_id)).unwrap();
            }
            let NativeDisposition::Unverified(cause) = stage_disposition(work.path(), stage_id)
            else {
                panic!("invalid stage id must block the stage");
            };
            assert!(cause.starts_with(&format!("stage {stage_id}: ")), "{cause}");
        }
    }
}

#[test]
fn disposition_attributes_unexpected_entry_to_stage() {
    for name in ["notes", "inv-invalid"] {
        for directory in [false, true] {
            let work = TempDir::new().unwrap();
            admit(work.path(), "active", 1, 0, &[]);
            let entry = stage_dir(work.path(), "stage").join(name);
            if directory {
                fs::create_dir(entry).unwrap();
            } else {
                fs::write(entry, b"notes").unwrap();
            }
            let NativeDisposition::Unverified(cause) = stage_disposition(work.path(), "stage")
            else {
                panic!("unexpected entry must block the stage");
            };
            assert!(
                cause.starts_with(&format!("stage stage: unexpected entry {name}: ")),
                "{cause}"
            );
        }
    }
}

#[test]
fn disposition_attributes_corrupt_manifest_to_invocation() {
    let work = TempDir::new().unwrap();
    let dir = admit(work.path(), "unit", 1, 0, &[]);
    fs::write(dir.join("manifest.json"), b"broken").unwrap();
    let NativeDisposition::Unverified(cause) = stage_disposition(work.path(), "stage") else {
        panic!("corrupt invocation must block the stage");
    };
    assert!(cause.starts_with(&format!("inv-{:032x}: ", 1)), "{cause}");
}
