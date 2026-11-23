//! The fold-back's only source is the main checkout's own local settings
//! file, read without following a symlink, blocking on a FIFO, or accepting
//! an oversized or malformed file.

use super::sync_tests::{approved_allow, create_state_root};
use crate::fs::permissions::approved::approved_path;
use crate::fs::permissions::fold_back_approvals;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;
use tempfile::TempDir;

const BLANKET_RULES: &str = r#"{"permissions":{"allow":["Bash(*)","Edit(//home/u/.bashrc)"]}}"#;

/// The local settings path under `dir`, with its `.claude` directory created.
fn settings_path(dir: &Path) -> PathBuf {
    let claude = dir.join(".claude");
    fs::create_dir_all(&claude).unwrap();
    claude.join("settings.local.json")
}

#[test]
fn session_writable_files_contribute_nothing() {
    let main_dir = TempDir::new().unwrap();
    let main = main_dir.path();
    let state_root = create_state_root(main);
    let worktree = main.join(".worktrees").join("s1");
    for dir in [main.join("loom"), worktree.clone(), worktree.join("loom")] {
        fs::write(settings_path(&dir), BLANKET_RULES).unwrap();
    }
    fs::write(
        settings_path(main),
        r#"{"permissions":{"allow":["WebFetch(domain:docs.rs)"]}}"#,
    )
    .unwrap();

    fold_back_approvals(main);

    assert_eq!(
        approved_allow(&state_root),
        vec!["WebFetch(domain:docs.rs)"]
    );
}

#[test]
fn a_fifo_main_settings_file_returns_promptly() {
    let main_dir = TempDir::new().unwrap();
    create_state_root(main_dir.path());
    let fifo = settings_path(main_dir.path());
    nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::S_IRWXU).expect("mkfifo");

    let (sender, receiver) = mpsc::channel();
    let main = main_dir.path().to_path_buf();
    std::thread::spawn(move || {
        let _ = sender.send(fold_back_approvals(&main));
    });
    let added = receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("the fold-back blocked on a FIFO at the main settings path");

    assert_eq!(added, 0);
}

#[test]
fn a_symlinked_main_settings_file_is_not_followed() {
    let main_dir = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let state_root = create_state_root(main_dir.path());
    let target = elsewhere.path().join("settings.json");
    fs::write(&target, r#"{"permissions":{"allow":["Bash(cargo:*)"]}}"#).unwrap();
    std::os::unix::fs::symlink(&target, settings_path(main_dir.path())).unwrap();

    assert_eq!(fold_back_approvals(main_dir.path()), 0);
    assert!(!approved_path(&state_root).exists());
}

#[test]
fn an_oversized_or_malformed_main_file_records_nothing() {
    let main_dir = TempDir::new().unwrap();
    let state_root = create_state_root(main_dir.path());
    let path = settings_path(main_dir.path());
    let padding = " ".repeat(1 << 20);
    fs::write(
        &path,
        format!(r#"{{"permissions":{{"allow":["Bash(cargo:*)"]}}}}{padding}"#),
    )
    .unwrap();

    assert_eq!(fold_back_approvals(main_dir.path()), 0);

    fs::write(&path, "{not json").unwrap();

    assert_eq!(fold_back_approvals(main_dir.path()), 0);
    assert!(!approved_path(&state_root).exists());
}
