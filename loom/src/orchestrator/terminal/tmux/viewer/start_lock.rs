//! The lock tmux clients hold while starting a server on a socket.
//!
//! A tmux client whose connect is refused or finds no socket takes an exclusive
//! `flock` on `<socket>.lock`, connects once more, unlinks the socket and starts a
//! server. The server binds and listens, then unlinks the lock file and closes it
//! (`client_connect` in tmux's client.c, `server_start` in its server.c, read at
//! tag 3.6). So between its bind and its listen a live server refuses connections
//! exactly as a dead one does, and only this lock tells them apart.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

/// Lock files replaced under one attempt before giving up. Each replacement means
/// a server finished starting in that instant, so more than a few is not expected.
const REPLACED_LOCK_ATTEMPTS: usize = 8;

/// tmux's start lock on one socket, held until dropped.
pub(super) struct StartLock {
    _file: File,
    path: PathBuf,
}

impl StartLock {
    /// Take the lock on `socket` without waiting. `None` when another process holds
    /// it: a client is starting a server right now.
    pub(super) fn try_take(socket: &Path) -> io::Result<Option<Self>> {
        let mut path = socket.as_os_str().to_owned();
        path.push(".lock");
        let path = PathBuf::from(path);
        for _ in 0..REPLACED_LOCK_ATTEMPTS {
            let file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&path)?;
            // tmux uses flock(2), which is what must exclude it.
            // SAFETY: `file` is an open descriptor for the duration of the call.
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                let error = io::Error::last_os_error();
                return match error.kind() {
                    io::ErrorKind::WouldBlock => Ok(None),
                    _ => Err(error),
                };
            }
            // A starting server unlinks the file it locked, so a lock on an unlinked
            // file excludes nobody who opens the path after that.
            if is_at_path(&file, &path)? {
                return Ok(Some(Self { _file: file, path }));
            }
        }
        Err(io::Error::other(format!(
            "the lock file keeps being replaced: {}",
            path.display()
        )))
    }
}

fn is_at_path(file: &File, path: &Path) -> io::Result<bool> {
    let held = file.metadata()?;
    Ok(match fs::symlink_metadata(path) {
        Ok(current) => current.dev() == held.dev() && current.ino() == held.ino(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => return Err(error),
    })
}

/// Unlink, and the file closes after this body, as a tmux server does: a client
/// waiting on it wakes, connects again, and takes a new lock if that is refused.
impl Drop for StartLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
