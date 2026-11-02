//! Tests for `install_binary`'s atomic staging write, executable mode, and
//! atomic replacement. A replacement seam makes failures deterministic.

use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use tempfile::TempDir;

use crate::commands::self_update::install::{install_binary, install_with_replace};

#[test]
fn failed_replacement_preserves_the_original_at_its_public_path() {
    let temp_dir = TempDir::new().unwrap();
    let current_exe = temp_dir.path().join("loom");
    fs::write(&current_exe, b"old binary").unwrap();
    let result = install_with_replace(b"new binary", &current_exe, |staging, target| {
        assert_eq!(fs::read(target).unwrap(), b"old binary");
        assert_eq!(fs::read(staging).unwrap(), b"new binary");
        Err(std::io::Error::other("injected replacement failure"))
    });
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("replace current binary"));
    assert_eq!(fs::read(&current_exe).unwrap(), b"old binary");
    assert_eq!(fs::read_dir(temp_dir.path()).unwrap().count(), 1);
}

#[test]
fn replacement_keeps_the_original_visible_until_the_atomic_rename() {
    let temp_dir = TempDir::new().unwrap();
    let current_exe = temp_dir.path().join("loom");
    fs::write(&current_exe, b"old binary").unwrap();
    install_with_replace(b"new binary", &current_exe, |staging, target| {
        assert_eq!(fs::read(target).unwrap(), b"old binary");
        fs::rename(staging, target)
    })
    .unwrap();
    assert_eq!(fs::read(&current_exe).unwrap(), b"new binary");
}

#[test]
fn install_binary_replaces_the_target_and_leaves_no_temp_files() {
    let temp_dir = TempDir::new().unwrap();
    let current_exe = temp_dir.path().join("loom");
    fs::write(&current_exe, b"old binary").unwrap();

    install_binary(b"new binary content", &current_exe).unwrap();

    assert_eq!(fs::read(&current_exe).unwrap(), b"new binary content");
    let entries: Vec<_> = fs::read_dir(temp_dir.path()).unwrap().collect();
    assert_eq!(
        entries.len(),
        1,
        "the staging file must not survive a successful install"
    );
}

#[cfg(unix)]
#[test]
fn install_binary_sets_the_executable_mode() {
    let temp_dir = TempDir::new().unwrap();
    let current_exe = temp_dir.path().join("loom");
    fs::write(&current_exe, b"old binary").unwrap();
    fs::set_permissions(&current_exe, fs::Permissions::from_mode(0o644)).unwrap();

    install_binary(b"new binary content", &current_exe).unwrap();

    let mode = fs::metadata(&current_exe).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o755);
}
