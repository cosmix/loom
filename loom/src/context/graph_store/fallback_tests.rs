use super::*;
use crate::context::graph_store::FileEntry;
use crate::context::source_graph::FileCoverage;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

fn layer_with(path: &str, content_hash: &str) -> GraphLayer {
    let mut layer = GraphLayer {
        revision: "rev1".to_string(),
        ..GraphLayer::default()
    };
    layer.files.insert(
        path.to_string(),
        FileEntry {
            content_hash: content_hash.to_string(),
            coverage: FileCoverage::Full,
            ..FileEntry::default()
        },
    );
    layer
}

/// `chmod 0o555` on `dir`, then prove the mode bit is actually enforced by
/// probing a real write - root and some sandboxes ignore directory
/// permission bits entirely, in which case a test built on this helper has
/// nothing to exercise and must skip itself (mirrors
/// `context::refresh::tests_snapshot::a_snapshot_that_cannot_persist_still_serves_results_from_memory`).
/// Restores `0o755` before returning so a skip never leaves the caller's
/// `TempDir` unremovable.
fn lock_down(dir: &Path) -> bool {
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let probe = dir.join(".write-probe");
    let enforced = std::fs::write(&probe, b"x").is_err();
    if !enforced {
        let _ = std::fs::remove_file(&probe);
    }
    enforced
}

fn unlock(dir: &Path) {
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn publish_base_falls_back_to_memory_when_the_base_dir_is_read_only() {
    let temp = TempDir::new().unwrap();
    let store = GraphStore::new(&temp.path().join("cache"), &temp.path().join("work"));
    let base_dir = store.base_dir();
    std::fs::create_dir_all(&base_dir).unwrap();

    if !lock_down(&base_dir) {
        eprintln!(
            "SKIP publish_base_falls_back_to_memory_when_the_base_dir_is_read_only: this \
             environment does not enforce 0o555 directory permissions"
        );
        return;
    }

    let layer = layer_with("src/a.rs", "hash-a");
    let published = store.publish_base("rev1", &layer);
    unlock(&base_dir);

    assert!(
        published.unwrap(),
        "a denied write must still report the layer as published for this process"
    );
    assert!(
        !store.base_path("rev1").exists(),
        "nothing may land on the read-only disk"
    );
    let loaded = store
        .load_base("rev1")
        .unwrap()
        .expect("load_base must answer from the in-memory fallback");
    assert_eq!(loaded, layer);

    let resolved = store.resolved("rev1", None).unwrap();
    assert_eq!(resolved.files["src/a.rs"].content_hash, "hash-a");
}

#[test]
fn save_overlay_falls_back_to_memory_when_the_overlay_dir_is_read_only() {
    let temp = TempDir::new().unwrap();
    let store = GraphStore::new(&temp.path().join("cache"), &temp.path().join("work"));
    let overlay_dir = store.overlay_dir("plan", "stage");
    std::fs::create_dir_all(&overlay_dir).unwrap();

    if !lock_down(&overlay_dir) {
        eprintln!(
            "SKIP save_overlay_falls_back_to_memory_when_the_overlay_dir_is_read_only: this \
             environment does not enforce 0o555 directory permissions"
        );
        return;
    }

    let overlay = layer_with("src/b.rs", "hash-b");
    let saved = store.save_overlay("plan", "stage", &overlay);
    unlock(&overlay_dir);

    saved.expect("a denied overlay write must not fail the caller");
    assert!(
        !store.overlay_path("plan", "stage").exists(),
        "nothing may land on the read-only disk"
    );
    let loaded = store
        .load_overlay("plan", "stage")
        .unwrap()
        .expect("load_overlay must answer from the in-memory fallback");
    assert_eq!(loaded.files["src/b.rs"].content_hash, "hash-b");

    let resolved = store.resolved("rev1", Some(("plan", "stage"))).unwrap();
    assert_eq!(resolved.files["src/b.rs"].content_hash, "hash-b");
}

#[test]
fn is_write_denied_matches_permission_and_read_only_errors() {
    let permission = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
    let erofs = std::io::Error::from_raw_os_error(libc::EROFS);
    let not_found = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");

    assert!(is_write_denied(
        &anyhow::Error::new(permission).context("writing")
    ));
    assert!(is_write_denied(
        &anyhow::Error::new(erofs).context("writing")
    ));
    assert!(!is_write_denied(
        &anyhow::Error::new(not_found).context("writing")
    ));
    assert!(!is_write_denied(&anyhow::anyhow!("not an io error at all")));
}
