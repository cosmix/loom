//! Tests for `verify_installed_version`, the check that catches a swapped-in
//! binary reporting the wrong version after a `loom update` binary swap.

#![cfg(unix)]

use tempfile::TempDir;

use crate::commands::self_update::verify_installed_version;

use super::{retry_past_etxtbsy, write_stub};

#[test]
fn verify_installed_version_accepts_a_matching_report() {
    let temp_dir = TempDir::new().unwrap();
    let stub = write_stub(&temp_dir, "echo 'loom 1.2.3'");

    retry_past_etxtbsy(|| verify_installed_version(&stub, "v1.2.3")).unwrap();
}

#[test]
fn verify_installed_version_rejects_a_mismatched_version() {
    let temp_dir = TempDir::new().unwrap();
    let stub = write_stub(&temp_dir, "echo 'loom 9.9.9'");

    let error = retry_past_etxtbsy(|| verify_installed_version(&stub, "v1.2.3"))
        .unwrap_err()
        .to_string();

    assert!(
        error.contains("did not report the installed release version"),
        "{error}"
    );
}

#[test]
fn verify_installed_version_rejects_a_nonzero_exit() {
    let temp_dir = TempDir::new().unwrap();
    let stub = write_stub(&temp_dir, "exit 3");

    let error = retry_past_etxtbsy(|| verify_installed_version(&stub, "v1.2.3"))
        .unwrap_err()
        .to_string();

    assert!(error.contains("exited unsuccessfully"), "{error}");
}
