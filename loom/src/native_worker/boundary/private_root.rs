//! The private invocation root of a macOS boundary,
//! `<stage cache>/native/<invocation>`: its temp directory and the Codex home
//! copy the worker runs with. Every ancestor of the trusted base (the stage
//! cache root, or the `native` directory on removal) is checked by `lstat`:
//! a real directory owned by root or this user and not writable by others
//! unless sticky. Only the base itself is opened (`O_NOFOLLOW`, then matched
//! by device and inode against the check), because a sandbox may grant only
//! metadata on the ancestors. Below the base every step is an `openat`,
//! `mkdirat` or `unlinkat` that never follows a link, so a link planted in
//! the stage cache (which the stage's host jobs may write) cannot redirect
//! creation, copying or removal into another directory.
//!
//! Accepted risk: an ancestor owned by root or this user could be renamed or
//! replaced between its check and the open of the base. Only root or this
//! user can do that, and both already control everything the root holds.
//! Revisit if a base is ever placed under a directory a third party owns.
use std::ffi::OsStr;
#[cfg(any(target_os = "macos", test))]
use std::io::{Read, Write};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::path::{Component, Path, PathBuf};

#[cfg(any(target_os = "macos", test))]
use anyhow::{bail, Context, Result};
use nix::errno::Errno;
#[cfg(any(target_os = "macos", test))]
use nix::fcntl::openat;
use nix::fcntl::{AtFlags, OFlag};
#[cfg(any(target_os = "macos", test))]
use nix::sys::stat::mkdirat;
use nix::sys::stat::{fchmod, fstat, fstatat, lstat, Mode, SFlag};
use nix::unistd::{unlinkat, UnlinkatFlags};

use super::Quiescence;
use crate::fs::no_follow_tree::{entries, kind, open_directory};
use crate::native_worker::model::MacIdentity;

/// The stage cache directory holding every invocation root.
pub(crate) const NATIVE_DIR: &str = "native";
/// The private temp directory below an invocation root.
#[cfg(any(target_os = "macos", test))]
pub(crate) const TMP_DIR: &str = "tmp";
/// The private Codex home below an invocation root. Not gated: reclaiming,
/// which reads its rollouts, compiles on every host (a record without a macOS
/// identity reclaims nothing).
pub(crate) const CODEX_HOME_DIR: &str = "codex-home";
/// Deeper trees are refused rather than recursed into without bound.
const MAX_DEPTH: usize = 64;
/// The project-wide instructions file copied beside the auth files.
#[cfg(any(target_os = "macos", test))]
const AGENTS_FILE: &str = "AGENTS.md";

#[path = "private_root_rollouts.rs"]
mod rollouts;

/// A created invocation root and the directories in it; all canonical.
#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PrivateRoot {
    pub(crate) root: PathBuf,
    pub(crate) tmp: PathBuf,
    pub(crate) codex_home: PathBuf,
}

/// Creates `<cache_root>/native/<invocation>` exclusively, mode 0700, with
/// `tmp/`, `codex-home/` and `codex-home/sessions/`, and copies the regular
/// auth files and `AGENTS.md` of `host_codex_home` into `codex-home/`, mode
/// 0600. `cache_root` must be canonical (`StageCache::ensure`). An invocation
/// root that already exists is refused: it is not this launch's.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn create(
    cache_root: &Path,
    invocation: &str,
    host_codex_home: &Path,
) -> Result<PrivateRoot> {
    crate::native_worker::ids::validate_invocation_id(invocation)
        .map_err(|error| anyhow::anyhow!("invalid invocation id {invocation:?}: {error}"))?;
    let cache = walk(cache_root).with_context(|| {
        format!(
            "open the stage cache {} without following links",
            cache_root.display()
        )
    })?;
    let native = owned_dir(cache.as_fd(), NATIVE_DIR, false)
        .with_context(|| format!("create {}/{NATIVE_DIR}", cache_root.display()))?;
    let root = cache_root.join(NATIVE_DIR).join(invocation);
    let invocation_dir = owned_dir(native.as_fd(), invocation, true)
        .with_context(|| format!("create the private root {}", root.display()))?;
    let home = owned_dir(invocation_dir.as_fd(), CODEX_HOME_DIR, true)?;
    owned_dir(home.as_fd(), "sessions", true)?;
    owned_dir(invocation_dir.as_fd(), TMP_DIR, true)?;
    let names = super::mounts::AUTH_FILES.iter().chain([&AGENTS_FILE]);
    for name in names {
        copy_private(host_codex_home, name, home.as_fd())
            .with_context(|| format!("copy {name} into {}", root.display()))?;
    }
    Ok(PrivateRoot {
        tmp: root.join(TMP_DIR),
        codex_home: root.join(CODEX_HOME_DIR),
        root,
    })
}

/// Removes the invocation root `root` and everything in it. Refuses unless
/// the last two components of `root` are `native` and `invocation`; never
/// follows a symlink (a link is unlinked, never descended into) and never
/// crosses onto another device. Already gone is success; an error leaves the
/// rest for a retry.
pub(crate) fn remove(root: &Path, invocation: &str) -> std::io::Result<()> {
    let parent = match walk(invocation_parent(root, invocation)?) {
        Ok(parent) => parent,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let device = fstat(&parent)?.st_dev;
    match remove_entry(parent.as_fd(), OsStr::new(invocation), device, 0) {
        Err(Errno::ENOENT) | Ok(()) => Ok(()),
        Err(errno) => Err(errno.into()),
    }
}

/// The `native` directory holding the invocation root `root`, or why
/// [`remove`] refuses `root`: `invocation` must be an invocation id and `root`
/// the absolute `.../native/<invocation>`.
fn invocation_parent<'a>(root: &'a Path, invocation: &str) -> std::io::Result<&'a Path> {
    let refused = |why: &str| {
        let detail = format!("refusing to remove {}: {why}", root.display());
        std::io::Error::new(std::io::ErrorKind::InvalidInput, detail)
    };
    if crate::native_worker::ids::validate_invocation_id(invocation).is_err() {
        return Err(refused("not an invocation id"));
    }
    let parent = root.parent().filter(|_| root.is_absolute());
    let named = root.file_name() == Some(OsStr::new(invocation))
        && parent.and_then(Path::file_name) == Some(OsStr::new(NATIVE_DIR));
    parent
        .filter(|_| named)
        .ok_or_else(|| refused("not a native/<invocation> root"))
}

/// After `Released`, moves the Codex rollouts of the private invocation root
/// `mac` names into the host Codex sessions tree (`rollouts`), then removes
/// the root (a no-op without a macOS identity or when it is already gone);
/// any other verdict keeps it. `Err` names a failed move or removal after a
/// proven release: the caller keeps the record unreleased
/// (`Quiescence::Unverified` with this text) and the root with every rollout
/// not yet moved, so a later recovery pass or operator release retries.
pub(crate) fn reclaim(
    mac: Option<&MacIdentity>,
    quiescence: Quiescence,
) -> Result<Quiescence, String> {
    let host = crate::provider_data::codex_discovery::codex_root(None);
    let host_sessions = host.map(|root| root.join("sessions"));
    reclaim_into(mac, quiescence, host_sessions.as_deref())
}

/// [`reclaim`] moving the rollouts below `host_sessions`.
pub(crate) fn reclaim_into(
    mac: Option<&MacIdentity>,
    quiescence: Quiescence,
    host_sessions: Option<&Path>,
) -> Result<Quiescence, String> {
    let (Quiescence::Released, Some(mac)) = (&quiescence, mac) else {
        return Ok(quiescence);
    };
    let removal_failed = |error: std::io::Error| {
        format!(
            "no process of the boundary is left, but its private root {} could not be \
             removed ({error}); a release retries the removal",
            mac.scratch_root.display()
        )
    };
    // A root `remove` refuses is never read either.
    invocation_parent(&mac.scratch_root, &mac.invocation).map_err(removal_failed)?;
    rollouts::move_rollouts(&mac.scratch_root, host_sessions).map_err(|error| {
        format!(
            "no process of the boundary is left, but the Codex rollouts in its private \
             root {} could not be moved to the host sessions tree ({error}); the root is \
             kept and a release retries the move",
            mac.scratch_root.display()
        )
    })?;
    remove(&mac.scratch_root, &mac.invocation)
        .map(|()| Quiescence::Released)
        .map_err(removal_failed)
}

/// Opens the absolute, normalized directory `path` after proving every
/// prefix of it, `/` included, a trusted directory by `lstat` alone (see
/// `trusted_directory`): ancestors above the stage cache are often readable
/// only as metadata under a sandbox, so only `path` itself is opened, with
/// `O_NOFOLLOW`, and the opened directory must be the one last checked.
fn walk(path: &Path) -> std::io::Result<OwnedFd> {
    let invalid = || {
        let detail = format!("{} is not an absolute normalized path", path.display());
        std::io::Error::new(std::io::ErrorKind::InvalidInput, detail)
    };
    let mut components = path.components();
    if components.next() != Some(Component::RootDir) {
        return Err(invalid());
    }
    let mut prefix = PathBuf::from("/");
    let mut checked = trusted_directory(&prefix)?;
    for component in components {
        let Component::Normal(name) = component else {
            return Err(invalid());
        };
        prefix.push(name);
        checked = trusted_directory(&prefix)?;
    }
    let flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
    let directory = nix::fcntl::open(path, flags, Mode::empty())?;
    let opened = fstat(&directory)?;
    if (opened.st_dev, opened.st_ino) != (checked.st_dev, checked.st_ino) {
        let detail = format!("{} changed while it was opened", path.display());
        return Err(std::io::Error::other(detail));
    }
    Ok(directory)
}

/// `lstat`s `path` and requires a real directory (never a symlink) owned by
/// root or this user that no other user can rename entries in: neither group
/// nor world writable unless sticky.
fn trusted_directory(path: &Path) -> std::io::Result<libc::stat> {
    let status = lstat(path)?;
    let refused = |why: &str| {
        let detail = format!("{} {why}", path.display());
        std::io::Error::new(std::io::ErrorKind::PermissionDenied, detail)
    };
    match kind(status.st_mode) {
        SFlag::S_IFDIR => {}
        SFlag::S_IFLNK => return Err(refused("is a symlink; the walk never follows one")),
        _ => return Err(refused("is not a directory")),
    }
    // SAFETY: `geteuid` has no preconditions and cannot fail.
    let user = unsafe { libc::geteuid() };
    if status.st_uid != 0 && status.st_uid != user {
        return Err(refused("is owned by another user"));
    }
    let sticky = status.st_mode & libc::S_ISVTX != 0;
    if status.st_mode & 0o022 != 0 && !sticky {
        return Err(refused("is writable by other users"));
    }
    Ok(status)
}

/// Creates `name` under `parent` (0700) and opens it without following a
/// link, requiring this user to own it and closing it to others. With
/// `exclusive`, an existing entry is refused.
#[cfg(any(target_os = "macos", test))]
fn owned_dir(parent: BorrowedFd<'_>, name: &str, exclusive: bool) -> Result<OwnedFd> {
    match mkdirat(parent, name, Mode::S_IRWXU) {
        Ok(()) => {}
        Err(Errno::EEXIST) if !exclusive => {}
        Err(Errno::EEXIST) => bail!("{name} already exists"),
        Err(error) => return Err(error).with_context(|| format!("create {name}")),
    }
    let flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
    let directory = openat(parent, name, flags, Mode::empty())
        .with_context(|| format!("{name} must be a directory, not a symlink"))?;
    let status = fstat(&directory)?;
    // SAFETY: `geteuid` has no preconditions and cannot fail.
    let user = unsafe { libc::geteuid() };
    anyhow::ensure!(status.st_uid == user, "{name} is not owned by this user");
    if status.st_mode & 0o777 != 0o700 {
        fchmod(&directory, Mode::S_IRWXU).with_context(|| format!("close {name}"))?;
    }
    Ok(directory)
}

/// Copies the regular file `name` of `source_dir`, when present, into
/// `target` as a new 0600 file. A symlink or a non-regular entry is refused.
#[cfg(any(target_os = "macos", test))]
fn copy_private(source_dir: &Path, name: &str, target: BorrowedFd<'_>) -> Result<()> {
    let source = source_dir.join(name);
    let read = OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC;
    let file = match nix::fcntl::open(&source, read, Mode::empty()) {
        Ok(file) => file,
        Err(Errno::ENOENT) => return Ok(()),
        Err(Errno::ELOOP) => bail!("{} is a symlink; the boundary refuses it", source.display()),
        Err(error) => return Err(error).with_context(|| format!("open {}", source.display())),
    };
    if kind(fstat(&file)?.st_mode) != SFlag::S_IFREG {
        bail!("{} is not a regular file", source.display());
    }
    let write = OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW;
    let copy = openat(
        target,
        name,
        write | OFlag::O_CLOEXEC,
        Mode::S_IRUSR | Mode::S_IWUSR,
    )
    .with_context(|| format!("create the private {name}"))?;
    fchmod(&copy, Mode::S_IRUSR | Mode::S_IWUSR)?;
    let mut contents = Vec::new();
    std::fs::File::from(file).read_to_end(&mut contents)?;
    std::fs::File::from(copy).write_all(&contents)?;
    Ok(())
}

/// Removes `name` under `parent` without following it; a directory with its
/// contents, refusing (`EXDEV`) any directory on another device than
/// `device`.
fn remove_entry(
    parent: BorrowedFd<'_>,
    name: &OsStr,
    device: libc::dev_t,
    depth: usize,
) -> nix::Result<()> {
    let status = fstatat(parent, name, AtFlags::AT_SYMLINK_NOFOLLOW)?;
    if kind(status.st_mode) != SFlag::S_IFDIR {
        return unlinkat(parent, name, UnlinkatFlags::NoRemoveDir);
    }
    if status.st_dev != device {
        return Err(Errno::EXDEV);
    }
    if depth >= MAX_DEPTH {
        return Err(Errno::ELOOP);
    }
    let mut directory = open_directory(parent, name)?;
    if fstat(&directory)?.st_dev != device {
        return Err(Errno::EXDEV);
    }
    fchmod(&directory, Mode::S_IRWXU)?;
    for child in entries(&mut directory)? {
        match remove_entry(directory.as_fd(), &child, device, depth + 1) {
            Err(Errno::ENOENT) | Ok(()) => {}
            Err(errno) => return Err(errno),
        }
    }
    unlinkat(parent, name, UnlinkatFlags::RemoveDir)
}
