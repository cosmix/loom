use super::*;
use crate::fs::permissions::scratch_home::ScratchHome;
use serial_test::serial;
use std::os::unix::ffi::OsStringExt;
use std::process::Command;

fn git(root: &Path, args: &[&std::ffi::OsStr]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn repo() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    git(root.path(), &["init".as_ref()]);
    git(
        root.path(),
        &[
            "commit".as_ref(),
            "--allow-empty".as_ref(),
            "-m".as_ref(),
            "initial".as_ref(),
        ],
    );
    root
}

#[test]
fn preserves_unregistered_worktree_contents() {
    let repo = repo();
    let path = repo.path().join(".worktrees/s");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("unique"), "keep me").unwrap();
    for with_git in [false, true] {
        if with_git {
            std::fs::write(path.join(".git"), "invalid").unwrap();
        }
        let error = get_or_create_worktree("s", repo.path(), None, None).unwrap_err();
        assert!(format!("{error:#}").contains("preserved"));
        assert_eq!(
            std::fs::read_to_string(path.join("unique")).unwrap(),
            "keep me"
        );
    }
}

#[test]
#[serial]
fn recreates_empty_remnant_after_prune() {
    let repo = repo();
    let path = repo.path().join(".worktrees/s");
    std::fs::create_dir_all(&path).unwrap();
    let _home = ScratchHome::set();
    assert!(get_or_create_worktree("s", repo.path(), None, None).is_ok());
    assert!(path.join(".git").exists());
}

#[test]
fn prune_and_listing_errors_preserve_directory() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join(".worktrees/s");
    std::fs::create_dir_all(&path).unwrap();
    assert!(get_or_create_worktree("s", root.path(), None, None).is_err());
    assert!(path.is_dir());
    std::fs::write(path.join(".git"), "invalid").unwrap();
    assert!(get_or_create_worktree("s", root.path(), None, None).is_err());
    assert!(path.join(".git").exists());
}

#[test]
fn listing_preserves_real_git_paths_with_newline_and_non_utf8() {
    let repo = repo();
    let name = std::ffi::OsString::from_vec(b"linked\n\xff".to_vec());
    let path = repo.path().join(name);
    git(
        repo.path(),
        &[
            "worktree".as_ref(),
            "add".as_ref(),
            "--detach".as_ref(),
            path.as_os_str(),
        ],
    );
    let entries = list_worktrees(repo.path()).unwrap();
    assert!(entries.iter().any(|entry| entry.path == path));
    assert!(is_valid_git_worktree(&path, repo.path()).unwrap());
}
