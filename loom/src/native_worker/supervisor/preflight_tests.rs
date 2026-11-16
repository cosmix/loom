use std::fs;
use std::os::unix::fs::PermissionsExt;

use super::{prepare_claude_dir, prepare_codex_home};

#[test]
fn prepare_codex_home_creates_a_private_sessions_dir() {
    let home = tempfile::TempDir::new().expect("temp");
    fs::write(home.path().join("auth.json"), "{}").expect("auth");
    prepare_codex_home(home.path()).expect("prepare");
    let sessions = fs::symlink_metadata(home.path().join("sessions")).expect("sessions");
    assert!(sessions.is_dir());
    assert_eq!(sessions.permissions().mode() & 0o777, 0o700);
}

#[test]
fn prepare_codex_home_refuses_a_symlinked_sessions_dir() {
    let home = tempfile::TempDir::new().expect("temp");
    let other = tempfile::TempDir::new().expect("temp");
    fs::write(home.path().join("auth.json"), "{}").expect("auth");
    std::os::unix::fs::symlink(other.path(), home.path().join("sessions")).expect("symlink");
    assert!(prepare_codex_home(home.path()).is_err());
    fs::remove_file(home.path().join("sessions")).expect("unlink");
    fs::write(home.path().join("sessions"), "").expect("file");
    assert!(prepare_codex_home(home.path()).is_err());
}

#[test]
fn prepare_claude_dir_creates_it_and_refuses_a_symlink() {
    let root = tempfile::TempDir::new().expect("temp");
    prepare_claude_dir(root.path()).expect("create");
    assert!(root.path().join(".claude").is_dir());
    let other = tempfile::TempDir::new().expect("temp");
    let linked = tempfile::TempDir::new().expect("temp");
    std::os::unix::fs::symlink(other.path(), linked.path().join(".claude")).expect("symlink");
    assert!(prepare_claude_dir(linked.path()).is_err());
}

#[test]
fn prepare_codex_home_requires_auth_json() {
    let home = tempfile::TempDir::new().expect("temp");
    let error = prepare_codex_home(home.path()).expect_err("no auth.json");
    assert_eq!(error, "codex is not logged in: auth.json missing");
    let target = home.path().join("real-auth.json");
    fs::write(&target, "{}").expect("auth");
    std::os::unix::fs::symlink(&target, home.path().join("auth.json")).expect("symlink");
    assert!(prepare_codex_home(home.path()).is_err());
}
