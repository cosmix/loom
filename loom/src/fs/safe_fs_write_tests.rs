use super::*;
use std::os::unix::fs::PermissionsExt;
use std::sync::mpsc;
use std::time::Duration;

fn write_variant(root: &Path, variant: usize) -> Result<()> {
    let dir = safe_open_dirfd(root)?;
    let path = Path::new("target");
    match variant {
        0 => safe_locked_write_in_workdir(dir.as_raw_fd(), path, b"new"),
        1 => safe_append_in_workdir(dir.as_raw_fd(), path, b"new"),
        2 => safe_write_with_mode_in_workdir(dir.as_raw_fd(), path, b"new", 0o755),
        _ => unreachable!(),
    }
}

#[test]
fn write_operations_reject_fifo_without_blocking() {
    for (variant, portable) in (0..3).flat_map(|variant| [(variant, false), (variant, true)]) {
        let root = tempfile::tempdir().unwrap();
        nix::unistd::mkfifo(&root.path().join("target"), nix::sys::stat::Mode::S_IRWXU).unwrap();
        let path = root.path().to_owned();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            if portable {
                force_disable_openat2_for_tests();
            }
            let _ = sender.send(write_variant(&path, variant).is_err());
        });
        assert!(receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("write blocked on FIFO"));
    }
}

#[test]
fn portable_walk_rejects_unsafe_write_targets() {
    force_disable_openat2_for_tests();
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("outside"), b"original").unwrap();
    std::fs::hard_link(root.path().join("outside"), root.path().join("target")).unwrap();
    for variant in 0..3 {
        assert!(write_variant(root.path(), variant).is_err());
    }
    assert_eq!(
        std::fs::read(root.path().join("outside")).unwrap(),
        b"original"
    );
}

#[test]
fn write_operations_reject_hard_links_without_changing_bytes_or_mode() {
    for variant in 0..3 {
        let root = tempfile::tempdir().unwrap();
        let outside = root.path().join("outside");
        std::fs::write(&outside, b"original").unwrap();
        std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o640)).unwrap();
        std::fs::hard_link(&outside, root.path().join("target")).unwrap();

        assert!(write_variant(root.path(), variant).is_err());
        assert_eq!(std::fs::read(&outside).unwrap(), b"original");
        assert_eq!(
            std::fs::metadata(&outside).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }
}

/// A directory outside the project holding one file, `keep.md`, that every removal must leave.
fn outside_keep() -> (tempfile::TempDir, std::path::PathBuf) {
    let outside = tempfile::tempdir().unwrap();
    let keep = outside.path().join("keep.md");
    std::fs::write(&keep, b"keep").unwrap();
    (outside, keep)
}

fn remove_tree(root: &Path, relpath: &str) -> Result<()> {
    let dir = safe_open_dirfd(root)?;
    safe_remove_tree_in_workdir(dir.as_raw_fd(), Path::new(relpath))
}

#[test]
fn remove_tree_unlinks_symlink_entries_without_following_them() {
    let (outside, keep) = outside_keep();
    let root = tempfile::tempdir().unwrap();
    let tree = root.path().join("doc/plans/briefs/demo");
    std::fs::create_dir_all(tree.join("sub/deeper")).unwrap();
    std::fs::write(tree.join("a.md"), b"a").unwrap();
    std::fs::write(tree.join("sub/deeper/b.md"), b"b").unwrap();
    std::os::unix::fs::symlink(outside.path(), tree.join("sub/dir-link")).unwrap();
    std::os::unix::fs::symlink(&keep, tree.join("file-link")).unwrap();

    remove_tree(root.path(), "doc/plans/briefs/demo").unwrap();

    assert!(std::fs::symlink_metadata(&tree).is_err(), "tree remains");
    assert!(root.path().join("doc/plans/briefs").is_dir());
    assert_eq!(std::fs::read(&keep).unwrap(), b"keep");
    // A second removal of the missing tree is not an error.
    remove_tree(root.path(), "doc/plans/briefs/demo").unwrap();
}

#[test]
fn remove_tree_unlinks_a_symlinked_target_as_a_link() {
    let (outside, keep) = outside_keep();
    let root = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("bundle")).unwrap();

    remove_tree(root.path(), "bundle").unwrap();

    assert!(std::fs::symlink_metadata(root.path().join("bundle")).is_err());
    assert_eq!(std::fs::read(&keep).unwrap(), b"keep");
}

#[test]
fn remove_tree_refuses_a_symlinked_parent_and_touches_nothing() {
    for portable in [false, true] {
        let (outside, keep) = outside_keep();
        let root = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("parent")).unwrap();
        std::fs::create_dir(outside.path().join("inner")).unwrap();
        std::fs::write(outside.path().join("inner/x.md"), b"x").unwrap();
        let path = root.path().to_owned();
        let result = std::thread::spawn(move || {
            if portable {
                force_disable_openat2_for_tests();
            }
            remove_tree(&path, "parent/inner").is_err()
        })
        .join()
        .unwrap();

        assert!(
            result,
            "a symlinked parent was traversed (portable={portable})"
        );
        assert!(outside.path().join("inner/x.md").exists());
        assert_eq!(std::fs::read(&keep).unwrap(), b"keep");
        assert!(std::fs::symlink_metadata(root.path().join("parent")).is_ok());
    }
}

#[test]
fn remove_tree_refuses_parent_dir_and_absolute_paths() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("a")).unwrap();
    assert!(remove_tree(root.path(), "a/../a").is_err());
    assert!(remove_tree(root.path(), "/tmp").is_err());
    assert!(root.path().join("a").is_dir());
}

#[test]
fn remove_tree_stops_past_the_entry_limit_and_reports_the_count() {
    let root = tempfile::tempdir().unwrap();
    let tree = root.path().join("big");
    std::fs::create_dir(&tree).unwrap();
    for i in 0..4097 {
        std::fs::write(tree.join(format!("{i}")), b"").unwrap();
    }
    let error = format!("{:#}", remove_tree(root.path(), "big").unwrap_err());
    assert!(error.contains("stopped after 0 removed entries"), "{error}");
    assert!(error.contains("more than 4096 entries"), "{error}");
}

#[test]
fn remove_empty_dir_reports_whether_it_removed() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("full")).unwrap();
    std::fs::write(root.path().join("full/x.md"), b"x").unwrap();
    std::fs::create_dir(root.path().join("empty")).unwrap();
    let dir = safe_open_dirfd(root.path()).unwrap();
    let fd = dir.as_raw_fd();

    assert!(!safe_remove_empty_dir_in_workdir(fd, Path::new("full")).unwrap());
    assert!(root.path().join("full/x.md").exists());
    assert!(safe_remove_empty_dir_in_workdir(fd, Path::new("empty")).unwrap());
    assert!(!root.path().join("empty").exists());
    assert!(!safe_remove_empty_dir_in_workdir(fd, Path::new("empty")).unwrap());
}

/// `root/<name>` with `levels` nested directories under it and a file in the deepest.
fn chain(root: &Path, name: &str, levels: usize) -> std::path::PathBuf {
    let mut path = root.join(name);
    for level in 0..levels {
        path.push(format!("{level}"));
    }
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("leaf.md"), b"").unwrap();
    path
}

#[test]
fn remove_tree_stops_past_the_depth_limit_before_touching_the_deepest_level() {
    let root = tempfile::tempdir().unwrap();
    // The 64th directory's file sits 65 levels down: one past the limit.
    let deepest = chain(root.path(), "deep", 64);
    let error = format!("{:#}", remove_tree(root.path(), "deep").unwrap_err());
    assert!(error.contains("more than 64 levels deep"), "{error}");
    assert!(error.contains("stopped after 0 removed entries"), "{error}");
    assert!(
        deepest.join("leaf.md").exists(),
        "the deepest level was touched"
    );

    chain(root.path(), "shallow", 63);
    remove_tree(root.path(), "shallow").unwrap();
    assert!(std::fs::symlink_metadata(root.path().join("shallow")).is_err());
}
