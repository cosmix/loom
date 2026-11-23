use super::{safe_create_new_dir_in_workdir, safe_remove_in_workdir, safe_rename_in_workdir};
use crate::fs::safe_fs::safe_open_dirfd;
use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use tempfile::TempDir;

fn non_utf8_parent() -> (TempDir, &'static Path) {
    let root = TempDir::new().unwrap();
    let parent = Path::new(OsStr::from_bytes(b"a\xff"));
    fs::create_dir(root.path().join(parent)).unwrap();
    (root, parent)
}

#[test]
fn a_directory_is_created_under_a_non_utf8_parent() {
    let (root, parent) = non_utf8_parent();
    let dirfd = safe_open_dirfd(root.path()).unwrap();
    let made = parent.join("made");

    safe_create_new_dir_in_workdir(dirfd.as_raw_fd(), &made, 0o700).unwrap();

    assert!(root.path().join(made).is_dir());
}

#[test]
fn a_rename_under_a_non_utf8_parent_lands_there() {
    let (root, parent) = non_utf8_parent();
    let dirfd = safe_open_dirfd(root.path()).unwrap();
    let src = parent.join("src");
    let dst = parent.join("dst");
    fs::write(root.path().join(&src), b"content").unwrap();

    safe_rename_in_workdir(dirfd.as_raw_fd(), &src, &dst).unwrap();

    assert!(root.path().join(dst).is_file());
    assert!(!root.path().join(src).exists());
}

#[test]
fn a_remove_under_a_non_utf8_parent_removes_that_file() {
    let (root, parent) = non_utf8_parent();
    let dirfd = safe_open_dirfd(root.path()).unwrap();
    let dst = parent.join("dst");
    let file = root.path().join(&dst);
    fs::write(&file, b"content").unwrap();

    safe_remove_in_workdir(dirfd.as_raw_fd(), &dst).unwrap();

    assert!(!file.exists());
}
