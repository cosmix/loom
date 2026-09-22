use super::*;
use std::thread;

#[test]
fn test_locked_write_and_read() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("test.md");

    locked_write(&path, "hello world").unwrap();
    let content = locked_read(&path).unwrap();
    assert_eq!(content, "hello world");
}

#[test]
fn test_locked_write_overwrites() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("test.md");

    locked_write(&path, "first content").unwrap();
    locked_write(&path, "second").unwrap();
    let content = locked_read(&path).unwrap();
    assert_eq!(content, "second");
}

#[test]
fn test_concurrent_write_safety() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("test-concurrent.md");

    locked_write(&path, "initial").unwrap();

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let path = path.clone();
            thread::spawn(move || {
                let content = format!("content from thread {i}");
                locked_write(&path, &content).unwrap();
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    let final_content = locked_read(&path).unwrap();
    assert!(final_content.starts_with("content from thread"));
}

#[test]
fn test_concurrent_read_write() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("test-rw.md");

    locked_write(&path, "initial content").unwrap();

    let read_path = path.clone();
    let read_handle = thread::spawn(move || {
        for _ in 0..50 {
            let _ = locked_read(&read_path);
        }
    });

    let write_path = path.clone();
    let write_handle = thread::spawn(move || {
        for i in 0..50 {
            locked_write(&write_path, &format!("write {i}")).unwrap();
        }
    });

    read_handle.join().unwrap();
    write_handle.join().unwrap();

    let final_content = locked_read(&path).unwrap();
    assert!(final_content.starts_with("write "));
}

#[test]
fn test_locked_write_leaves_no_tmp_file() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("atomic.md");

    locked_write(&path, "durable content").unwrap();

    // The temp sibling must not survive a successful write.
    let tmp = temp.path().join("atomic.md.tmp");
    assert!(!tmp.exists(), "stray .tmp file left behind: {tmp:?}");
    assert_eq!(locked_read(&path).unwrap(), "durable content");
}

#[test]
fn test_locked_write_replaces_atomically() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("swap.md");

    locked_write(&path, "v1").unwrap();
    // Overwriting replaces the inode via rename; content fully swaps.
    locked_write(&path, "v2-longer-content").unwrap();
    assert_eq!(locked_read(&path).unwrap(), "v2-longer-content");
    // And a shorter write fully replaces the longer content (no leftover
    // tail bytes, which truncate-in-place could leave on a partial write).
    locked_write(&path, "v3").unwrap();
    assert_eq!(locked_read(&path).unwrap(), "v3");
}

#[test]
fn test_locked_write_creates_missing_parent_dir() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("nested").join("dir").join("file.md");

    locked_write(&path, "created").unwrap();
    assert_eq!(locked_read(&path).unwrap(), "created");
}

#[test]
fn test_locked_read_modify_write_basic() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("test-rmw.md");

    locked_write(&path, "hello").unwrap();
    locked_read_modify_write(&path, |s| format!("{s} world")).unwrap();
    let content = locked_read(&path).unwrap();
    assert_eq!(content, "hello world");
}

#[test]
fn test_locked_read_modify_write_creates_file() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("test-rmw-new.md");

    // File does not exist yet — should create it
    locked_read_modify_write(&path, |s| {
        assert!(s.is_empty());
        "created content".to_string()
    })
    .unwrap();
    let content = locked_read(&path).unwrap();
    assert_eq!(content, "created content");
}

#[test]
fn test_locked_dir_update_serializes_find_read_write() {
    // Two files share a directory; concurrent locked_dir_update closures that
    // each read-modify-write one of them must not interleave (the lock is on
    // the directory inode, so all are serialized).
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("d");
    std::fs::create_dir_all(&dir).unwrap();
    let counter = dir.join("counter.txt");
    atomic_write_locked(&counter, "0").unwrap();

    let handles: Vec<_> = (0..20)
        .map(|_| {
            let dir = dir.clone();
            let counter = counter.clone();
            thread::spawn(move || {
                locked_dir_update(&dir, || {
                    let n: u64 = std::fs::read_to_string(&counter)
                        .unwrap()
                        .trim()
                        .parse()
                        .unwrap();
                    atomic_write_locked(&counter, &(n + 1).to_string())
                })
                .unwrap();
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }

    let final_n: u64 = std::fs::read_to_string(&counter)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(final_n, 20, "lost updates under locked_dir_update");
}

#[test]
fn test_locked_dir_update_propagates_closure_value_and_error() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("d2");
    std::fs::create_dir_all(&dir).unwrap();

    let v = locked_dir_update(&dir, || Ok(42u32)).unwrap();
    assert_eq!(v, 42);

    let e: Result<u32> = locked_dir_update(&dir, || anyhow::bail!("boom"));
    assert!(e.is_err());
}

#[test]
fn test_locked_read_modify_write_concurrent_append() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("test-rmw-concurrent.md");

    locked_write(&path, "").unwrap();

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let path = path.clone();
            thread::spawn(move || {
                locked_read_modify_write(&path, |existing| {
                    if existing.is_empty() {
                        format!("line-{i}")
                    } else {
                        format!("{existing}\nline-{i}")
                    }
                })
                .unwrap();
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    let final_content = locked_read(&path).unwrap();
    // All 10 lines should be present — no lost writes
    let line_count = final_content.lines().count();
    assert_eq!(
        line_count, 10,
        "Expected 10 lines but got {line_count}. Content:\n{final_content}"
    );
}

#[test]
fn test_locked_write_refuses_a_symlinked_tmp_file() {
    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let outside_file = outside.path().join("real.md");
    std::fs::write(&outside_file, "outside content").unwrap();

    let path = temp.path().join("linked.md");
    let tmp_path = temp.path().join("linked.md.tmp");
    std::os::unix::fs::symlink(&outside_file, &tmp_path).unwrap();

    let error = locked_write(&path, "attacker content")
        .unwrap_err()
        .to_string();
    assert!(error.contains(&tmp_path.display().to_string()), "{error}");

    // The symlink is left in place, and the file it points at is untouched.
    assert!(tmp_path
        .symlink_metadata()
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        std::fs::read_to_string(&outside_file).unwrap(),
        "outside content"
    );
}
