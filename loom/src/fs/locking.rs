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
//! Writes use a unique sibling temporary file, `fsync` it, and rename it over
//! the target. The directory sync is best effort after publication; failure is
//! logged because the replacement has already happened.
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
use sha2::Digest;
use std::fs::File;
use std::io::{BufReader, Read};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;

/// Serialize Loom plan writers across inode-replacing publication.
///
/// The lock file is keyed by canonical parent and basename, so replacing the
/// plan inode does not change lock identity. Callers that also hold the
/// plan-versions lock must acquire this lock second. External editors do not
/// participate in this advisory lock.
pub fn lock_plan(path: &Path) -> Result<File> {
    let name = path.file_name().context("plan path has no filename")?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()
        .context("Failed to resolve plan parent for lock")?;
    let mut identity = parent.as_os_str().as_encoded_bytes().to_vec();
    identity.push(0);
    identity.extend_from_slice(name.as_encoded_bytes());
    let digest = hex::encode(sha2::Sha256::digest(identity));
    let lock_path = plan_lock_dir()?.join(format!("{digest}.lock"));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&lock_path)
        .with_context(|| format!("Failed to open plan lock: {}", lock_path.display()))?;
    let metadata = file.metadata()?;
    anyhow::ensure!(metadata.is_file(), "Plan lock is not a regular file");
    // SAFETY: geteuid has no preconditions and does not access caller memory.
    let uid = unsafe { libc::geteuid() };
    anyhow::ensure!(metadata.uid() == uid, "Plan lock has another owner");
    anyhow::ensure!(
        metadata.permissions().mode() & 0o022 == 0,
        "Plan lock is writable by others"
    );
    file.lock_exclusive()
        .with_context(|| format!("Failed to lock plan: {}", path.display()))?;
    Ok(file)
}

/// A fixed per-user location avoids per-process TMPDIR differences and does
/// not leave an untracked lock file beside the plan. The mode checks reject a
/// pre-existing directory another user could manipulate.
///
/// A unit test process only contends with itself, and `/tmp` is read-only in a
/// session sandbox, so a test build uses its own temporary directory.
fn plan_lock_dir() -> Result<std::path::PathBuf> {
    use std::os::unix::fs::DirBuilderExt;

    // SAFETY: geteuid has no preconditions and does not access caller memory.
    let uid = unsafe { libc::geteuid() };
    let base = if cfg!(test) {
        std::env::temp_dir()
    } else {
        std::path::PathBuf::from("/tmp")
    };
    let dir = base.join(format!("loom-plan-locks-{uid}"));
    let mut builder = std::fs::DirBuilder::new();
    builder.mode(0o700);
    match builder.create(&dir) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error).context("Failed to create plan lock directory"),
    }
    let metadata = std::fs::symlink_metadata(&dir)?;
    anyhow::ensure!(
        metadata.is_dir() && metadata.uid() == uid,
        "Unsafe plan lock directory"
    );
    anyhow::ensure!(
        metadata.permissions().mode() & 0o077 == 0,
        "Plan lock directory is shared"
    );
    Ok(dir)
}

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

/// Crash-atomically write through a sibling temporary file. The caller holds
/// the parent-directory lock across publication and any surrounding read.
fn atomic_write(path: &Path, content: &str) -> Result<()> {
    let name = path.file_name().context("write target has no filename")?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let dir = File::open(parent)
        .with_context(|| format!("Failed to open directory: {}", parent.display()))?;
    let dir = std::os::fd::OwnedFd::from(dir);
    super::safe_replace::publish_in_dir(&dir, name, content.as_bytes(), 0o666, false)
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
/// Acquires the exclusive parent-directory lock, treats a missing file as empty,
/// calls the modifier function, and atomically publishes the result.
pub fn locked_read_modify_write<F>(path: &Path, modify: F) -> Result<()>
where
    F: FnOnce(String) -> String,
{
    locked_update(path, |content| Ok(modify(content)))
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
