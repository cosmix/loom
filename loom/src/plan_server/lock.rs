//! Per-project singleton lock and running server record.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions, Permissions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::sync::Mutex;

const MAX_RECORD_BYTES: u64 = 4096;

#[derive(Clone, Serialize, Deserialize)]
pub struct LockRecord {
    pub pid: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

pub enum Acquired {
    Held(ServerLock),
    Running(LockRecord),
}

pub struct ServerLock {
    file: File,
}

/// Shared by the running server, its transport router and cleanup owner. Keeping the
/// singleton here lets admitted handlers finish after a bounded stop without
/// allowing a replacement project server to start in the meantime.
#[derive(Default)]
pub(super) struct Ownership(Mutex<Option<ServerLock>>);

impl Ownership {
    pub(super) fn retain(&self, lock: ServerLock) {
        *self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(lock);
    }

    pub(super) fn publish(&self, url: &str, token: &str) -> Result<()> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_mut()
            .context("server singleton is not owned")?
            .publish(url, token)
    }
}

pub fn acquire(path: &Path) -> Result<Acquired> {
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .with_context(|| format!("failed to open server lock {}", path.display()))?;
    if !file.metadata()?.is_file() {
        bail!("server lock {} is not a regular file", path.display());
    }
    file.set_permissions(Permissions::from_mode(0o600))?;
    // SAFETY: file owns a valid descriptor, and the flags request a nonblocking flock.
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if result != 0 {
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::WouldBlock {
            return Err(error).with_context(|| format!("failed to lock {}", path.display()));
        }
        return Ok(Acquired::Running(read_record(&mut file)?));
    }
    let mut lock = ServerLock { file };
    lock.write_record(&LockRecord {
        pid: std::process::id(),
        url: None,
        token: None,
    })?;
    Ok(Acquired::Held(lock))
}

fn read_record(file: &mut File) -> Result<LockRecord> {
    let mut bytes = Vec::new();
    file.seek(SeekFrom::Start(0))?;
    file.take(MAX_RECORD_BYTES + 1).read_to_end(&mut bytes)?;
    let record = if bytes.len() <= MAX_RECORD_BYTES as usize {
        serde_json::from_slice(&bytes).ok()
    } else {
        None
    };
    Ok(record.unwrap_or(LockRecord {
        pid: 0,
        url: None,
        token: None,
    }))
}

impl ServerLock {
    pub fn publish(&mut self, url: &str, token: &str) -> Result<()> {
        self.write_record(&LockRecord {
            pid: std::process::id(),
            url: Some(url.to_owned()),
            token: Some(token.to_owned()),
        })
    }

    fn write_record(&mut self, record: &LockRecord) -> Result<()> {
        let bytes = serde_json::to_vec(record)?;
        self.file.set_len(0)?;
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&bytes)?;
        self.file.sync_all()?;
        Ok(())
    }

    /// A second descriptor of the locked file, like the copy a child holds until its exec.
    #[cfg(test)]
    pub(super) fn duplicate(&self) -> std::io::Result<File> {
        self.file.try_clone()
    }
}

impl Drop for ServerLock {
    /// Unlocks the open file description itself: a child spawned on another thread holds a
    /// copy of the descriptor until its exec, so closing ours alone would not release the lock.
    /// The record stays; the next holder overwrites it.
    fn drop(&mut self) {
        // SAFETY: file owns a valid descriptor until it closes after this returns.
        unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) };
    }
}
