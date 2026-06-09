//! File locking utilities for safe concurrent access
//!
//! Provides locked read/write operations using `fs2` advisory locks to prevent
//! data corruption when multiple processes (orchestrator, agents) access the same files.
//!
//! Advisory locks are cooperative - all participants must use these functions
//! for the locking to be effective.
//!
//! # Crash atomicity vs. advisory-lock identity
//!
//! Writes are crash-atomic: content is written to a sibling `<file>.tmp`,
//! `fsync`ed, and `rename`d over the target, then the containing directory is
//! `fsync`ed. A crash mid-write therefore leaves either the old file intact or
//! the fully-written new file — never a truncated/empty file.
//!
//! `flock` identity is tied to an inode, but `rename` swaps the target's inode,
//! so a lock taken on the *data file* would not exclude a reader that opened the
//! pre-rename inode. To keep readers and writers mutually exclusive across the
//! rename, the lock is held on the **parent directory** (a stable inode that the
//! rename does not replace) rather than on the data file itself. All read and
//! write helpers in this module lock the same parent directory, so they remain
//! cooperatively serialized.

use anyhow::{Context, Result};
use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

/// Open the parent directory of `path` and acquire an advisory lock on it.
///
/// The returned `File` owns the lock; dropping it releases the lock. Locking the
/// directory (a stable inode) rather than the data file keeps readers and
/// writers mutually exclusive even when the writer atomically replaces the data
/// file via `rename` (which would otherwise swap the data file's inode and break
/// `flock` identity).
///
/// The parent directory is created if it does not already exist so that callers
/// writing into a fresh subtree do not have to pre-create it.
fn lock_parent_dir(path: &Path, exclusive: bool) -> Result<File> {
    let parent = path.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "Cannot lock parent directory: path has no parent: {}",
            path.display()
        )
    })?;
    let dir = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    if !dir.exists() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("Failed to create directory: {}", dir.display()))?;
    }
    let dir_file =
        File::open(dir).with_context(|| format!("Failed to open directory: {}", dir.display()))?;
    if exclusive {
        dir_file
            .lock_exclusive()
            .with_context(|| format!("Failed to acquire exclusive lock: {}", dir.display()))?;
    } else {
        dir_file
            .lock_shared()
            .with_context(|| format!("Failed to acquire shared lock: {}", dir.display()))?;
    }
    Ok(dir_file)
}

/// Crash-atomically write `content` to `path` via a temp file + `rename`.
///
/// The caller MUST already hold the parent-directory lock (see
/// [`lock_parent_dir`]) for the write to be serialized against readers. The
/// sequence is: write `<file>.tmp` → `sync_all` → `rename` over `<file>` →
/// `fsync` the directory so the rename itself is durable.
fn atomic_write(path: &Path, content: &str) -> Result<()> {
    let mut tmp_os = path.as_os_str().to_os_string();
    tmp_os.push(".tmp");
    let tmp_path = std::path::PathBuf::from(tmp_os);

    {
        let tmp = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp_path)
            .with_context(|| format!("Failed to open temp file: {}", tmp_path.display()))?;
        let mut writer = BufWriter::new(&tmp);
        writer
            .write_all(content.as_bytes())
            .with_context(|| format!("Failed to write temp file: {}", tmp_path.display()))?;
        writer
            .flush()
            .with_context(|| format!("Failed to flush temp file: {}", tmp_path.display()))?;
        drop(writer);
        tmp.sync_all()
            .with_context(|| format!("Failed to sync temp file: {}", tmp_path.display()))?;
    }

    std::fs::rename(&tmp_path, path).with_context(|| {
        format!(
            "Failed to rename {} to {}",
            tmp_path.display(),
            path.display()
        )
    })?;

    // Fsync the directory so the rename (the metadata change that makes the new
    // content visible) is itself durable across a crash.
    if let Some(parent) = path.parent() {
        let dir = if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        };
        if let Ok(dir_file) = File::open(dir) {
            let _ = dir_file.sync_all();
        }
    }

    Ok(())
}

/// Read file contents with a shared (read) lock.
///
/// Acquires a shared lock on the parent directory before reading, allowing
/// multiple concurrent readers but blocking while an exclusive (write) lock is
/// held. The directory lock (rather than a lock on the data file) is what keeps
/// readers excluded from a writer's atomic `rename` swap — see the module docs.
pub fn locked_read(path: &Path) -> Result<String> {
    // Hold the parent-directory lock for the duration of the read so a
    // concurrent atomic write (temp + rename) cannot swap the file out from
    // under us mid-read.
    let _dir_lock = lock_parent_dir(path, false)?;
    let file =
        File::open(path).with_context(|| format!("Failed to open file: {}", path.display()))?;
    let mut content = String::new();
    BufReader::new(&file)
        .read_to_string(&mut content)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;
    Ok(content)
}

/// Read-modify-write with an exclusive lock held throughout.
///
/// Opens the file (creating if needed), acquires exclusive lock, reads content,
/// calls the modifier function, writes the result, and releases the lock.
pub fn locked_read_modify_write<F>(path: &Path, modify: F) -> Result<()>
where
    F: FnOnce(String) -> String,
{
    // Hold the parent-directory lock across the whole read-modify-write so no
    // other reader/writer observes an intermediate state.
    let _dir_lock = lock_parent_dir(path, true)?;

    let content = match File::open(path) {
        Ok(file) => {
            let mut buf = String::new();
            BufReader::new(&file)
                .read_to_string(&mut buf)
                .with_context(|| format!("Failed to read file: {}", path.display()))?;
            buf
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // File does not exist yet — treat as empty so callers can create it.
            String::new()
        }
        Err(e) => {
            return Err(e).with_context(|| {
                format!(
                    "Failed to open file for read-modify-write: {}",
                    path.display()
                )
            });
        }
    };

    let new_content = modify(content);

    // Crash-atomic replace (temp + rename). The directory lock above already
    // serializes this against concurrent readers/writers.
    atomic_write(path, &new_content)
}

/// Fallible read-modify-write with an exclusive lock held throughout.
///
/// Identical locking/atomicity guarantees to [`locked_read_modify_write`], but
/// the `modify` closure may fail (returning `Err`) — e.g. when the read content
/// fails to parse. On a closure error the file is left untouched (no write
/// happens). A missing file is presented to the closure as an empty string.
pub fn locked_update<F>(path: &Path, modify: F) -> Result<()>
where
    F: FnOnce(String) -> Result<String>,
{
    let _dir_lock = lock_parent_dir(path, true)?;

    let content = match File::open(path) {
        Ok(file) => {
            let mut buf = String::new();
            BufReader::new(&file)
                .read_to_string(&mut buf)
                .with_context(|| format!("Failed to read file: {}", path.display()))?;
            buf
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            return Err(e)
                .with_context(|| format!("Failed to open file for update: {}", path.display()));
        }
    };

    let new_content = modify(content)?;
    atomic_write(path, &new_content)
}

/// Write file contents crash-atomically with an exclusive (write) lock.
///
/// Acquires an exclusive lock on the parent directory, then writes to a sibling
/// `<file>.tmp`, `fsync`s it, and `rename`s it over the target. A crash at any
/// point leaves either the old file intact or the fully-written new file — never
/// a truncated/partial file (the failure mode of the old `set_len(0)` +
/// write-in-place approach).
///
/// The directory lock (not a lock on the data file) is what keeps a concurrent
/// [`locked_read`] excluded across the `rename`, since `rename` swaps the data
/// file's inode and would otherwise break `flock` identity — see the module docs.
pub fn locked_write(path: &Path, content: &str) -> Result<()> {
    let _dir_lock = lock_parent_dir(path, true)?;
    atomic_write(path, content)
}

#[cfg(test)]
mod tests {
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
}
