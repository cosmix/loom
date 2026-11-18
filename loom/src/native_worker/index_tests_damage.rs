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
    let blocker = stage_disposition(work.path(), "stage")
        .blocker("stage")
        .unwrap();
    assert!(
        blocker.contains("damaged native Codex worker record"),
        "{blocker}"
    );
    assert!(!blocker.contains("loom subagents release"), "{blocker}");
}

#[test]
fn invocation_state_ignores_corrupt_siblings() {
    let work = TempDir::new().unwrap();
    let corrupt = admit(work.path(), "corrupt", 1, 0, &[]);
    admit(work.path(), "unknown", 2, 1, &[unverified()]);
    fs::write(corrupt.join("events.jsonl"), b"broken\n").unwrap();
    let good = format!("inv-{:032x}", 2);
    let state = invocation_state(work.path(), "stage", &good)
        .unwrap()
        .unwrap();
    assert_eq!(state.ownership, Ownership::Unverified);
    assert!(invocation_state(work.path(), "stage", &format!("inv-{:032x}", 1)).is_err());
    assert!(
        invocation_state(work.path(), "stage", &format!("inv-{:032x}", 3))
            .unwrap()
            .is_none()
    );
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

#[test]
fn disposition_none_for_loom_stage_ids_without_records() {
    for stage_id in ["s".repeat(70), "_x".to_string(), "-x".to_string()] {
        let work = TempDir::new().unwrap();
        assert_eq!(
            stage_disposition(work.path(), &stage_id),
            NativeDisposition::None,
            "{stage_id}"
        );
    }
}
