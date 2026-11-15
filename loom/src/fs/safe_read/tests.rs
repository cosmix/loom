use super::*;

#[test]
fn regular_no_follow_accepts_only_a_regular_file() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("file");
    let directory = root.path().join("directory");
    let symlink = root.path().join("symlink");
    let fifo = root.path().join("fifo");
    std::fs::write(&file, b"content").unwrap();
    std::fs::create_dir(&directory).unwrap();
    std::os::unix::fs::symlink(&file, &symlink).unwrap();
    nix::unistd::mkfifo(&fifo, Mode::S_IRWXU).unwrap();

    assert!(is_regular_no_follow(&file));
    for path in [symlink, directory, fifo, root.path().join("missing")] {
        assert!(!is_regular_no_follow(&path), "{}", path.display());
    }
}

#[test]
fn a_file_over_the_limit_yields_the_typed_error_with_its_wording() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("big"), [b'x'; 8]).unwrap();

    let error = read_bounded(root.path(), Path::new("big"), 4).unwrap_err();
    let typed = error.downcast_ref::<OverLimit>().expect("typed error");
    assert_eq!(typed.stage, OverLimitStage::Stat);
    assert_eq!(
        error.to_string(),
        "big exceeds the 4 byte verification limit"
    );
}

#[test]
fn a_file_that_grows_while_reading_yields_the_typed_error() {
    let error = read_capped(&[b'x'; 8][..], 2, Path::new("grown"), 4).unwrap_err();
    let typed = error.downcast_ref::<OverLimit>().expect("typed error");
    assert_eq!(typed.stage, OverLimitStage::Read);
    assert_eq!(
        error.to_string(),
        "grown grew beyond the 4 byte verification limit while reading"
    );
}

#[test]
fn read_capped_returns_a_body_at_exactly_the_limit() {
    let bytes = read_capped(&[b'x'; 4][..], 4, Path::new("ok"), 4).unwrap();
    assert_eq!(bytes.len(), 4);
}

#[test]
fn an_open_file_is_read_within_the_limit_and_refused_over_it_by_stat() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("body");
    std::fs::write(&path, [b'x'; 8]).unwrap();

    let bytes = read_open_bounded(File::open(&path).unwrap(), Path::new("body"), 8).unwrap();
    assert_eq!(bytes.len(), 8);

    let error = read_open_bounded(File::open(&path).unwrap(), Path::new("body"), 4).unwrap_err();
    let typed = error.downcast_ref::<OverLimit>().expect("typed error");
    assert_eq!(typed.stage, OverLimitStage::Stat);
}

#[test]
fn an_open_directory_is_refused_as_not_a_regular_file() {
    let root = tempfile::tempdir().unwrap();
    let error =
        read_open_bounded(File::open(root.path()).unwrap(), Path::new("dir"), 8).unwrap_err();
    assert!(format!("{error:#}").contains("not a regular file"));
}

#[test]
fn rejects_leaf_symlink() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::NamedTempFile::new().unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("link")).unwrap();

    assert!(read_bounded(root.path(), Path::new("link"), 1024).is_err());
}

#[test]
fn rejects_intermediate_symlink() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret"), b"secret").unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("dir")).unwrap();

    assert!(read_bounded(root.path(), Path::new("dir/secret"), 1024).is_err());
}

/// A FIFO with no writer would block a plain `open()` forever; the read
/// is refused instead, on another thread so a regression fails the test
/// rather than hanging it.
#[test]
fn rejects_fifo_without_blocking() {
    let root = tempfile::tempdir().unwrap();
    nix::unistd::mkfifo(&root.path().join("pipe"), nix::sys::stat::Mode::S_IRWXU).unwrap();
    let dir = root.path().to_path_buf();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = read_bounded(&dir, Path::new("pipe"), 1024);
        sender
            .send(result.map_err(|error| format!("{error:#}")))
            .unwrap();
    });

    let result = receiver
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("read_bounded blocked on a FIFO");

    let message = result.unwrap_err();
    assert!(message.contains("not a regular file"), "{message}");
}

fn sorted_listing(root: &OwnedFd, relative: &str) -> Vec<(OsString, EntryKind)> {
    let mut entries = list_dir_no_follow(root, Path::new(relative)).unwrap();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}

#[test]
fn lists_entries_by_kind_without_dot_entries() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("dir/sub")).unwrap();
    std::fs::write(root.path().join("dir/file"), b"").unwrap();
    nix::unistd::mkfifo(&root.path().join("dir/pipe"), Mode::S_IRWXU).unwrap();
    std::os::unix::fs::symlink("/dev", root.path().join("dir/link")).unwrap();
    let root_fd = safe_open_dirfd(root.path()).unwrap();

    assert_eq!(
        sorted_listing(&root_fd, ""),
        vec![("dir".into(), EntryKind::Directory)]
    );
    assert_eq!(
        sorted_listing(&root_fd, "dir"),
        vec![
            ("file".into(), EntryKind::File),
            ("link".into(), EntryKind::Symlink),
            ("pipe".into(), EntryKind::Special),
            ("sub".into(), EntryKind::Directory),
        ]
    );
}

/// A directory swapped for a symlink to an outside tree, as the last or
/// an intermediate component, is refused rather than listed.
#[test]
fn refuses_to_list_through_a_symlink() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir(outside.path().join("sub")).unwrap();
    nix::unistd::mkfifo(&outside.path().join("sub/pipe"), Mode::S_IRWXU).unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("dir")).unwrap();
    let root_fd = safe_open_dirfd(root.path()).unwrap();

    for relative in ["dir", "dir/sub"] {
        assert!(list_dir_no_follow(&root_fd, Path::new(relative)).is_err());
    }
}
