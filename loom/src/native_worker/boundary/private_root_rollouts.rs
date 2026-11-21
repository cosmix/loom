//! A released macOS boundary's Codex rollouts, moved out of its private root
//! before the root is removed. The worker ran with `CODEX_HOME` in the root,
//! so its rollouts sit in `codex-home/sessions/`, which token discovery never
//! reads; they move to the same relative path below the host Codex sessions
//! tree (`provider_data::codex_discovery::codex_root`), where it does.
//!
//! The root is walked from a trusted, opened base (`super::walk`) with
//! `openat` and `fstatat` that never follow a link, as removal is: the stage
//! cache is writable by the stage's host jobs, and a link planted there must
//! not make loom move another directory's files. Only regular files named
//! `rollout-*.jsonl` and linked once move; a symlink, a file with another
//! hard link or anything else stays and goes with the root. The boundary is proven released before this runs, so nothing in it
//! changes during the move.

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::Read;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use nix::dir::Dir;
use nix::errno::Errno;
use nix::fcntl::{openat, AtFlags, OFlag};
use nix::sys::stat::{fstatat, Mode, SFlag};
use nix::unistd::{linkat, unlinkat, UnlinkatFlags};

use super::CODEX_HOME_DIR;
use crate::fs::no_follow_tree::{entries, kind, open_directory};
use crate::provider_data::codex_discovery::MAX_DEPTH;

/// The block size two rollouts are compared in.
const CHUNK: usize = 64 * 1024;

/// One rollout to move: its directory in the private tree, its name and its
/// directory path relative to the sessions root.
struct Rollout {
    dir: OwnedFd,
    name: OsString,
    relative: PathBuf,
}

/// Moves every rollout below `<root>/codex-home/sessions` to the same
/// relative path below `host_sessions`, never replacing a file there. A
/// destination that already holds the same bytes (a copy across devices
/// interrupted after its link) counts as moved. Returns how many moved. `Err` names the first
/// rollout that could not move; the ones before it have moved, the rest
/// stay, and a retry continues.
pub(super) fn move_rollouts(root: &Path, host_sessions: Option<&Path>) -> Result<usize, String> {
    let rollouts = private_rollouts(root).map_err(|errno| {
        format!(
            "read the rollouts below {}: {errno}",
            root.join(CODEX_HOME_DIR).display()
        )
    })?;
    if rollouts.is_empty() {
        return Ok(0);
    }
    let Some(host_sessions) = host_sessions else {
        return Err("no host Codex sessions directory is known (HOME is unset)".into());
    };
    for rollout in &rollouts {
        let target = host_sessions.join(&rollout.relative).join(&rollout.name);
        move_one(rollout, &target)
            .map_err(|error| format!("move {:?} to {}: {error}", rollout.name, target.display()))?;
    }
    Ok(rollouts.len())
}

/// The rollouts of the private sessions tree; none when it is absent.
fn private_rollouts(root: &Path) -> nix::Result<Vec<Rollout>> {
    let base = match super::walk(root) {
        Ok(base) => base,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io_errno(error)),
    };
    let home = match open_directory(base.as_fd(), CODEX_HOME_DIR) {
        Ok(home) => home,
        Err(Errno::ENOENT) => return Ok(Vec::new()),
        Err(errno) => return Err(errno),
    };
    let mut sessions = match open_directory(home.as_fd(), "sessions") {
        Ok(sessions) => sessions,
        Err(Errno::ENOENT) => return Ok(Vec::new()),
        Err(errno) => return Err(errno),
    };
    let mut found = Vec::new();
    collect(&mut sessions, PathBuf::new(), 0, &mut found)?;
    Ok(found)
}

fn collect(
    dir: &mut Dir,
    relative: PathBuf,
    depth: usize,
    found: &mut Vec<Rollout>,
) -> nix::Result<()> {
    for name in entries(dir)? {
        let status = fstatat(dir.as_fd(), name.as_os_str(), AtFlags::AT_SYMLINK_NOFOLLOW)?;
        match kind(status.st_mode) {
            SFlag::S_IFDIR if depth < MAX_DEPTH => {
                let mut child = open_directory(dir.as_fd(), name.as_os_str())?;
                collect(&mut child, relative.join(&name), depth + 1, found)?;
            }
            // The worker can write this tree, so a hard link here may name a
            // file outside it; linking that inode into the host sessions tree
            // would expose and share it there. Only a file linked once moves.
            // A move interrupted after its link leaves the rollout linked in
            // both trees: the host copy is in place and this one goes with
            // the root.
            SFlag::S_IFREG if status.st_nlink == 1 && is_rollout_name(&name) => {
                found.push(Rollout {
                    dir: dir.as_fd().try_clone_to_owned().map_err(io_errno)?,
                    name,
                    relative: relative.clone(),
                })
            }
            _ => {}
        }
    }
    Ok(())
}

fn is_rollout_name(name: &OsStr) -> bool {
    let bytes = name.as_bytes();
    bytes.starts_with(b"rollout-") && bytes.ends_with(b".jsonl")
}

fn io_errno(error: std::io::Error) -> Errno {
    error.raw_os_error().map_or(Errno::EIO, Errno::from_raw)
}

/// Links the rollout at `target` without replacing anything, copying through
/// a partial file first when the two trees are on different devices, then
/// unlinks it from the private tree.
fn move_one(rollout: &Rollout, target: &Path) -> std::io::Result<()> {
    let parent = target.parent().unwrap_or(Path::new("/"));
    std::fs::create_dir_all(parent)?;
    let target_dir = File::open(parent)?;
    let name = rollout.name.as_os_str();
    let linked = linkat(
        rollout.dir.as_fd(),
        name,
        target_dir.as_fd(),
        name,
        AtFlags::empty(),
    );
    match linked {
        Ok(()) => {}
        Err(Errno::EEXIST) => already_there(rollout, target)?,
        Err(Errno::EXDEV) => copy_across(rollout, target_dir.as_fd(), target)?,
        Err(errno) => return Err(errno.into()),
    }
    unlinkat(rollout.dir.as_fd(), name, UnlinkatFlags::NoRemoveDir)?;
    Ok(())
}

/// Accepts an existing `target` only when it holds the rollout's bytes.
fn already_there(rollout: &Rollout, target: &Path) -> std::io::Result<()> {
    if same_bytes(open_private(rollout)?, File::open(target)?)? {
        return Ok(());
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "another file already has that name; it is left as it is",
    ))
}

/// Copies the rollout into a new partial file beside `target`, syncs it and
/// links it into place, so `target` never holds a partial copy.
fn copy_across(
    rollout: &Rollout,
    target_dir: BorrowedFd<'_>,
    target: &Path,
) -> std::io::Result<()> {
    let mut partial_name = OsString::from(".");
    partial_name.push(&rollout.name);
    partial_name.push(".loom-partial");
    let partial = target.with_file_name(&partial_name);
    let _ = std::fs::remove_file(&partial);
    let linked = write_partial(rollout, &partial).map(|()| {
        linkat(
            target_dir,
            partial_name.as_os_str(),
            target_dir,
            rollout.name.as_os_str(),
            AtFlags::empty(),
        )
    });
    // Whether the copy, the sync or the link failed, the partial file never
    // stays in the host sessions tree.
    let _ = std::fs::remove_file(&partial);
    match linked? {
        Ok(()) => Ok(()),
        Err(Errno::EEXIST) => already_there(rollout, target),
        Err(errno) => Err(errno.into()),
    }
}

/// Streams the rollout into the new 0600 file `partial` and syncs it.
fn write_partial(rollout: &Rollout, partial: &Path) -> std::io::Result<()> {
    let mut copy = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(partial)?;
    std::io::copy(&mut open_private(rollout)?, &mut copy)?;
    copy.sync_all()
}

/// The rollout, opened without following a link.
fn open_private(rollout: &Rollout) -> std::io::Result<File> {
    let flags = OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC;
    let file = openat(
        rollout.dir.as_fd(),
        rollout.name.as_os_str(),
        flags,
        Mode::empty(),
    )?;
    Ok(File::from(file))
}

/// Whether `left` and `right` hold the same bytes, read [`CHUNK`] at a time.
fn same_bytes(mut left: File, mut right: File) -> std::io::Result<bool> {
    if left.metadata()?.len() != right.metadata()?.len() {
        return Ok(false);
    }
    let (mut ours, mut theirs) = (vec![0; CHUNK], vec![0; CHUNK]);
    loop {
        let read = fill(&mut left, &mut ours)?;
        if read != fill(&mut right, &mut theirs)? || ours[..read] != theirs[..read] {
            return Ok(false);
        }
        if read == 0 {
            return Ok(true);
        }
    }
}

/// Reads into `buffer` until it is full or the file ends; the bytes read.
fn fill(file: &mut File, buffer: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match file.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(read) => filled += read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(filled)
}

#[cfg(test)]
#[path = "private_root_rollouts_copy_tests.rs"]
mod copy_tests;
