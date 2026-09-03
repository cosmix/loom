//! Tests for `install_binary`'s atomic staging write, executable mode, and
//! backup rename.
//!
//! The rollback-on-failure path (`install.rs:43-58`) fires only when the
//! final `staging -> current_exe` rename fails after the backup rename
//! already succeeded. `staging`, `backup`, and `current_exe` are all created
//! via `NamedTempFile::new_in(parent)` against the exact same `parent`
//! directory, so any filesystem permission or attribute change that would
//! block the second rename blocks the first one identically - there is no
//! directory- or file-level state reachable from a test that fails one
//! same-directory rename but not the other, short of a fault-injection seam
//! inside `install_binary` itself. No test for that path is included here
//! for that reason; see the stage report for the same conclusion.

use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use tempfile::TempDir;

use crate::commands::self_update::install::install_binary;

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
        "the staging and backup temp files must not survive a successful install"
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
