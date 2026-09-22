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
use std::os::unix::fs::OpenOptionsExt;
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
///
/// The temp file is opened with `O_NOFOLLOW`: a committed repository can
/// track `<file>.tmp` as a symlink to a path outside the repo, and this
/// write must not follow it. A symlinked temp path fails closed with an
/// error naming the path; the symlink itself is left untouched.
fn atomic_write(path: &Path, content: &str) -> Result<()> {
    let mut tmp_os = path.as_os_str().to_os_string();
    tmp_os.push(".tmp");
    let tmp_path = std::path::PathBuf::from(tmp_os);

    {
        let tmp = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .custom_flags(libc::O_NOFOLLOW)
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

/// Acquire an exclusive advisory lock on `dir` for the duration of `f`.
///
/// This is the multi-file analogue of [`locked_update`]: it locks a *directory*
/// (a stable inode) so a caller can find-read-modify-write one of several files
/// living under it as a single critical section. Use it when the target file's
/// exact path is not known up front — e.g. depth-prefixed stage files whose
/// `NN-` prefix has to be discovered by enumerating the directory.
///
/// The same directory lock is what [`locked_read`], [`locked_write`], and
/// [`locked_update`] take on a file's *parent* directory, so a `f` that operates
/// on files directly inside `dir` is mutually exclusive with every other locked
/// read/write of those files. Within `f`, perform the actual file replacement via
/// [`atomic_write_locked`] so the write stays crash-atomic.
///
/// The directory is created if it does not exist (mirroring `lock_parent_dir`).
pub fn locked_dir_update<T, F>(dir: &Path, f: F) -> Result<T>
where
    F: FnOnce() -> Result<T>,
{
    // `lock_parent_dir` locks the *parent* of the path it is given, so pass a
    // sentinel child of `dir` to make `dir` itself the locked inode.
    let _dir_lock = lock_parent_dir(&dir.join(".loom-dir-lock"), true)?;
    f()
}

/// Crash-atomically write `content` to `path`, assuming the caller already holds
/// the relevant directory lock (e.g. via [`locked_dir_update`]).
///
/// This is the public entry point for the temp-file + `rename` write used by all
/// the locked writers in this module. It performs NO locking of its own — the
/// caller is responsible for serialization. Calling it outside a held directory
/// lock reintroduces the lost-update/torn-read races these helpers exist to
/// prevent.
pub fn atomic_write_locked(path: &Path, content: &str) -> Result<()> {
    atomic_write(path, content)
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
mod tests;
