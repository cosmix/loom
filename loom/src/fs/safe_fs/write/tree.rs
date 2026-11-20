//! Directory removal anchored at a dirfd that never follows a symlink.
//!
//! Directories are listed with `fdopendir`/`readdir` (`nix::dir::Dir`) on a descriptor that
//! was itself opened with `openat(O_DIRECTORY | O_NOFOLLOW)`, so the listing reads the very
//! inode that was opened, on macOS and Linux alike. `std::fs::read_dir("/dev/fd/<n>")` was
//! rejected: it re-resolves a path (a procfs magic link on Linux, devfs on macOS) that a
//! sandbox may hide, and the guarantee would rest on that resolution.

use super::super::{path_to_cstring, validate_relpath};
use super::parent_dirfd_for;
use anyhow::{bail, Context, Result};
use nix::dir::Dir;
use nix::errno::Errno;
use nix::fcntl::{openat, OFlag};
use nix::sys::stat::Mode;
use nix::unistd::{unlinkat, UnlinkatFlags};
use std::ffi::{CStr, CString};
use std::os::fd::{AsFd, BorrowedFd, RawFd};
use std::path::Path;

/// Most entries one `safe_remove_tree_in_workdir` call visits before it stops with an error.
const MAX_TREE_ENTRIES: usize = 4096;

/// Deepest level below the removed directory one call descends to. Each level holds one open
/// descriptor and one stack frame, so a deeper chain would run into the descriptor limit or the
/// connection thread's stack; it stops with an error before anything past this level is opened.
const MAX_TREE_DEPTH: usize = 64;

/// Entries visited and removed so far by one tree removal, and the level being emptied.
#[derive(Default)]
struct Walk {
    seen: usize,
    removed: usize,
    depth: usize,
}

/// Remove the directory `relpath` under `dirfd` and everything in it. Every component is
/// opened with `O_NOFOLLOW`; a symlink anywhere (the path, an entry) is unlinked as a link and
/// never followed. Refuses `..` and absolute paths. At most `MAX_TREE_ENTRIES` (4096) entries
/// and `MAX_TREE_DEPTH` (64) levels: past either it stops with an error naming how many it
/// removed. A missing `relpath` is not an
/// error; a non-directory at `relpath` is unlinked like any other entry.
pub fn safe_remove_tree_in_workdir(dirfd: RawFd, relpath: &Path) -> Result<()> {
    let components = validate_relpath(relpath)?;
    let parent = parent_dirfd_for(dirfd, &components)?;
    let name = final_name(&components)?;
    let mut walk = Walk::default();
    let outcome = remove_entry(parent.as_fd(), &name, &mut walk);
    outcome.with_context(|| {
        format!(
            "safe_fs: removing {} stopped after {} removed entries",
            relpath.display(),
            walk.removed
        )
    })
}

/// Remove the directory `relpath` under `dirfd` when it is empty. True when removed; false
/// when it holds entries or does not exist. A symlink at `relpath` is refused, never followed.
pub fn safe_remove_empty_dir_in_workdir(dirfd: RawFd, relpath: &Path) -> Result<bool> {
    let components = validate_relpath(relpath)?;
    let parent = parent_dirfd_for(dirfd, &components)?;
    let name = final_name(&components)?;
    match unlinkat(parent.as_fd(), name.as_c_str(), UnlinkatFlags::RemoveDir) {
        Ok(()) => Ok(true),
        // POSIX allows EEXIST in place of ENOTEMPTY for a non-empty directory.
        Err(Errno::ENOTEMPTY | Errno::EEXIST | Errno::ENOENT) => Ok(false),
        Err(error) => {
            Err(error).with_context(|| format!("safe_fs: rmdir failed on {}", relpath.display()))
        }
    }
}

fn final_name(components: &[Vec<u8>]) -> Result<CString> {
    let last = components
        .last()
        .context("safe_fs: validated path has no components")?;
    path_to_cstring(last)
}

/// Remove `name` under `parent`: a directory is emptied and removed, anything else (a
/// symlink included) is unlinked as itself.
fn remove_entry(parent: BorrowedFd<'_>, name: &CStr, walk: &mut Walk) -> Result<()> {
    if walk.depth > MAX_TREE_DEPTH {
        bail!("safe_fs: the tree is more than {MAX_TREE_DEPTH} levels deep");
    }
    // O_NONBLOCK keeps a FIFO from blocking should the kernel open it before the type check.
    let flags = OFlag::O_DIRECTORY
        | OFlag::O_NOFOLLOW
        | OFlag::O_CLOEXEC
        | OFlag::O_RDONLY
        | OFlag::O_NONBLOCK;
    match openat(parent, name, flags, Mode::empty()) {
        Ok(fd) => {
            let dir = Dir::from_fd(fd).with_context(|| {
                format!(
                    "safe_fs: fdopendir failed on '{}'",
                    name.to_bytes().escape_ascii()
                )
            })?;
            empty_dir(dir, walk)?;
            unlink(parent, name, UnlinkatFlags::RemoveDir, walk)
        }
        // A symlink fails with ELOOP (Linux; macOS without O_DIRECTORY) or ENOTDIR (macOS with
        // O_DIRECTORY); a non-directory fails with ENOTDIR. Each is unlinked, never followed.
        Err(Errno::ELOOP | Errno::ENOTDIR) => {
            unlink(parent, name, UnlinkatFlags::NoRemoveDir, walk)
        }
        Err(Errno::ENOENT) => Ok(()),
        Err(error) => Err(error).with_context(|| {
            format!(
                "safe_fs: openat failed on '{}'",
                name.to_bytes().escape_ascii()
            )
        }),
    }
}

/// Remove every entry of `dir`. The names are read in full before any is removed, because
/// readdir's behaviour on a directory that changes under it is unspecified.
fn empty_dir(mut dir: Dir, walk: &mut Walk) -> Result<()> {
    let names = list_names(&mut dir, walk)?;
    walk.depth += 1;
    for name in &names {
        remove_entry(dir.as_fd(), name, walk)?;
    }
    walk.depth -= 1;
    Ok(())
}

fn list_names(dir: &mut Dir, walk: &mut Walk) -> Result<Vec<CString>> {
    let mut names = Vec::new();
    for entry in dir.iter() {
        let entry = entry.context("safe_fs: readdir failed")?;
        let name = entry.file_name();
        if matches!(name.to_bytes(), b"." | b"..") {
            continue;
        }
        walk.seen += 1;
        if walk.seen > MAX_TREE_ENTRIES {
            bail!("safe_fs: the tree holds more than {MAX_TREE_ENTRIES} entries");
        }
        names.push(name.to_owned());
    }
    Ok(names)
}

fn unlink(parent: BorrowedFd<'_>, name: &CStr, flag: UnlinkatFlags, walk: &mut Walk) -> Result<()> {
    match unlinkat(parent, name, flag) {
        Ok(()) => {
            walk.removed += 1;
            Ok(())
        }
        Err(Errno::ENOENT) => Ok(()),
        Err(error) => Err(error).with_context(|| {
            format!(
                "safe_fs: unlinkat failed on '{}'",
                name.to_bytes().escape_ascii()
            )
        }),
    }
}
