use std::ffi::{CStr, OsStr, OsString};
use std::fs::File;
use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use nix::errno::Errno;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{TerminalOutcome, WaitIdentity, WaitLease, WAIT_SCHEMA_VERSION};
use crate::fs::safe_fs;
use crate::models::forward_receipt::is_safe_id;

pub(super) fn open_lease_dir(
    scratch_root: &Path,
    identity: &WaitIdentity,
) -> Result<(PathBuf, OwnedFd)> {
    validate_identity_ids(identity)?;
    let canonical_root = scratch_root.canonicalize().with_context(|| {
        format!(
            "failed to canonicalize scratch root {}",
            scratch_root.display()
        )
    })?;
    let scratch = safe_fs::safe_open_dirfd(&canonical_root)?;
    let digest = hex::encode(Sha256::digest(
        identity.canonical_repo.as_os_str().as_bytes(),
    ));
    let mut relative = PathBuf::new();
    for component in [
        "loom-subagent-waits",
        &digest[..16],
        &identity.stage_id,
        &identity.loom_session_id,
        &identity.parent_session_id,
        "results",
    ] {
        relative.push(component);
        ensure_component(scratch.as_raw_fd(), &relative, &canonical_root)?;
    }
    relative.pop();
    let root = canonical_root.join(&relative);
    let dirfd = safe_fs::open_safely(
        scratch.as_raw_fd(),
        &relative,
        libc::O_DIRECTORY | libc::O_RDONLY,
        0,
    )?;
    Ok((root, dirfd))
}

fn validate_identity_ids(identity: &WaitIdentity) -> Result<()> {
    for (name, value) in [
        ("stage ID", identity.stage_id.as_str()),
        ("Loom session ID", identity.loom_session_id.as_str()),
        ("parent session ID", identity.parent_session_id.as_str()),
    ] {
        if !is_safe_id(value) {
            bail!("{name} is empty or unsafe");
        }
    }
    Ok(())
}

fn ensure_component(dirfd: RawFd, relative: &Path, scratch_root: &Path) -> Result<()> {
    match safe_fs::safe_create_new_dir_in_workdir(dirfd, relative, 0o700) {
        Ok(()) => Ok(()),
        Err(error) if is_io_kind(&error, std::io::ErrorKind::AlreadyExists) => {
            validate_existing_directory(dirfd, relative, &scratch_root.join(relative))
        }
        Err(error) => Err(error).with_context(|| {
            format!(
                "failed to create lease directory {}",
                scratch_root.join(relative).display()
            )
        }),
    }
}

fn validate_existing_directory(dirfd: RawFd, relative: &Path, display: &Path) -> Result<()> {
    let fd = match safe_fs::open_safely(dirfd, relative, libc::O_DIRECTORY | libc::O_RDONLY, 0) {
        Ok(fd) => fd,
        Err(error) if is_symlink_refusal(display, &error) => {
            bail!(
                "refusing symlink directory component: {}",
                display.display()
            );
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("refusing unsafe lease directory {}", display.display()));
        }
    };
    let metadata = File::from(fd)
        .metadata()
        .with_context(|| format!("failed to inspect lease directory {}", display.display()))?;
    // SAFETY: `geteuid` takes no arguments and has no memory-safety preconditions.
    if metadata.uid() != unsafe { libc::geteuid() } {
        bail!("lease directory has another owner: {}", display.display());
    }
    if metadata.mode() & 0o022 != 0 {
        bail!(
            "lease directory is group- or other-writable: {}",
            display.display()
        );
    }
    Ok(())
}

fn is_symlink_refusal(display: &Path, error: &anyhow::Error) -> bool {
    let loop_error = error
        .chain()
        .filter_map(|cause| cause.downcast_ref::<std::io::Error>())
        .any(|error| error.raw_os_error() == Some(libc::ELOOP));
    loop_error
        || std::fs::symlink_metadata(display)
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
}

fn is_io_kind(error: &anyhow::Error, kind: std::io::ErrorKind) -> bool {
    error
        .chain()
        .find_map(|cause| cause.downcast_ref::<std::io::Error>())
        .is_some_and(|error| error.kind() == kind)
}

/// Locks the stable `results` directory opened beneath the retained lease dirfd.
/// The returned descriptor holds the lock until it is dropped.
pub(super) fn lock(dirfd: RawFd, root: &Path) -> Result<OwnedFd> {
    let fd = safe_fs::open_safely(
        dirfd,
        Path::new("results"),
        libc::O_DIRECTORY | libc::O_RDONLY,
        0,
    )?;
    safe_fs::flock_exclusive(&fd)
        .with_context(|| format!("failed to lock {}", root.join("results").display()))?;
    Ok(fd)
}

pub(super) fn read_active(dirfd: RawFd, root: &Path) -> Result<Option<WaitLease>> {
    let relative = Path::new("lease.json");
    match read_lease(dirfd, relative, &root.join(relative)) {
        Ok(lease) => Ok(Some(lease)),
        Err(error) if is_io_kind(&error, std::io::ErrorKind::NotFound) => Ok(None),
        Err(error) => Err(error),
    }
}

fn read_lease(dirfd: RawFd, relative: &Path, display: &Path) -> Result<WaitLease> {
    let fd = safe_fs::open_safely(dirfd, relative, libc::O_RDONLY, 0)
        .with_context(|| format!("failed to open wait lease {}", display.display()))?;
    let mut file = File::from(fd);
    if !file.metadata()?.is_file() {
        bail!("wait lease is not a regular file: {}", display.display());
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .with_context(|| format!("failed to read wait lease {}", display.display()))?;
    let lease: WaitLease = serde_json::from_slice(&bytes)
        .with_context(|| format!("malformed wait lease {}", display.display()))?;
    validate_lease(display, &lease)?;
    Ok(lease)
}

fn validate_lease(path: &Path, lease: &WaitLease) -> Result<()> {
    if lease.schema_version != WAIT_SCHEMA_VERSION {
        bail!("unsupported wait lease schema in {}", path.display());
    }
    if !is_safe_id(&lease.wait_id) {
        bail!("unsafe wait ID in {}", path.display());
    }
    Ok(())
}

pub(super) fn write_active(dirfd: RawFd, root: &Path, lease: &WaitLease) -> Result<()> {
    let relative = Path::new("lease.json");
    validate_lease(&root.join(relative), lease)?;
    write_atomic(dirfd, relative, &root.join(relative), lease)
}

pub(super) fn remove_active(dirfd: RawFd, root: &Path) -> Result<()> {
    safe_fs::safe_remove_in_workdir(dirfd, Path::new("lease.json")).with_context(|| {
        format!(
            "failed to remove active lease {}",
            root.join("lease.json").display()
        )
    })
}

pub(super) fn write_result(
    dirfd: RawFd,
    root: &Path,
    lease: &WaitLease,
    outcome: TerminalOutcome,
    finished_unix_secs: u64,
) -> Result<()> {
    let relative = PathBuf::from("results").join(format!("{}.json", lease.wait_id));
    let display = root.join(&relative);
    validate_lease(&display, lease)?;
    let mut result = lease.clone();
    result.terminal_result = Some(outcome);
    result.finished_unix_secs = Some(finished_unix_secs);
    write_atomic(dirfd, &relative, &display, &result)
}

fn write_atomic(dirfd: RawFd, target: &Path, display: &Path, lease: &WaitLease) -> Result<()> {
    validate_target(dirfd, target, display)?;
    let name = target
        .file_name()
        .context("wait lease target has no file name")?;
    let temporary = target.with_file_name(format!(
        ".{}.{}.tmp",
        name.to_string_lossy(),
        Uuid::new_v4()
    ));
    let bytes = serde_json::to_vec_pretty(lease).context("failed to encode wait lease")?;
    let result = replace_from_temporary(dirfd, &temporary, target, display, &bytes);
    if let Err(error) = result {
        let _ = safe_fs::safe_remove_in_workdir(dirfd, &temporary);
        return Err(error);
    }
    sync_parent(dirfd, target, display).with_context(|| {
        format!(
            "target was replaced but its directory sync failed: {}",
            display.display()
        )
    })
}

fn replace_from_temporary(
    dirfd: RawFd,
    temporary: &Path,
    target: &Path,
    display: &Path,
    bytes: &[u8],
) -> Result<()> {
    safe_fs::safe_create_new_in_workdir(dirfd, temporary, bytes)?;
    let temp = safe_fs::open_safely(dirfd, temporary, libc::O_RDONLY, 0)?;
    File::from(temp)
        .sync_all()
        .with_context(|| format!("failed to sync temporary lease for {}", display.display()))?;
    validate_target(dirfd, target, display)?;
    safe_fs::safe_rename_in_workdir(dirfd, temporary, target)?;
    Ok(())
}

fn validate_target(dirfd: RawFd, target: &Path, display: &Path) -> Result<()> {
    match safe_fs::open_safely(dirfd, target, libc::O_RDONLY, 0) {
        Ok(fd) => {
            if !File::from(fd).metadata()?.is_file() {
                bail!("target is not a regular file: {}", display.display());
            }
            Ok(())
        }
        Err(error) if is_io_kind(&error, std::io::ErrorKind::NotFound) => Ok(()),
        Err(error) => Err(error)
            .with_context(|| format!("refusing unsafe wait lease file {}", display.display())),
    }
}

fn sync_parent(dirfd: RawFd, target: &Path, display: &Path) -> Result<()> {
    let parent = target.parent().filter(|path| !path.as_os_str().is_empty());
    let parent = match parent {
        Some(path) => File::from(safe_fs::open_safely(
            dirfd,
            path,
            libc::O_DIRECTORY | libc::O_RDONLY,
            0,
        )?),
        None => {
            // SAFETY: `dirfd` is owned by the caller's live `LeaseDir` and
            // remains valid for the duration of this call.
            let duplicate = unsafe { libc::dup(dirfd) };
            if duplicate < 0 {
                return Err(std::io::Error::last_os_error())
                    .context("failed to duplicate lease dirfd");
            }
            // SAFETY: successful `dup` returned a fresh descriptor owned here;
            // ownership is transferred exactly once to `OwnedFd`.
            File::from(unsafe { OwnedFd::from_raw_fd(duplicate) })
        }
    };
    parent
        .sync_all()
        .with_context(|| format!("failed to sync lease directory for {}", display.display()))
}

pub(super) fn prune(dirfd: RawFd, root: &Path, now: u64, retention: u64) -> Result<usize> {
    let results_fd = safe_fs::open_safely(
        dirfd,
        Path::new("results"),
        libc::O_DIRECTORY | libc::O_RDONLY,
        0,
    )?;
    let mut removed = 0;
    for name in read_entry_names(results_fd)? {
        if name.as_os_str() == OsStr::new(".")
            || name.as_os_str() == OsStr::new("..")
            || Path::new(&name).extension() != Some(OsStr::new("json"))
        {
            continue;
        }
        let relative = PathBuf::from("results").join(name);
        let display = root.join(&relative);
        if should_prune(dirfd, &relative, &display, now, retention)
            && safe_fs::safe_remove_in_workdir(dirfd, &relative).is_ok()
        {
            removed += 1;
        }
    }
    Ok(removed)
}

struct OpenDirectory(*mut libc::DIR);

impl Drop for OpenDirectory {
    fn drop(&mut self) {
        // SAFETY: `self.0` came from `fdopendir` and is closed exactly once here.
        unsafe { libc::closedir(self.0) };
    }
}

fn read_entry_names(fd: OwnedFd) -> Result<Vec<OsString>> {
    let raw_fd = fd.into_raw_fd();
    // SAFETY: `raw_fd` is an owned, valid directory descriptor; on success,
    // ownership transfers to the returned `DIR` handle.
    let pointer = unsafe { libc::fdopendir(raw_fd) };
    if pointer.is_null() {
        let error = std::io::Error::last_os_error();
        // SAFETY: `fdopendir` failed, so ownership of `raw_fd` was not transferred.
        unsafe { libc::close(raw_fd) };
        return Err(error).context("failed to open wait results directory");
    }
    let directory = OpenDirectory(pointer);
    let mut names = Vec::new();
    loop {
        Errno::clear();
        // SAFETY: `directory.0` is non-null and owned by the live `OpenDirectory`;
        // no other operation accesses the `DIR` handle during this call.
        let entry = unsafe { libc::readdir(directory.0) };
        if entry.is_null() {
            let errno = Errno::last_raw();
            if errno != 0 {
                return Err(std::io::Error::from_raw_os_error(errno))
                    .context("failed to read wait results directory");
            }
            break;
        }
        // SAFETY: `readdir` returned a live entry whose `d_name` is NUL-terminated.
        let bytes = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
        names.push(OsString::from_vec(bytes.to_vec()));
    }
    Ok(names)
}

fn should_prune(dirfd: RawFd, relative: &Path, display: &Path, now: u64, retention: u64) -> bool {
    let Ok(lease) = read_lease(dirfd, relative, display) else {
        return false;
    };
    let Some(finished) = lease
        .finished_unix_secs
        .filter(|_| lease.terminal_result.is_some())
    else {
        return false;
    };
    now.checked_sub(finished).is_some_and(|age| age > retention)
}
