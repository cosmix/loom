//! Descriptor walk that removes a directory tree without ever following a
//! symlink: a link is unlinked, never descended into, so a tree writable by
//! untrusted code cannot redirect removal into another directory.

use nix::dir::Dir;
use nix::errno::Errno;
use nix::fcntl::{AtFlags, OFlag};
use nix::sys::stat::{fchmod, fchmodat, fstatat, FchmodatFlags, Mode, SFlag};
use nix::unistd::{unlinkat, UnlinkatFlags};
use nix::NixPath;
use std::ffi::{OsStr, OsString};
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::ffi::OsStrExt;

/// Deeper trees are refused (`ELOOP`) rather than recursed into without bound.
const MAX_DEPTH: usize = 64;

/// Open a directory entry without following a final symlink. One closed to
/// its owner is opened to the owner first; the open reports any other failure.
pub(crate) fn open_directory<P: ?Sized + NixPath>(
    parent: BorrowedFd<'_>,
    name: &P,
) -> nix::Result<Dir> {
    let flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
    match Dir::openat(parent, name, flags, Mode::empty()) {
        Err(Errno::EACCES) => {
            fchmodat(parent, name, Mode::S_IRWXU, FchmodatFlags::NoFollowSymlink)?;
            Dir::openat(parent, name, flags, Mode::empty())
        }
        result => result,
    }
}

/// The names in `directory`, without `.` and `..`.
pub(crate) fn entries(directory: &mut Dir) -> nix::Result<Vec<OsString>> {
    directory
        .iter()
        .filter_map(|entry| match entry {
            Ok(entry) => {
                let name = OsStr::from_bytes(entry.file_name().to_bytes());
                (name != "." && name != "..").then(|| Ok(name.to_os_string()))
            }
            Err(error) => Some(Err(error)),
        })
        .collect()
}

/// The file-type bits of a `st_mode`.
pub(crate) fn kind(mode: libc::mode_t) -> SFlag {
    SFlag::from_bits_truncate(mode) & SFlag::S_IFMT
}

/// Remove `name` under `parent`, a directory with its contents, never
/// following a symlink. An entry already gone (a concurrent remover) is fine.
pub(crate) fn remove_entry(parent: BorrowedFd<'_>, name: &OsStr, depth: usize) -> nix::Result<()> {
    let status = match fstatat(parent, name, AtFlags::AT_SYMLINK_NOFOLLOW) {
        Err(Errno::ENOENT) => return Ok(()),
        status => status?,
    };
    let removed = if kind(status.st_mode) == SFlag::S_IFDIR {
        remove_tree(parent, name, depth)
    } else {
        unlinkat(parent, name, UnlinkatFlags::NoRemoveDir)
    };
    match removed {
        Err(Errno::ENOENT) => Ok(()),
        removed => removed,
    }
}

fn remove_tree(parent: BorrowedFd<'_>, name: &OsStr, depth: usize) -> nix::Result<()> {
    if depth >= MAX_DEPTH {
        return Err(Errno::ELOOP);
    }
    let mut directory = open_directory(parent, name)?;
    // A directory closed to writes (go's module cache) cannot be emptied
    // until its owner opens it.
    fchmod(&directory, Mode::S_IRWXU)?;
    for child in entries(&mut directory)? {
        remove_entry(directory.as_fd(), &child, depth + 1)?;
    }
    unlinkat(parent, name, UnlinkatFlags::RemoveDir)
}
