//! Populates the persistent per-tree Cargo home used where paths cannot be
//! remapped. A stage agent runs as the same user and can write the temporary
//! root, so it can create this directory before loom does or edit it between
//! commands. A `config.toml` there (a target `runner`, `rustflags`, `[env]`, a
//! `[source]` replacement) would change what every later Cargo criterion for
//! the tree runs, and the name cannot be hidden from a process that can list
//! the root. Every populate therefore reduces the home to the entries managed
//! here, through one descriptor opened without following links, so a path
//! swapped during the reset cannot redirect it into another directory.
//!
//! Residual: entries written into the home after populate, before or while the
//! command runs, are not removed. Closing that window needs a home the agent
//! cannot write, which a boundary shared with the same user cannot provide.
use anyhow::{Context, Result};
use nix::dir::Dir;
use nix::errno::Errno;
use nix::fcntl::{readlinkat, AtFlags, OFlag, AT_FDCWD};
use nix::sys::stat::{fchmod, fchmodat, fstat, fstatat, FchmodatFlags, Mode, SFlag};
use nix::unistd::{symlinkat, unlinkat, UnlinkatFlags};
use nix::NixPath;
use std::ffi::{OsStr, OsString};
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

/// Cargo's own state in its home: the package cache locks and the global cache
/// database with its transient SQLite journal. Kept only as regular files.
const STATE_FILES: [&str; 4] = [
    ".package-cache",
    ".package-cache-mutate",
    ".global-cache",
    ".global-cache-journal",
];

/// Deeper unmanaged trees are refused rather than recursed into without bound.
const MAX_DEPTH: usize = 64;

/// Leave `home` holding only `sources` as symlinks named after them and
/// Cargo's regular state files. A source missing on the host gets no entry,
/// so Cargo's downloads there last one command, as in the mounted home.
pub(super) fn populate(home: &Path, sources: &[(&'static str, PathBuf)]) -> Result<()> {
    let mut directory = open_home(home)?;
    let reset = || format!("reset command Cargo home {}", home.display());
    for name in entries(&mut directory).with_context(reset)? {
        if !managed(directory.as_fd(), &name, sources) {
            remove_entry(directory.as_fd(), &name, 0).with_context(reset)?;
        }
    }
    for (name, source) in sources {
        link_source(directory.as_fd(), name, source)
            .with_context(|| format!("link {}", home.join(name).display()))?;
    }
    Ok(())
}

/// Create `home` private to this user, or open the existing one only when it
/// is a directory, not a symlink, owned by this user; it is left open to this
/// user alone.
fn open_home(home: &Path) -> Result<Dir> {
    match std::fs::DirBuilder::new().mode(0o700).create(home) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error).with_context(|| format!("create {}", home.display())),
    }
    let directory = open_directory(AT_FDCWD, home).with_context(|| {
        format!(
            "command Cargo home {} must be a directory of this user, not a symlink",
            home.display()
        )
    })?;
    let status = fstat(&directory)?;
    // SAFETY: `geteuid` has no preconditions and cannot fail.
    let user = unsafe { libc::geteuid() };
    anyhow::ensure!(
        status.st_uid == user,
        "command Cargo home {} is not owned by this user",
        home.display()
    );
    if status.st_mode & 0o777 != 0o700 {
        fchmod(&directory, Mode::S_IRWXU)
            .with_context(|| format!("close command Cargo home {}", home.display()))?;
    }
    Ok(directory)
}

/// Open a directory entry without following a final symlink. One closed to
/// its owner is opened to the owner first; the open reports any other failure.
fn open_directory<P: ?Sized + NixPath>(parent: BorrowedFd<'_>, name: &P) -> nix::Result<Dir> {
    let flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
    match Dir::openat(parent, name, flags, Mode::empty()) {
        Err(Errno::EACCES) => {
            fchmodat(parent, name, Mode::S_IRWXU, FchmodatFlags::NoFollowSymlink)?;
            Dir::openat(parent, name, flags, Mode::empty())
        }
        result => result,
    }
}

fn entries(directory: &mut Dir) -> nix::Result<Vec<OsString>> {
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

/// Source names are settled by `link_source`; state files stay when regular.
fn managed(home: BorrowedFd<'_>, name: &OsStr, sources: &[(&'static str, PathBuf)]) -> bool {
    if sources.iter().any(|(source, _)| name == OsStr::new(source)) {
        return true;
    }
    STATE_FILES.iter().any(|state| name == OsStr::new(state))
        && fstatat(home, name, AtFlags::AT_SYMLINK_NOFOLLOW)
            .is_ok_and(|status| kind(status.st_mode) == SFlag::S_IFREG)
}

fn kind(mode: libc::mode_t) -> SFlag {
    SFlag::from_bits_truncate(mode) & SFlag::S_IFMT
}

/// Remove `name` under `parent`, a directory with its contents, never
/// following a symlink. An entry already gone (a concurrent populate) is fine.
fn remove_entry(parent: BorrowedFd<'_>, name: &OsStr, depth: usize) -> nix::Result<()> {
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
    // A directory closed to writes cannot be emptied until its owner opens it.
    fchmod(&directory, Mode::S_IRWXU)?;
    for child in entries(&mut directory)? {
        remove_entry(directory.as_fd(), &child, depth + 1)?;
    }
    unlinkat(parent, name, UnlinkatFlags::RemoveDir)
}

/// Point `name` at `source`, replacing any other entry there. Concurrent
/// commands for one tree may race here; both write the same target.
fn link_source(home: BorrowedFd<'_>, name: &str, source: &Path) -> nix::Result<()> {
    let linked = || readlinkat(home, name).is_ok_and(|target| Path::new(&target) == source);
    if linked() {
        return Ok(());
    }
    remove_entry(home, OsStr::new(name), 0)?;
    match symlinkat(source, home, name) {
        Err(Errno::EEXIST) if linked() => Ok(()),
        result => result,
    }
}
