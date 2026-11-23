//! Contract `lossy-non-utf8-parent-path`: a rename and a remove under a parent
//! directory whose name is not UTF-8 act on that directory, never on the
//! lossily converted name with U+FFFD in it.

use std::ffi::OsStr;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use tempfile::TempDir;

use super::{safe_open_dirfd, safe_remove_in_workdir, safe_rename_in_workdir};

#[test]
fn a_rename_under_a_non_utf8_parent_lands_there() {
    if cfg!(target_os = "macos") {
        // APFS refuses a file name that is not valid UTF-8.
        return;
    }
    let root = TempDir::new().unwrap();
    let parent = Path::new(OsStr::from_bytes(b"a\xff"));
    std::fs::create_dir(root.path().join(parent)).unwrap();
    std::fs::write(root.path().join(parent).join("src"), b"content").unwrap();
    let dirfd = safe_open_dirfd(root.path()).unwrap();

    safe_rename_in_workdir(dirfd.as_raw_fd(), &parent.join("src"), &parent.join("dst"))
        .expect("rename under the non-UTF-8 parent");
    assert!(root.path().join(parent).join("dst").exists());

    safe_remove_in_workdir(dirfd.as_raw_fd(), &parent.join("dst"))
        .expect("remove under the non-UTF-8 parent");
    assert!(!root.path().join(parent).join("dst").exists());

    assert!(
        !root.path().join("a\u{FFFD}").exists(),
        "a directory named by the lossy conversion appeared"
    );
}
