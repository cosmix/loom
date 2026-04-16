//! File locking utilities for safe concurrent access
//!
//! Provides locked read/write operations using `fs2` advisory locks to prevent
//! data corruption when multiple processes (orchestrator, agents) access the same files.
//!
//! Advisory locks are cooperative - all participants must use these functions
//! for the locking to be effective.

use anyhow::{Context, Result};
use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

/// Read file contents with a shared (read) lock.
///
/// Acquires a shared lock before reading, allowing multiple concurrent readers
/// but blocking while an exclusive (write) lock is held.
pub fn locked_read(path: &Path) -> Result<String> {
    let file =
        File::open(path).with_context(|| format!("Failed to open file: {}", path.display()))?;
    file.lock_shared()
        .with_context(|| format!("Failed to acquire shared lock: {}", path.display()))?;
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
    #[allow(clippy::suspicious_open_options)]
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)
        .with_context(|| {
            format!(
                "Failed to open file for read-modify-write: {}",
                path.display()
            )
        })?;
    file.lock_exclusive()
        .with_context(|| format!("Failed to acquire exclusive lock: {}", path.display()))?;

    let mut content = String::new();
    BufReader::new(&file)
        .read_to_string(&mut content)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    let new_content = modify(content);

    // Truncate AFTER reading and BEFORE writing
    file.set_len(0)
        .with_context(|| format!("Failed to truncate file: {}", path.display()))?;
    use std::io::Seek;
    (&file)
        .seek(std::io::SeekFrom::Start(0))
        .with_context(|| format!("Failed to seek file: {}", path.display()))?;

    let mut writer = BufWriter::new(&file);
    writer
        .write_all(new_content.as_bytes())
        .with_context(|| format!("Failed to write file: {}", path.display()))?;
    writer
        .flush()
        .with_context(|| format!("Failed to flush file: {}", path.display()))?;
    drop(writer);
    file.sync_all()
        .with_context(|| format!("Failed to sync file: {}", path.display()))?;
    Ok(())
}

/// Write file contents with an exclusive (write) lock.
///
/// Acquires an exclusive lock BEFORE truncating the file, preventing the TOCTOU
/// race where another process reads an empty file between truncation and write.
///
/// The sequence is: open → lock → truncate → write → flush.
pub fn locked_write(path: &Path, content: &str) -> Result<()> {
    // Open without truncation - we truncate via set_len(0) AFTER acquiring
    // the exclusive lock to prevent the TOCTOU race where another process
    // reads an empty file between truncation and write completion.
    #[allow(clippy::suspicious_open_options)]
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .open(path)
        .with_context(|| format!("Failed to open file for writing: {}", path.display()))?;
    file.lock_exclusive()
        .with_context(|| format!("Failed to acquire exclusive lock: {}", path.display()))?;
    // Truncate AFTER acquiring the lock to prevent TOCTOU race
    file.set_len(0)
        .with_context(|| format!("Failed to truncate file: {}", path.display()))?;
    let mut writer = BufWriter::new(&file);
    writer
        .write_all(content.as_bytes())
        .with_context(|| format!("Failed to write file: {}", path.display()))?;
    writer
        .flush()
        .with_context(|| format!("Failed to flush file: {}", path.display()))?;
    // Drop writer to release borrow on file
    drop(writer);
    // Ensure data is persisted to disk before releasing lock
    file.sync_all()
        .with_context(|| format!("Failed to sync file: {}", path.display()))?;
    Ok(())
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
