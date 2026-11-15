//! Tests for the `reference-transaction` hook installer.

use super::reference_transaction::{write_script, SCRIPT};
use super::{
    install_reference_transaction_hook, is_reference_transaction_hook_installed, HookInstall,
};
use crate::git::target_guard::HOOK_MARKER;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// A directory with an empty `.git` directory, and the hook's path in it.
fn repo() -> (TempDir, PathBuf) {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join(".git")).unwrap();
    let hook = temp.path().join(".git/hooks/reference-transaction");
    (temp, hook)
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn script_carries_the_hook_marker() {
    assert!(SCRIPT.contains(HOOK_MARKER));
}

#[test]
fn installs_the_hook_when_absent() {
    let (temp, hook) = repo();

    let installed = install_reference_transaction_hook(temp.path()).unwrap();

    assert_eq!(installed, HookInstall::Installed);
    assert_eq!(fs::read_to_string(&hook).unwrap(), SCRIPT);
    assert_eq!(mode(&hook), 0o755);
    assert!(is_reference_transaction_hook_installed(temp.path()));
}

#[test]
fn second_install_is_up_to_date() {
    let (temp, _hook) = repo();
    install_reference_transaction_hook(temp.path()).unwrap();

    let again = install_reference_transaction_hook(temp.path()).unwrap();

    assert_eq!(again, HookInstall::UpToDate);
}

#[test]
fn overwrites_an_older_loom_hook() {
    let (temp, hook) = repo();
    fs::create_dir_all(hook.parent().unwrap()).unwrap();
    fs::write(&hook, format!("#!/bin/sh\n# {HOOK_MARKER}\nexit 0\n")).unwrap();

    let installed = install_reference_transaction_hook(temp.path()).unwrap();

    assert_eq!(installed, HookInstall::Installed);
    assert_eq!(fs::read_to_string(&hook).unwrap(), SCRIPT);
    assert_eq!(mode(&hook), 0o755);
}

#[test]
fn restores_the_mode_of_a_current_hook() {
    let (temp, hook) = repo();
    install_reference_transaction_hook(temp.path()).unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o644)).unwrap();

    let installed = install_reference_transaction_hook(temp.path()).unwrap();

    assert_eq!(installed, HookInstall::Installed);
    assert_eq!(mode(&hook), 0o755);
}

#[test]
fn foreign_hook_is_left_untouched() {
    let (temp, hook) = repo();
    fs::create_dir_all(hook.parent().unwrap()).unwrap();
    let foreign = b"#!/bin/sh\n# another tool's hook\n\xff\xfe\nexit 0\n";
    fs::write(&hook, foreign).unwrap();

    let installed = install_reference_transaction_hook(temp.path()).unwrap();

    assert_eq!(installed, HookInstall::ForeignHookPresent);
    assert_eq!(fs::read(&hook).unwrap(), foreign);
    assert!(!is_reference_transaction_hook_installed(temp.path()));
}

#[test]
fn failed_publish_leaves_no_staged_file() {
    let (_temp, hook) = repo();
    // A directory at the hook's path makes the rename fail. The installer reads
    // that path first and stops on a directory, so call `write_script` directly.
    let occupant = hook.join("occupant");
    fs::create_dir_all(&occupant).unwrap();

    let error = write_script(&hook).unwrap_err();

    let message = format!("{error:#}");
    assert!(message.contains("Failed to install hook"), "{message}");
    let entries: Vec<_> = fs::read_dir(hook.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(entries, [hook.file_name().unwrap()], "{entries:?}");
    assert!(occupant.is_dir());
}

#[test]
fn refuses_without_a_git_directory() {
    let temp = TempDir::new().unwrap();

    let result = install_reference_transaction_hook(temp.path());

    assert!(result.is_err());
    assert!(!temp.path().join(".git").exists());
    assert!(!is_reference_transaction_hook_installed(temp.path()));
}

#[test]
fn a_symlink_at_the_old_staging_name_is_not_written_through() {
    let (temp, hook) = repo();
    fs::create_dir_all(hook.parent().unwrap()).unwrap();
    let outside = temp.path().join("outside");
    fs::write(&outside, "untouched").unwrap();
    std::os::unix::fs::symlink(&outside, hook.with_extension("loom-new")).unwrap();

    write_script(&hook).unwrap();

    assert_eq!(fs::read_to_string(&outside).unwrap(), "untouched");
    assert_eq!(fs::read(&hook).unwrap(), SCRIPT.as_bytes());
    assert_eq!(mode(&hook), 0o755);
}
