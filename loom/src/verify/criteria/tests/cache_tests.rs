//! Cache input-fingerprint tests.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::process::Command;

use serial_test::serial;
use tempfile::TempDir;

use crate::models::stage::CommandConfinement;
use crate::verify::criteria::cache::is_cacheable;
use crate::verify::criteria::cache_fingerprint::{
    capture, capture_with_budget, ExecutionIdentity, InputFingerprint,
};
use crate::verify::criteria::confine::{prepare_confined, CommandSpec};

const TEST_IDENTITY: &[&str] = &["-c", "user.name=Test", "-c", "user.email=test@example.com"];

fn git(args: &[&str], dir: &Path) {
    let mut full = TEST_IDENTITY.to_vec();
    full.extend_from_slice(args);
    let status = Command::new("git")
        .args(&full)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed in {}", dir.display());
}

fn init_repo() -> TempDir {
    let temp = TempDir::new().unwrap();
    git(&["init", "-q"], temp.path());
    std::fs::write(temp.path().join("file.txt"), "hello\n").unwrap();
    git(&["add", "file.txt"], temp.path());
    git(&["commit", "-q", "-m", "init"], temp.path());
    temp
}

fn execution_identity(repo: &Path) -> ExecutionIdentity {
    let prepared = prepare_confined(
        &CommandSpec::shell("printf ok"),
        Some(repo),
        CommandConfinement::Confined,
    )
    .unwrap();
    ExecutionIdentity::from_prepared(&prepared, CommandConfinement::Confined).unwrap()
}

fn fingerprint(repo: &Path) -> Option<InputFingerprint> {
    capture(repo, &execution_identity(repo))
}

#[test]
#[serial]
fn tracked_and_untracked_content_mutations_change_fingerprint() {
    let repo = init_repo();
    let clean = fingerprint(repo.path()).unwrap();
    std::fs::write(repo.path().join("file.txt"), "changed\n").unwrap();
    let tracked = fingerprint(repo.path()).unwrap();
    std::fs::write(repo.path().join("new.txt"), "one\n").unwrap();
    let untracked_one = fingerprint(repo.path()).unwrap();
    std::fs::write(repo.path().join("new.txt"), "two\n").unwrap();
    let untracked_two = fingerprint(repo.path()).unwrap();

    assert_ne!(clean, tracked);
    assert_ne!(tracked, untracked_one);
    assert_ne!(untracked_one, untracked_two);
}

#[test]
#[serial]
fn identical_inputs_have_identical_fingerprints() {
    let repo = init_repo();
    let identity = execution_identity(repo.path());
    assert_eq!(
        capture(repo.path(), &identity),
        capture(repo.path(), &identity)
    );
}

#[test]
#[serial]
fn large_same_size_same_mtime_byte_change_is_detected() {
    let repo = init_repo();
    let path = repo.path().join("large.bin");
    std::fs::write(&path, vec![b'a'; 9 * 1024 * 1024]).unwrap();
    let modified = path.metadata().unwrap().modified().unwrap();
    let before = fingerprint(repo.path()).unwrap();

    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&path)
        .unwrap();
    file.write_all(&vec![b'b'; 9 * 1024 * 1024]).unwrap();
    file.set_times(std::fs::FileTimes::new().set_modified(modified))
        .unwrap();
    let metadata = path.metadata().unwrap();
    let after = fingerprint(repo.path()).unwrap();

    assert_eq!(metadata.len(), 9 * 1024 * 1024);
    assert_eq!(metadata.modified().unwrap(), modified);
    assert_ne!(before, after);
}

#[test]
#[serial]
fn executable_content_change_invalidates_execution_identity() {
    let repo = init_repo();
    let tools = TempDir::new().unwrap();
    let executable = tools.path().join("tool");
    std::fs::write(&executable, "#!/bin/sh\nprintf a\n").unwrap();
    make_executable(&executable);
    let spec = CommandSpec::program(executable.to_string_lossy(), std::iter::empty::<&str>());
    let prepared =
        prepare_confined(&spec, Some(repo.path()), CommandConfinement::Confined).unwrap();
    let identity =
        ExecutionIdentity::from_prepared(&prepared, CommandConfinement::Confined).unwrap();
    let before = capture(repo.path(), &identity).unwrap();

    std::fs::write(&executable, "#!/bin/sh\nprintf b\n").unwrap();
    let after = capture(repo.path(), &identity).unwrap();

    assert_ne!(before, after);
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = path.metadata().unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(path, permissions).unwrap();
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) {}

#[test]
fn hash_budget_exhaustion_is_ineligible() {
    let repo = init_repo();
    let identity = execution_identity(repo.path());
    assert!(capture_with_budget(repo.path(), &identity, 0).is_none());
}

#[test]
fn missing_executable_identity_and_inherited_environment_are_ineligible() {
    let repo = init_repo();
    let missing = prepare_confined(
        &CommandSpec::program(
            "loom-cache-test-no-such-executable",
            std::iter::empty::<&str>(),
        ),
        Some(repo.path()),
        CommandConfinement::Confined,
    )
    .unwrap();
    let inherited = prepare_confined(
        &CommandSpec::shell("true"),
        Some(repo.path()),
        CommandConfinement::Inherit,
    )
    .unwrap();
    let dynamic = prepare_confined(
        &CommandSpec::shell("eval dynamic-command"),
        Some(repo.path()),
        CommandConfinement::Confined,
    )
    .unwrap();

    assert!(ExecutionIdentity::from_prepared(&missing, CommandConfinement::Confined).is_none());
    assert!(ExecutionIdentity::from_prepared(&inherited, CommandConfinement::Inherit).is_none());
    assert!(ExecutionIdentity::from_prepared(&dynamic, CommandConfinement::Confined).is_none());
}

#[test]
fn deleted_tracked_input_is_ineligible() {
    let repo = init_repo();
    std::fs::remove_file(repo.path().join("file.txt")).unwrap();
    assert!(fingerprint(repo.path()).is_none());
}

#[cfg(unix)]
#[test]
fn source_symlink_and_unreadable_input_are_ineligible() {
    use std::os::unix::fs::{symlink, PermissionsExt};

    let symlink_repo = init_repo();
    symlink("file.txt", symlink_repo.path().join("source-link")).unwrap();
    git(&["add", "source-link"], symlink_repo.path());
    git(&["commit", "-q", "-m", "symlink"], symlink_repo.path());
    assert!(fingerprint(symlink_repo.path()).is_none());

    let unreadable_repo = init_repo();
    let path = unreadable_repo.path().join("unreadable.txt");
    std::fs::write(&path, "private").unwrap();
    let mut permissions = path.metadata().unwrap().permissions();
    permissions.set_mode(0o000);
    std::fs::set_permissions(&path, permissions).unwrap();
    assert!(fingerprint(unreadable_repo.path()).is_none());
}

#[test]
fn known_external_and_ignored_inputs_are_ineligible() {
    let repo = init_repo();
    std::fs::write(repo.path().join(".gitignore"), "ignored.txt\ntarget/\n").unwrap();
    std::fs::write(repo.path().join("ignored.txt"), "ignored").unwrap();
    let outside = TempDir::new().unwrap();
    let external = outside.path().join("outside.txt");
    std::fs::write(&external, "outside").unwrap();

    assert!(!is_cacheable("cat ignored.txt", repo.path()));
    assert!(!is_cacheable("./target/debug/loom --version", repo.path()));
    assert!(!is_cacheable(
        &format!("cat {}", external.display()),
        repo.path()
    ));
    assert!(is_cacheable("cat new.txt", repo.path()));
}

#[test]
fn ambient_input_refusals_remain_in_force() {
    let repo = init_repo();
    for command in [
        "echo $HOME/file",
        "cat ~/secret",
        "d=$(mktemp -d)",
        "echo ${LOOM_HOME:-/tmp}",
        "echo $USER",
        "curl https://example.com/input",
    ] {
        assert!(!is_cacheable(command, repo.path()), "accepted {command}");
    }
    assert!(is_cacheable("cargo test --lib", repo.path()));
    assert!(!is_cacheable("echo $HOMEBREW_PREFIX/bin", repo.path()));
}
