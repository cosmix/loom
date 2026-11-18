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
