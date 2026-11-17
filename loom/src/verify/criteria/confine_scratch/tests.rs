use std::path::Path;
use std::process::Command;

use super::{stable_cargo_home, CargoScratch};

#[test]
fn stable_cargo_home_digest_is_stable() {
    let path = stable_cargo_home(Path::new("/loom-test-tree")).unwrap();
    assert_eq!(
        path.file_name().unwrap(),
        ".loom-cargo-c4173759b16a543fccc72bcd"
    );
}

/// A private scratch's Linked Cargo home lies inside it, never at the per-tree
/// `.loom-cargo-*` path, and is removed when the scratch is dropped.
#[test]
fn private_linked_cargo_home_goes_with_its_scratch() {
    let root = tempfile::tempdir().unwrap();
    let tree = root.path().canonicalize().unwrap().join("tree");
    let cargo = tree.join("cargo-home");
    std::fs::create_dir_all(cargo.join("registry")).unwrap();
    let mut command = Command::new("true");
    command.env_clear().env("CARGO_HOME", &cargo);
    let scratch = CargoScratch::prepare_private(&command, &tree).unwrap();
    let home = scratch
        .linked_cargo_home()
        .expect("overlap forces a Linked home");
    assert_eq!(home, scratch.path().join("cargo"));
    assert_ne!(home, stable_cargo_home(&tree).unwrap());
    scratch.populate().unwrap();
    assert!(home.join("registry").is_symlink());
    let (home, directory) = (home.to_path_buf(), scratch.path().to_path_buf());
    drop(scratch);
    assert!(!home.exists() && !directory.exists());
    assert!(cargo.join("registry").is_dir(), "the host sources survive");
}

/// A scratch dropped while its thread unwinds a panic stays on disk for the
/// sweep: whatever would have proven its users stopped did not run.
#[cfg(target_os = "linux")]
#[test]
fn a_scratch_dropped_by_a_panic_is_kept() {
    let tree = tempfile::tempdir().unwrap();
    let mut command = Command::new("true");
    command.env_clear();
    let scratch = CargoScratch::prepare_private(&command, tree.path()).unwrap();
    let path = scratch.path().to_path_buf();
    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _owned = scratch;
        panic!("supervising the boundary failed");
    }));
    assert!(unwound.is_err());
    assert!(path.is_dir(), "a scratch dropped by a panic was removed");
    std::fs::remove_dir_all(&path).unwrap();
}
