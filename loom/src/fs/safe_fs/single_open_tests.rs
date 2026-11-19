//! The single-syscall path refuses a symlink in any component and reaches a path whose
//! intermediate directories cannot be opened on their own. An execute-only directory stands in
//! for the Seatbelt profile of a planning session: it can be traversed but not opened, as the
//! planning root there can be resolved through but not read.

use std::fs;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;

use tempfile::TempDir;

use super::super::{open_safely, safe_create_dir_all_in_workdir, safe_open_dirfd};

/// `<tmp>/hidden/grant` with `hidden` execute-only; restores the mode on drop so the temp dir
/// can be removed.
struct Hidden {
    tmp: TempDir,
}

impl Hidden {
    fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("hidden/grant")).unwrap();
        fs::write(tmp.path().join("hidden/grant/f"), b"x").unwrap();
        set_mode(&tmp.path().join("hidden"), 0o100);
        Hidden { tmp }
    }

    fn root(&self) -> &Path {
        self.tmp.path()
    }
}

impl Drop for Hidden {
    fn drop(&mut self) {
        set_mode(&self.tmp.path().join("hidden"), 0o700);
    }
}

fn set_mode(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

fn os_error(error: &anyhow::Error) -> Option<i32> {
    error
        .chain()
        .find_map(|cause| cause.downcast_ref::<std::io::Error>()?.raw_os_error())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_path_below_an_unopenable_directory_opens_in_one_call() {
    let hidden = Hidden::new();
    let anchor = safe_open_dirfd(hidden.root()).unwrap();
    let rel = Path::new("hidden/grant/f");
    open_safely(anchor.as_raw_fd(), rel, libc::O_RDONLY, 0).expect("single-syscall open");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn create_dir_all_starts_below_an_unopenable_directory() {
    let hidden = Hidden::new();
    let anchor = safe_open_dirfd(hidden.root()).unwrap();
    let rel = Path::new("hidden/grant/a/b");
    safe_create_dir_all_in_workdir(anchor.as_raw_fd(), rel, 0o700).expect("create below grant");
    assert!(hidden.root().join("hidden/grant/a/b").is_dir());
    safe_create_dir_all_in_workdir(anchor.as_raw_fd(), rel, 0o700).expect("idempotent");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_symlinked_component_is_refused_by_the_single_call() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("real/sub")).unwrap();
    fs::write(tmp.path().join("real/sub/f"), b"x").unwrap();
    symlink(tmp.path().join("real"), tmp.path().join("link")).unwrap();
    symlink("f", tmp.path().join("real/sub/flink")).unwrap();
    let anchor = safe_open_dirfd(tmp.path()).unwrap();
    let fd = anchor.as_raw_fd();
    // ELOOP for every case: the portable walk would report ENOTDIR on macOS for the
    // intermediate and directory cases, so this errno shows the single call refused them.
    for (rel, flags) in [
        ("link/sub/f", libc::O_RDONLY),
        ("real/sub/flink", libc::O_RDONLY),
        ("link/sub", libc::O_RDONLY | libc::O_DIRECTORY),
    ] {
        let error = open_safely(fd, Path::new(rel), flags, 0).expect_err(rel);
        assert_eq!(os_error(&error), Some(libc::ELOOP), "{rel}: {error:#}");
    }
    let error = safe_create_dir_all_in_workdir(fd, Path::new("link/sub/new"), 0o700)
        .expect_err("created through a symlink");
    assert_eq!(os_error(&error), Some(libc::ELOOP), "{error:#}");
    assert!(!tmp.path().join("real/sub/new").exists());
}

#[test]
fn the_forced_portable_walk_cannot_open_below_an_unopenable_directory() {
    // Proves the first test exercises the single call: the walk opens `hidden` itself.
    let hidden = Hidden::new();
    let anchor = safe_open_dirfd(hidden.root()).unwrap();
    super::force_disable_openat2_for_tests();
    let error = open_safely(
        anchor.as_raw_fd(),
        Path::new("hidden/grant/f"),
        libc::O_RDONLY,
        0,
    )
    .expect_err("the walk opened an execute-only directory");
    assert_eq!(os_error(&error), Some(libc::EACCES), "{error:#}");
}
