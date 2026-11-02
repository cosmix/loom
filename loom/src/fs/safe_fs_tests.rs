use super::*;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

fn read_file(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap()
}

#[test]
fn rejects_absolute_path() {
    let tmp = TempDir::new().unwrap();
    let dirfd = safe_open_dirfd(tmp.path()).unwrap();
    let err = safe_locked_write_in_workdir(dirfd.as_raw_fd(), Path::new("/etc/passwd"), b"x")
        .unwrap_err();
    assert!(err.to_string().contains("absolute"));
}

#[test]
fn rejects_parent_dir_traversal() {
    let tmp = TempDir::new().unwrap();
    let dirfd = safe_open_dirfd(tmp.path()).unwrap();
    let err =
        safe_locked_write_in_workdir(dirfd.as_raw_fd(), Path::new("../escape"), b"x").unwrap_err();
    assert!(err.to_string().contains(".."));
}

#[test]
fn rejects_final_symlink() {
    let tmp = TempDir::new().unwrap();
    let target = tmp.path().join("target.txt");
    std::fs::write(&target, b"original").unwrap();
    let link = tmp.path().join("link.txt");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let dirfd = safe_open_dirfd(tmp.path()).unwrap();
    let err =
        safe_locked_write_in_workdir(dirfd.as_raw_fd(), Path::new("link.txt"), b"new").unwrap_err();
    let s = format!("{:#}", err);
    // ELOOP from O_NOFOLLOW or RESOLVE_NO_SYMLINKS rejection.
    assert!(
        s.contains("symbolic link")
            || s.contains("Too many levels of symbolic links")
            || s.contains("ELOOP")
            || s.contains("loop"),
        "expected symlink rejection, got: {s}"
    );
    // Target must remain untouched.
    assert_eq!(read_file(&target), b"original");
}

#[test]
fn rejects_intermediate_symlink() {
    let tmp = TempDir::new().unwrap();
    let real_dir = tmp.path().join("real");
    std::fs::create_dir(&real_dir).unwrap();
    let link_dir = tmp.path().join("link");
    std::os::unix::fs::symlink(&real_dir, &link_dir).unwrap();
    let dirfd = safe_open_dirfd(tmp.path()).unwrap();
    let err = safe_locked_write_in_workdir(dirfd.as_raw_fd(), Path::new("link/file.txt"), b"x")
        .unwrap_err();
    let s = format!("{:#}", err).to_ascii_lowercase();
    assert!(
        s.contains("symbolic link")
            || s.contains("too many levels of symbolic links")
            || s.contains("eloop")
            || s.contains("loop")
            || s.contains("exdev")
            || s.contains("not permitted")
            // macOS: O_NOFOLLOW|O_DIRECTORY on a symlink yields ENOTDIR
            // rather than Linux's ELOOP. The symlink is refused either
            // way; only the errno differs. Matches the wording already
            // accepted by rejects_intermediate_symlink_portable_path.
            || s.contains("not a directory"),
        "expected intermediate-symlink rejection, got: {s}"
    );
    // Refusal must be real: nothing may be created through the symlink.
    assert!(!real_dir.join("file.txt").exists());
}

#[test]
fn rejects_intermediate_symlink_portable_path() {
    // Force-disable openat2 to exercise the portable openat-walk path.
    let tmp = TempDir::new().unwrap();
    let real_dir = tmp.path().join("real");
    std::fs::create_dir(&real_dir).unwrap();
    let link_dir = tmp.path().join("link");
    std::os::unix::fs::symlink(&real_dir, &link_dir).unwrap();

    let dirfd = safe_open_dirfd(tmp.path()).unwrap();
    force_disable_openat2_for_tests();
    let err = safe_locked_write_in_workdir(dirfd.as_raw_fd(), Path::new("link/file.txt"), b"x")
        .unwrap_err();
    let s = format!("{:#}", err).to_ascii_lowercase();
    assert!(
        s.contains("symbolic link")
            || s.contains("too many levels of symbolic links")
            || s.contains("eloop")
            || s.contains("loop")
            || s.contains("not a directory"),
        "expected intermediate-symlink rejection (portable), got: {s}"
    );
}

#[test]
fn safe_locked_write_then_read() {
    let tmp = TempDir::new().unwrap();
    let dirfd = safe_open_dirfd(tmp.path()).unwrap();
    safe_locked_write_in_workdir(dirfd.as_raw_fd(), Path::new("a.txt"), b"hello").unwrap();
    assert_eq!(read_file(&tmp.path().join("a.txt")), b"hello");
    safe_locked_write_in_workdir(dirfd.as_raw_fd(), Path::new("a.txt"), b"world!").unwrap();
    assert_eq!(read_file(&tmp.path().join("a.txt")), b"world!");
}

#[test]
fn safe_locked_write_concurrent_serialises() {
    use std::sync::Arc;
    use std::thread;
    let tmp = Arc::new(TempDir::new().unwrap());
    let path = tmp.path().join("c.txt");
    // Pre-create so all threads race on the same inode.
    std::fs::write(&path, b"").unwrap();

    let handles: Vec<_> = (0..8)
        .map(|i| {
            let tmp = Arc::clone(&tmp);
            thread::spawn(move || {
                let dirfd = safe_open_dirfd(tmp.path()).unwrap();
                let content = format!("payload-{i}").into_bytes();
                safe_locked_write_in_workdir(dirfd.as_raw_fd(), Path::new("c.txt"), &content)
                    .unwrap();
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let out = read_file(&path);
    let s = String::from_utf8_lossy(&out);
    assert!(s.starts_with("payload-"), "{s}");
}

#[test]
fn safe_append_preserves_order() {
    use std::sync::Arc;
    use std::thread;
    let tmp = Arc::new(TempDir::new().unwrap());

    let handles: Vec<_> = (0..16)
        .map(|i| {
            let tmp = Arc::clone(&tmp);
            thread::spawn(move || {
                let dirfd = safe_open_dirfd(tmp.path()).unwrap();
                let line = format!("line-{i:02}\n");
                safe_append_in_workdir(dirfd.as_raw_fd(), Path::new("log.txt"), line.as_bytes())
                    .unwrap();
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let out = read_file(&tmp.path().join("log.txt"));
    let s = String::from_utf8_lossy(&out);
    let lines: Vec<&str> = s.lines().collect();
    assert_eq!(lines.len(), 16, "expected 16 lines, got {}", lines.len());
    // No torn writes: every line must be of the form "line-NN".
    for l in &lines {
        assert!(l.starts_with("line-"), "torn line: {l:?}");
    }
}

#[test]
fn safe_create_new_refuses_existing() {
    let tmp = TempDir::new().unwrap();
    let dirfd = safe_open_dirfd(tmp.path()).unwrap();
    safe_create_new_in_workdir(dirfd.as_raw_fd(), Path::new("h.txt"), b"first").unwrap();
    let err =
        safe_create_new_in_workdir(dirfd.as_raw_fd(), Path::new("h.txt"), b"second").unwrap_err();
    let s = format!("{:#}", err);
    assert!(
        s.contains("exists") || s.contains("EEXIST"),
        "expected EEXIST, got: {s}"
    );
    assert_eq!(read_file(&tmp.path().join("h.txt")), b"first");
}

#[test]
fn safe_write_with_mode_sets_bits() {
    let tmp = TempDir::new().unwrap();
    let dirfd = safe_open_dirfd(tmp.path()).unwrap();
    safe_write_with_mode_in_workdir(
        dirfd.as_raw_fd(),
        Path::new("script.sh"),
        b"#!/bin/sh\necho ok\n",
        0o755,
    )
    .unwrap();
    let meta = std::fs::metadata(tmp.path().join("script.sh")).unwrap();
    let perm = meta.permissions().mode() & 0o777;
    assert_eq!(perm, 0o755, "got mode {:o}", perm);
}

#[test]
fn safe_create_dir_all_creates_nested() {
    let tmp = TempDir::new().unwrap();
    let dirfd = safe_open_dirfd(tmp.path()).unwrap();
    safe_create_dir_all_in_workdir(dirfd.as_raw_fd(), Path::new("a/b/c"), 0o755).unwrap();
    assert!(tmp.path().join("a/b/c").is_dir());
    // Idempotent.
    safe_create_dir_all_in_workdir(dirfd.as_raw_fd(), Path::new("a/b/c"), 0o755).unwrap();
}

#[test]
fn safe_create_dir_all_rejects_intermediate_symlink() {
    let tmp = TempDir::new().unwrap();
    let real_dir = tmp.path().join("real");
    std::fs::create_dir(&real_dir).unwrap();
    let link_dir = tmp.path().join("link");
    std::os::unix::fs::symlink(&real_dir, &link_dir).unwrap();
    let dirfd = safe_open_dirfd(tmp.path()).unwrap();
    let err = safe_create_dir_all_in_workdir(dirfd.as_raw_fd(), Path::new("link/inner"), 0o755)
        .unwrap_err();
    let s = format!("{:#}", err).to_ascii_lowercase();
    assert!(
        s.contains("symbolic link")
            || s.contains("too many levels of symbolic links")
            || s.contains("eloop")
            || s.contains("loop")
            || s.contains("file exists")
            || s.contains("eexist")
            || s.contains("not a directory"),
        "expected symlink rejection or EEXIST race, got: {s}"
    );
}

#[test]
fn rejects_oversize_write() {
    let tmp = TempDir::new().unwrap();
    let dirfd = safe_open_dirfd(tmp.path()).unwrap();
    let too_big = vec![b'x'; MAX_LOG_BYTES + 1];
    let err = safe_locked_write_in_workdir(dirfd.as_raw_fd(), Path::new("big.txt"), &too_big)
        .unwrap_err();
    assert!(err.to_string().contains("refusing write"));
}

#[test]
fn safe_remove_drops_file() {
    let tmp = TempDir::new().unwrap();
    let dirfd = safe_open_dirfd(tmp.path()).unwrap();
    safe_locked_write_in_workdir(dirfd.as_raw_fd(), Path::new("d.txt"), b"bye").unwrap();
    safe_remove_in_workdir(dirfd.as_raw_fd(), Path::new("d.txt")).unwrap();
    assert!(!tmp.path().join("d.txt").exists());
    // Removing again returns Ok (idempotent for ENOENT).
    safe_remove_in_workdir(dirfd.as_raw_fd(), Path::new("d.txt")).unwrap();
}
