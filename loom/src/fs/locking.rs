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
}
